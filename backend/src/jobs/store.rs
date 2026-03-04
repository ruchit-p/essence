use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, info};

use crate::types::Document;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl JobStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(self, JobStatus::Completed | JobStatus::Failed | JobStatus::Cancelled)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum JobType {
    AsyncCrawl,
    BatchScrape,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobProgress {
    pub completed: usize,
    pub total: Option<usize>,
    pub current_url: Option<String>,
    pub percent: Option<u8>,
}


#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum JobResult {
    CrawlResult(Vec<Document>),
    BatchScrapeResult(Vec<BatchScrapeItem>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchScrapeItem {
    pub url: String,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Document>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: String,
    pub job_type: JobType,
    pub status: JobStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub progress: JobProgress,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<JobResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip)]
    pub webhook_url: Option<String>,
    #[serde(skip)]
    pub webhook_secret: Option<String>,
    #[serde(skip)]
    pub webhook_metadata: Option<serde_json::Value>,
    #[serde(skip)]
    pub request: serde_json::Value,
}

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct JobStoreConfig {
    pub max_jobs: usize,
    pub result_ttl_secs: u64,
    pub cleanup_interval_secs: u64,
}

impl Default for JobStoreConfig {
    fn default() -> Self {
        Self {
            max_jobs: std::env::var("MAX_STORED_JOBS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(1000),
            result_ttl_secs: std::env::var("JOB_RESULT_TTL_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(3600),
            cleanup_interval_secs: std::env::var("JOB_CLEANUP_INTERVAL_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(300),
        }
    }
}

// ---------------------------------------------------------------------------
// JobStore
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct JobStore {
    jobs: Arc<DashMap<String, Job>>,
    config: JobStoreConfig,
}

impl JobStore {
    pub fn new(config: JobStoreConfig) -> Self {
        Self {
            jobs: Arc::new(DashMap::new()),
            config,
        }
    }

    /// Spawn background cleanup task that evicts expired jobs.
    pub fn start_cleanup_task(&self) {
        let store = self.clone();
        let interval = self.config.cleanup_interval_secs;
        let ttl = self.config.result_ttl_secs;

        tokio::spawn(async move {
            let mut ticker =
                tokio::time::interval(std::time::Duration::from_secs(interval));
            loop {
                ticker.tick().await;
                let now = Utc::now();
                let cutoff = now - chrono::Duration::seconds(ttl as i64);
                let before = store.jobs.len();
                store.jobs.retain(|_, job| job.updated_at > cutoff);
                let removed = before.saturating_sub(store.jobs.len());
                if removed > 0 {
                    info!("Job cleanup: removed {} expired jobs", removed);
                }
            }
        });
    }

    // ---- CRUD ----

    /// Create a new job and return its ID.
    pub fn create_job(
        &self,
        job_type: JobType,
        request: serde_json::Value,
        webhook_url: Option<String>,
        webhook_secret: Option<String>,
        webhook_metadata: Option<serde_json::Value>,
    ) -> Result<String, String> {
        if self.jobs.len() >= self.config.max_jobs {
            return Err("Job store full: too many concurrent jobs".to_string());
        }

        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now();

        let job = Job {
            id: id.clone(),
            job_type,
            status: JobStatus::Queued,
            created_at: now,
            updated_at: now,
            progress: JobProgress::default(),
            result: None,
            error: None,
            webhook_url,
            webhook_secret,
            webhook_metadata,
            request,
        };

        self.jobs.insert(id.clone(), job);
        debug!("Created job {}", id);
        Ok(id)
    }

    /// Get a clone of a job by ID.
    pub fn get_job(&self, id: &str) -> Option<Job> {
        self.jobs.get(id).map(|entry| entry.clone())
    }

    /// Update the status of a job. Terminal states are immutable.
    pub fn update_status(&self, id: &str, status: JobStatus) -> Result<(), String> {
        let mut entry = self
            .jobs
            .get_mut(id)
            .ok_or_else(|| format!("Job {} not found", id))?;

        if entry.status.is_terminal() {
            return Err(format!(
                "Cannot update job {}: already in terminal state {:?}",
                id, entry.status
            ));
        }

        entry.status = status;
        entry.updated_at = Utc::now();
        Ok(())
    }

    /// Update progress for a running job.
    pub fn update_progress(&self, id: &str, progress: JobProgress) -> Result<(), String> {
        let mut entry = self
            .jobs
            .get_mut(id)
            .ok_or_else(|| format!("Job {} not found", id))?;

        if entry.status != JobStatus::Running {
            return Err(format!(
                "Cannot update progress for job {}: status is {:?}",
                id, entry.status
            ));
        }

        entry.progress = progress;
        entry.updated_at = Utc::now();
        Ok(())
    }

    /// Set the result and mark the job as Completed.
    /// Returns the webhook_url if set (for webhook delivery).
    pub fn set_result(&self, id: &str, result: JobResult) -> Result<Option<String>, String> {
        {
            let mut entry = self
                .jobs
                .get_mut(id)
                .ok_or_else(|| format!("Job {} not found", id))?;

            if entry.status.is_terminal() {
                return Err(format!(
                    "Cannot set result for job {}: already in terminal state {:?}",
                    id, entry.status
                ));
            }

            entry.result = Some(result);
            entry.status = JobStatus::Completed;
            entry.updated_at = Utc::now();
        }

        // Trigger webhook delivery if configured
        if let Some(job) = self.get_job(id) {
            super::webhook::deliver_webhook(&job);
            return Ok(job.webhook_url);
        }

        Ok(None)
    }

    /// Set an error message and mark the job as Failed.
    /// Returns the webhook_url if set (for webhook delivery).
    pub fn set_error(&self, id: &str, error_msg: String) -> Result<Option<String>, String> {
        {
            let mut entry = self
                .jobs
                .get_mut(id)
                .ok_or_else(|| format!("Job {} not found", id))?;

            if entry.status.is_terminal() {
                return Err(format!(
                    "Cannot set error for job {}: already in terminal state {:?}",
                    id, entry.status
                ));
            }

            entry.error = Some(error_msg);
            entry.status = JobStatus::Failed;
            entry.updated_at = Utc::now();
        }

        // Trigger webhook delivery if configured
        if let Some(job) = self.get_job(id) {
            super::webhook::deliver_webhook(&job);
            return Ok(job.webhook_url);
        }

        Ok(None)
    }

    /// Cancel a job. Only valid for Queued or Running jobs.
    pub fn cancel_job(&self, id: &str) -> Result<(), String> {
        let mut entry = self
            .jobs
            .get_mut(id)
            .ok_or_else(|| format!("Job {} not found", id))?;

        if entry.status.is_terminal() {
            return Err(format!(
                "Cannot cancel job {}: already in terminal state {:?}",
                id, entry.status
            ));
        }

        entry.status = JobStatus::Cancelled;
        entry.updated_at = Utc::now();
        Ok(())
    }

    /// List jobs with optional filters and limit.
    pub fn list_jobs(
        &self,
        job_type: Option<&JobType>,
        status: Option<&JobStatus>,
        limit: usize,
    ) -> Vec<Job> {
        let mut jobs: Vec<Job> = self
            .jobs
            .iter()
            .filter(|entry| {
                let job = entry.value();
                let type_match = job_type.is_none_or(|t| &job.job_type == t);
                let status_match = status.is_none_or(|s| &job.status == s);
                type_match && status_match
            })
            .map(|entry| entry.value().clone())
            .collect();

        // Sort by created_at descending (newest first)
        jobs.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        jobs.truncate(limit);
        jobs
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> JobStoreConfig {
        JobStoreConfig {
            max_jobs: 5,
            result_ttl_secs: 2,
            cleanup_interval_secs: 1,
        }
    }

    #[test]
    fn test_create_job_returns_unique_ids() {
        let store = JobStore::new(test_config());
        let id1 = store
            .create_job(JobType::AsyncCrawl, serde_json::json!({}), None, None, None)
            .unwrap();
        let id2 = store
            .create_job(JobType::AsyncCrawl, serde_json::json!({}), None, None, None)
            .unwrap();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_create_job_rejects_when_full() {
        let store = JobStore::new(JobStoreConfig {
            max_jobs: 2,
            ..test_config()
        });
        store
            .create_job(JobType::AsyncCrawl, serde_json::json!({}), None, None, None)
            .unwrap();
        store
            .create_job(JobType::AsyncCrawl, serde_json::json!({}), None, None, None)
            .unwrap();
        let result = store.create_job(JobType::AsyncCrawl, serde_json::json!({}), None, None, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("full"));
    }

    #[test]
    fn test_get_job_none_for_missing() {
        let store = JobStore::new(test_config());
        assert!(store.get_job("nonexistent").is_none());
    }

    #[test]
    fn test_get_job_returns_correct_job() {
        let store = JobStore::new(test_config());
        let id = store
            .create_job(
                JobType::BatchScrape,
                serde_json::json!({"url": "test"}),
                None,
                None,
                None,
            )
            .unwrap();
        let job = store.get_job(&id).unwrap();
        assert_eq!(job.id, id);
        assert_eq!(job.job_type, JobType::BatchScrape);
        assert_eq!(job.status, JobStatus::Queued);
    }

    #[test]
    fn test_status_transitions() {
        let store = JobStore::new(test_config());
        let id = store
            .create_job(JobType::AsyncCrawl, serde_json::json!({}), None, None, None)
            .unwrap();

        // Queued -> Running
        store.update_status(&id, JobStatus::Running).unwrap();
        assert_eq!(store.get_job(&id).unwrap().status, JobStatus::Running);

        // Running -> Completed
        store.update_status(&id, JobStatus::Completed).unwrap();
        assert_eq!(store.get_job(&id).unwrap().status, JobStatus::Completed);
    }

    #[test]
    fn test_terminal_state_immutable() {
        let store = JobStore::new(test_config());
        let id = store
            .create_job(JobType::AsyncCrawl, serde_json::json!({}), None, None, None)
            .unwrap();

        store.update_status(&id, JobStatus::Running).unwrap();
        store.update_status(&id, JobStatus::Completed).unwrap();

        // Completed -> Running should fail
        let result = store.update_status(&id, JobStatus::Running);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("terminal"));
    }

    #[test]
    fn test_set_result() {
        let store = JobStore::new(test_config());
        let id = store
            .create_job(JobType::AsyncCrawl, serde_json::json!({}), None, None, None)
            .unwrap();
        store.update_status(&id, JobStatus::Running).unwrap();

        let result = JobResult::CrawlResult(vec![]);
        store.set_result(&id, result).unwrap();

        let job = store.get_job(&id).unwrap();
        assert_eq!(job.status, JobStatus::Completed);
        assert!(job.result.is_some());
    }

    #[test]
    fn test_set_error() {
        let store = JobStore::new(test_config());
        let id = store
            .create_job(JobType::AsyncCrawl, serde_json::json!({}), None, None, None)
            .unwrap();
        store.update_status(&id, JobStatus::Running).unwrap();

        store.set_error(&id, "something went wrong".to_string()).unwrap();

        let job = store.get_job(&id).unwrap();
        assert_eq!(job.status, JobStatus::Failed);
        assert_eq!(job.error.unwrap(), "something went wrong");
    }

    #[test]
    fn test_cancel_queued() {
        let store = JobStore::new(test_config());
        let id = store
            .create_job(JobType::AsyncCrawl, serde_json::json!({}), None, None, None)
            .unwrap();
        store.cancel_job(&id).unwrap();
        assert_eq!(store.get_job(&id).unwrap().status, JobStatus::Cancelled);
    }

    #[test]
    fn test_cancel_running() {
        let store = JobStore::new(test_config());
        let id = store
            .create_job(JobType::AsyncCrawl, serde_json::json!({}), None, None, None)
            .unwrap();
        store.update_status(&id, JobStatus::Running).unwrap();
        store.cancel_job(&id).unwrap();
        assert_eq!(store.get_job(&id).unwrap().status, JobStatus::Cancelled);
    }

    #[test]
    fn test_cancel_completed_fails() {
        let store = JobStore::new(test_config());
        let id = store
            .create_job(JobType::AsyncCrawl, serde_json::json!({}), None, None, None)
            .unwrap();
        store.update_status(&id, JobStatus::Running).unwrap();
        store.update_status(&id, JobStatus::Completed).unwrap();

        let result = store.cancel_job(&id);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("terminal"));
    }

    #[test]
    fn test_cancel_failed_fails() {
        let store = JobStore::new(test_config());
        let id = store
            .create_job(JobType::AsyncCrawl, serde_json::json!({}), None, None, None)
            .unwrap();
        store.update_status(&id, JobStatus::Running).unwrap();
        store
            .set_error(&id, "error".to_string())
            .unwrap();

        let result = store.cancel_job(&id);
        assert!(result.is_err());
    }

    #[test]
    fn test_list_jobs_filters() {
        let store = JobStore::new(test_config());
        store
            .create_job(JobType::AsyncCrawl, serde_json::json!({}), None, None, None)
            .unwrap();
        let id2 = store
            .create_job(JobType::BatchScrape, serde_json::json!({}), None, None, None)
            .unwrap();
        store.update_status(&id2, JobStatus::Running).unwrap();
        store
            .create_job(JobType::AsyncCrawl, serde_json::json!({}), None, None, None)
            .unwrap();

        // Filter by type
        let crawls = store.list_jobs(Some(&JobType::AsyncCrawl), None, 20);
        assert_eq!(crawls.len(), 2);

        // Filter by status
        let running = store.list_jobs(None, Some(&JobStatus::Running), 20);
        assert_eq!(running.len(), 1);
        assert_eq!(running[0].id, id2);

        // Filter by both
        let queued_crawls =
            store.list_jobs(Some(&JobType::AsyncCrawl), Some(&JobStatus::Queued), 20);
        assert_eq!(queued_crawls.len(), 2);
    }

    #[test]
    fn test_list_jobs_respects_limit() {
        let store = JobStore::new(test_config());
        for _ in 0..5 {
            store
                .create_job(JobType::AsyncCrawl, serde_json::json!({}), None, None, None)
                .unwrap();
        }

        let jobs = store.list_jobs(None, None, 3);
        assert_eq!(jobs.len(), 3);
    }

    #[tokio::test]
    async fn test_ttl_cleanup() {
        let store = JobStore::new(JobStoreConfig {
            max_jobs: 10,
            result_ttl_secs: 1,
            cleanup_interval_secs: 1,
        });

        let id = store
            .create_job(JobType::AsyncCrawl, serde_json::json!({}), None, None, None)
            .unwrap();
        assert!(store.get_job(&id).is_some());

        store.start_cleanup_task();

        // Wait for cleanup to run (TTL=1s, interval=1s)
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;

        assert!(store.get_job(&id).is_none());
    }

    #[tokio::test]
    async fn test_concurrent_access() {
        let store = JobStore::new(JobStoreConfig {
            max_jobs: 1000,
            ..test_config()
        });

        let mut handles = vec![];
        for _ in 0..50 {
            let s = store.clone();
            handles.push(tokio::spawn(async move {
                let id = s
                    .create_job(JobType::AsyncCrawl, serde_json::json!({}), None, None, None)
                    .unwrap();
                s.update_status(&id, JobStatus::Running).unwrap();
                s.update_progress(
                    &id,
                    JobProgress {
                        completed: 1,
                        total: Some(10),
                        current_url: Some("https://example.com".to_string()),
                        percent: Some(10),
                    },
                )
                .unwrap();
                s.set_result(&id, JobResult::CrawlResult(vec![])).unwrap();
            }));
        }

        for h in handles {
            h.await.unwrap();
        }

        let all = store.list_jobs(None, None, 1000);
        assert_eq!(all.len(), 50);
        assert!(all.iter().all(|j| j.status == JobStatus::Completed));
    }

    #[tokio::test]
    async fn test_set_result_returns_webhook_url() {
        let store = JobStore::new(test_config());
        let id = store
            .create_job(
                JobType::AsyncCrawl,
                serde_json::json!({}),
                Some("https://hooks.example.com/callback".to_string()),
                None,
                None,
            )
            .unwrap();
        store.update_status(&id, JobStatus::Running).unwrap();

        let webhook = store
            .set_result(&id, JobResult::CrawlResult(vec![]))
            .unwrap();
        assert_eq!(
            webhook,
            Some("https://hooks.example.com/callback".to_string())
        );
    }

    #[test]
    fn test_update_progress_only_when_running() {
        let store = JobStore::new(test_config());
        let id = store
            .create_job(JobType::AsyncCrawl, serde_json::json!({}), None, None, None)
            .unwrap();

        // Should fail when Queued
        let result = store.update_progress(
            &id,
            JobProgress {
                completed: 1,
                ..Default::default()
            },
        );
        assert!(result.is_err());

        // Should succeed when Running
        store.update_status(&id, JobStatus::Running).unwrap();
        store
            .update_progress(
                &id,
                JobProgress {
                    completed: 5,
                    total: Some(10),
                    current_url: Some("https://example.com".to_string()),
                    percent: Some(50),
                },
            )
            .unwrap();

        let job = store.get_job(&id).unwrap();
        assert_eq!(job.progress.completed, 5);
        assert_eq!(job.progress.percent, Some(50));
    }
}
