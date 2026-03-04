use reqwest::Client;
use std::net::IpAddr;
use std::time::Duration;
use tracing::{info, warn};
use url::Url;

use super::store::{Job, JobResult, JobStatus, JobType};

const MAX_RETRIES: u32 = 3;
const INITIAL_BACKOFF_SECS: u64 = 1;
const WEBHOOK_TIMEOUT_SECS: u64 = 30;

// ---------------------------------------------------------------------------
// SSRF Validation
// ---------------------------------------------------------------------------

/// Validate a webhook URL to prevent SSRF attacks.
///
/// Rejects:
/// - Non-HTTP(S) schemes
/// - localhost / 127.x.x.x / ::1
/// - Private IP ranges (10.x, 172.16-31.x, 192.168.x)
/// - Link-local (169.254.x.x, fe80::)
/// - Cloud metadata endpoints (169.254.169.254)
pub fn validate_webhook_url(url_str: &str) -> Result<(), String> {
    let parsed = Url::parse(url_str).map_err(|e| format!("Invalid webhook URL: {}", e))?;

    // Only allow HTTP(S)
    match parsed.scheme() {
        "http" | "https" => {}
        scheme => return Err(format!("Webhook URL scheme '{}' not allowed, must be http or https", scheme)),
    }

    let host = parsed
        .host_str()
        .ok_or_else(|| "Webhook URL has no host".to_string())?;

    // Block localhost
    if host == "localhost" || host == "127.0.0.1" || host == "::1" || host == "[::1]" {
        return Err("Webhook URL cannot point to localhost".to_string());
    }

    // Block cloud metadata
    if host == "169.254.169.254" || host == "metadata.google.internal" {
        return Err("Webhook URL cannot point to cloud metadata endpoint".to_string());
    }

    // Try to parse as IP and check private ranges
    if let Ok(ip) = host.parse::<IpAddr>() {
        if is_private_ip(&ip) {
            return Err(format!("Webhook URL cannot point to private IP: {}", ip));
        }
    }

    // Also handle bracket-stripped IPv6
    let stripped = host.trim_start_matches('[').trim_end_matches(']');
    if let Ok(ip) = stripped.parse::<IpAddr>() {
        if is_private_ip(&ip) {
            return Err(format!("Webhook URL cannot point to private IP: {}", ip));
        }
    }

    Ok(())
}

fn is_private_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()            // 127.0.0.0/8
            || v4.is_private()          // 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16
            || v4.is_link_local()       // 169.254.0.0/16
            || v4.is_unspecified()      // 0.0.0.0
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()            // ::1
            || v6.is_unspecified()      // ::
            // fe80::/10 link-local — check manually since is_unicast_link_local is unstable
            || (v6.segments()[0] & 0xffc0) == 0xfe80
        }
    }
}

// ---------------------------------------------------------------------------
// HMAC Signing
// ---------------------------------------------------------------------------

/// Compute an HMAC-like signature using blake3 keyed hash.
///
/// The secret is first hashed to derive a 32-byte key (blake3 keyed_hash
/// requires exactly 32 bytes). Returns `"blake3=<hex>"`.
pub fn compute_signature(payload_bytes: &[u8], secret: &str) -> String {
    let key = blake3::hash(secret.as_bytes());
    let sig = blake3::keyed_hash(key.as_bytes(), payload_bytes);
    format!("blake3={}", sig.to_hex())
}

// ---------------------------------------------------------------------------
// Shared delivery helper
// ---------------------------------------------------------------------------

/// Fire-and-forget delivery of an arbitrary JSON payload to a webhook URL.
///
/// Retries up to `MAX_RETRIES` with exponential backoff. Optionally signs
/// the payload with an HMAC if `secret` is provided.
pub fn deliver_payload(
    webhook_url: String,
    event_type: String,
    id: String,
    payload: serde_json::Value,
    secret: Option<String>,
) {
    tokio::spawn(async move {
        let client = match Client::builder()
            .timeout(Duration::from_secs(WEBHOOK_TIMEOUT_SECS))
            .build()
        {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to create webhook client for {}: {}", id, e);
                return;
            }
        };

        // Pre-serialize payload for HMAC
        let payload_bytes = match serde_json::to_vec(&payload) {
            Ok(b) => b,
            Err(e) => {
                warn!("Failed to serialize webhook payload for {}: {}", id, e);
                return;
            }
        };

        let signature = secret.as_deref().map(|s| compute_signature(&payload_bytes, s));

        for attempt in 0..MAX_RETRIES {
            if attempt > 0 {
                let backoff = Duration::from_secs(INITIAL_BACKOFF_SECS * 5u64.pow(attempt));
                info!(
                    "Webhook retry {} for {} (waiting {}s)",
                    attempt,
                    id,
                    backoff.as_secs()
                );
                tokio::time::sleep(backoff).await;
            }

            let mut req = client
                .post(&webhook_url)
                .header("Content-Type", "application/json")
                .header("X-Essence-Event", &event_type)
                .header("X-Essence-Crawl-Id", &id)
                .header("User-Agent", "Essence/0.1.0");

            if let Some(ref sig) = signature {
                req = req.header("X-Essence-Signature", sig.as_str());
            }

            match req.body(payload_bytes.clone()).send().await {
                Ok(response) if response.status().is_success() => {
                    info!(
                        "Webhook delivered for {} -> {} ({})",
                        id,
                        webhook_url,
                        response.status()
                    );
                    return;
                }
                Ok(response) => {
                    warn!(
                        "Webhook returned {} for {} -> {}",
                        response.status(),
                        id,
                        webhook_url
                    );
                }
                Err(e) => {
                    warn!(
                        "Webhook failed for {} -> {}: {}",
                        id, webhook_url, e
                    );
                }
            }
        }

        warn!(
            "Webhook delivery exhausted retries for {} -> {}",
            id, webhook_url
        );
    });
}

// ---------------------------------------------------------------------------
// Job-based delivery (for async crawl / batch scrape)
// ---------------------------------------------------------------------------

/// Deliver a webhook notification for a completed/failed job.
/// This is fire-and-forget — spawns a background task.
pub fn deliver_webhook(job: &Job) {
    let webhook_url = match &job.webhook_url {
        Some(url) => url.clone(),
        None => return,
    };

    // Only deliver for terminal states
    let event_type = match (&job.job_type, &job.status) {
        (JobType::AsyncCrawl, JobStatus::Completed) => "crawl.completed",
        (JobType::AsyncCrawl, JobStatus::Failed) => "crawl.failed",
        (JobType::BatchScrape, JobStatus::Completed) => "batch_scrape.completed",
        (JobType::BatchScrape, JobStatus::Failed) => "batch_scrape.failed",
        _ => return,
    };

    let mut payload = build_webhook_payload(job);

    // Enrich with request context and metadata
    if job.request != serde_json::json!({}) {
        payload["request"] = job.request.clone();
    }
    if let Some(ref metadata) = job.webhook_metadata {
        payload["metadata"] = metadata.clone();
    }

    deliver_payload(
        webhook_url,
        event_type.to_string(),
        job.id.clone(),
        payload,
        job.webhook_secret.clone(),
    );
}

// ---------------------------------------------------------------------------
// Sync crawl webhook delivery
// ---------------------------------------------------------------------------

/// Deliver a webhook for a synchronous crawl completion.
/// Called from crawl_handler after a successful crawl.
pub fn deliver_sync_crawl_webhook(
    webhook_url: String,
    documents: &[crate::types::Document],
    request: &serde_json::Value,
    secret: Option<String>,
    metadata: Option<serde_json::Value>,
) {
    let mut payload = serde_json::json!({
        "event": "crawl.completed",
        "status": "completed",
        "data": documents,
        "totalPages": documents.len(),
        "completedAt": chrono::Utc::now(),
    });

    if *request != serde_json::json!({}) {
        payload["request"] = request.clone();
    }
    if let Some(meta) = metadata {
        payload["metadata"] = meta;
    }

    let crawl_id = format!("sync-{}", uuid::Uuid::new_v4());
    deliver_payload(webhook_url, "crawl.completed".to_string(), crawl_id, payload, secret);
}

/// Deliver a webhook for a synchronous crawl error.
pub fn deliver_sync_crawl_error_webhook(
    webhook_url: String,
    error: &str,
    request: &serde_json::Value,
    secret: Option<String>,
    metadata: Option<serde_json::Value>,
) {
    let mut payload = serde_json::json!({
        "event": "crawl.failed",
        "status": "failed",
        "error": error,
        "failedAt": chrono::Utc::now(),
    });

    if *request != serde_json::json!({}) {
        payload["request"] = request.clone();
    }
    if let Some(meta) = metadata {
        payload["metadata"] = meta;
    }

    let crawl_id = format!("sync-{}", uuid::Uuid::new_v4());
    deliver_payload(webhook_url, "crawl.failed".to_string(), crawl_id, payload, secret);
}

// ---------------------------------------------------------------------------
// Payload builder
// ---------------------------------------------------------------------------

fn build_webhook_payload(job: &Job) -> serde_json::Value {
    let mut payload = serde_json::json!({
        "status": job.status,
    });

    match job.job_type {
        JobType::AsyncCrawl => {
            payload["crawlId"] = serde_json::json!(job.id);
            match job.status {
                JobStatus::Completed => {
                    payload["event"] = serde_json::json!("crawl.completed")
                }
                JobStatus::Failed => payload["event"] = serde_json::json!("crawl.failed"),
                _ => {}
            }
        }
        JobType::BatchScrape => {
            payload["batchId"] = serde_json::json!(job.id);
            match job.status {
                JobStatus::Completed => {
                    payload["event"] = serde_json::json!("batch_scrape.completed")
                }
                JobStatus::Failed => {
                    payload["event"] = serde_json::json!("batch_scrape.failed")
                }
                _ => {}
            }
        }
    }

    match &job.result {
        Some(JobResult::CrawlResult(docs)) => {
            payload["data"] = serde_json::to_value(docs).unwrap_or_default();
            payload["totalPages"] = serde_json::json!(docs.len());
            payload["completedAt"] = serde_json::json!(job.updated_at);
        }
        Some(JobResult::BatchScrapeResult(items)) => {
            let success_count = items.iter().filter(|i| i.success).count();
            payload["data"] = serde_json::to_value(items).unwrap_or_default();
            payload["totalUrls"] = serde_json::json!(items.len());
            payload["successCount"] = serde_json::json!(success_count);
            payload["errorCount"] = serde_json::json!(items.len() - success_count);
            payload["completedAt"] = serde_json::json!(job.updated_at);
        }
        None => {}
    }

    if let Some(ref error) = job.error {
        payload["error"] = serde_json::json!(error);
        payload["failedAt"] = serde_json::json!(job.updated_at);
    }

    payload
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::store::{JobProgress, JobType};
    use chrono::Utc;

    fn make_test_job(job_type: JobType, status: JobStatus) -> Job {
        Job {
            id: "test-123".to_string(),
            job_type,
            status,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            progress: JobProgress::default(),
            result: None,
            error: None,
            webhook_url: Some("https://example.com/webhook".to_string()),
            webhook_secret: None,
            webhook_metadata: None,
            request: serde_json::json!({}),
        }
    }

    #[test]
    fn test_payload_completed_crawl() {
        let mut job = make_test_job(JobType::AsyncCrawl, JobStatus::Completed);
        job.result = Some(JobResult::CrawlResult(vec![]));

        let payload = build_webhook_payload(&job);
        assert_eq!(payload["event"], "crawl.completed");
        assert_eq!(payload["crawlId"], "test-123");
        assert_eq!(payload["status"], "completed");
        assert_eq!(payload["totalPages"], 0);
    }

    #[test]
    fn test_payload_failed_crawl() {
        let mut job = make_test_job(JobType::AsyncCrawl, JobStatus::Failed);
        job.error = Some("Connection timeout".to_string());

        let payload = build_webhook_payload(&job);
        assert_eq!(payload["event"], "crawl.failed");
        assert_eq!(payload["error"], "Connection timeout");
        assert!(payload.get("failedAt").is_some());
    }

    #[test]
    fn test_payload_completed_batch() {
        use crate::jobs::store::BatchScrapeItem;

        let mut job = make_test_job(JobType::BatchScrape, JobStatus::Completed);
        job.result = Some(JobResult::BatchScrapeResult(vec![
            BatchScrapeItem {
                url: "https://example.com".to_string(),
                success: true,
                data: None,
                error: None,
            },
            BatchScrapeItem {
                url: "https://broken.com".to_string(),
                success: false,
                data: None,
                error: Some("timeout".to_string()),
            },
        ]));

        let payload = build_webhook_payload(&job);
        assert_eq!(payload["event"], "batch_scrape.completed");
        assert_eq!(payload["batchId"], "test-123");
        assert_eq!(payload["totalUrls"], 2);
        assert_eq!(payload["successCount"], 1);
        assert_eq!(payload["errorCount"], 1);
    }

    #[test]
    fn test_deliver_skips_when_no_webhook() {
        let mut job = make_test_job(JobType::AsyncCrawl, JobStatus::Completed);
        job.webhook_url = None;
        // Should not panic or do anything
        deliver_webhook(&job);
    }

    #[test]
    fn test_deliver_skips_non_terminal() {
        let job = make_test_job(JobType::AsyncCrawl, JobStatus::Running);
        // Should not panic — running is not a terminal state
        deliver_webhook(&job);
    }

    // --- SSRF Validation tests ---

    #[test]
    fn test_validate_webhook_url_valid() {
        assert!(validate_webhook_url("https://example.com/webhook").is_ok());
        assert!(validate_webhook_url("http://hooks.example.com/cb").is_ok());
    }

    #[test]
    fn test_validate_webhook_url_rejects_non_http() {
        assert!(validate_webhook_url("ftp://example.com/file").is_err());
        assert!(validate_webhook_url("file:///etc/passwd").is_err());
    }

    #[test]
    fn test_validate_webhook_url_rejects_localhost() {
        assert!(validate_webhook_url("http://localhost/webhook").is_err());
        assert!(validate_webhook_url("http://127.0.0.1/webhook").is_err());
        assert!(validate_webhook_url("http://[::1]/webhook").is_err());
    }

    #[test]
    fn test_validate_webhook_url_rejects_private_ip() {
        assert!(validate_webhook_url("http://10.0.0.1/webhook").is_err());
        assert!(validate_webhook_url("http://172.16.0.1/webhook").is_err());
        assert!(validate_webhook_url("http://192.168.1.1/webhook").is_err());
    }

    #[test]
    fn test_validate_webhook_url_rejects_metadata() {
        assert!(validate_webhook_url("http://169.254.169.254/latest/meta-data").is_err());
        assert!(validate_webhook_url("http://metadata.google.internal/computeMetadata").is_err());
    }

    // --- Signature tests ---

    #[test]
    fn test_compute_signature_deterministic() {
        let payload = b"hello world";
        let sig1 = compute_signature(payload, "secret");
        let sig2 = compute_signature(payload, "secret");
        assert_eq!(sig1, sig2);
        assert!(sig1.starts_with("blake3="));
    }

    #[test]
    fn test_compute_signature_different_secrets() {
        let payload = b"hello world";
        let sig1 = compute_signature(payload, "secret1");
        let sig2 = compute_signature(payload, "secret2");
        assert_ne!(sig1, sig2);
    }

    // --- Enriched payload tests ---

    #[test]
    fn test_payload_includes_metadata() {
        let mut job = make_test_job(JobType::AsyncCrawl, JobStatus::Completed);
        job.result = Some(JobResult::CrawlResult(vec![]));
        job.webhook_metadata = Some(serde_json::json!({"user_id": "abc"}));
        job.request = serde_json::json!({"url": "https://example.com"});

        // deliver_webhook builds enriched payload internally, so test via build_webhook_payload + enrichment
        let mut payload = build_webhook_payload(&job);
        // Simulate the enrichment done in deliver_webhook
        if job.request != serde_json::json!({}) {
            payload["request"] = job.request.clone();
        }
        if let Some(ref metadata) = job.webhook_metadata {
            payload["metadata"] = metadata.clone();
        }

        assert_eq!(payload["metadata"]["user_id"], "abc");
        assert_eq!(payload["request"]["url"], "https://example.com");
    }
}
