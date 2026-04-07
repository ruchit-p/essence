use crate::{
    api::scrape::scrape_core_logic,
    crawler::config::{CircuitBreaker, CrawlerConfig, MemoryMonitor},
    crawler::dedup::ContentDeduplicator,
    crawler::filter::{is_same_domain, should_crawl_url},
    crawler::pagination::{PaginationConfig, PaginationDetector},
    crawler::rate_limiter::DomainRateLimiter,
    engines::{http::HttpEngine, ScrapeEngine},
    error::{Result, ScrapeError},
    format,
    types::{CrawlRequest, Document, ScrapeRequest},
    crawler::url_normalization::normalize_url,
    utils::robots,
};
use scraper::{Html, Selector};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::{mpsc, Semaphore};
use tokio::time::{timeout, Duration};
use tracing::{debug, info, warn, error};
use url::Url;

/// Parallel crawler with configurable concurrency
pub struct ParallelCrawler {
    /// Semaphore to limit concurrent scrapes
    scrape_semaphore: Arc<Semaphore>,
    /// Semaphore to limit concurrent processing (reserved for future use)
    #[allow(dead_code)]
    process_semaphore: Arc<Semaphore>,
    /// Maximum number of worker tasks
    max_workers: usize,
}

impl ParallelCrawler {
    /// Create a new parallel crawler with default settings
    pub fn new() -> Self {
        let num_cpus = num_cpus::get();
        Self {
            scrape_semaphore: Arc::new(Semaphore::new(num_cpus)),
            process_semaphore: Arc::new(Semaphore::new(num_cpus / 2)),
            max_workers: num_cpus,
        }
    }

    /// Create a new parallel crawler with custom concurrency settings
    pub fn with_config(config: &CrawlerConfig) -> Self {
        let max_concurrent = config.max_concurrent_requests;
        Self {
            scrape_semaphore: Arc::new(Semaphore::new(max_concurrent)),
            process_semaphore: Arc::new(Semaphore::new(max_concurrent / 2)),
            max_workers: max_concurrent,
        }
    }

    /// Main parallel crawl method
    pub async fn crawl_parallel(&self, request: &CrawlRequest) -> Result<Vec<Document>> {
        info!(
            "Starting parallel crawl from URL: {} with {} workers",
            request.url, self.max_workers
        );

        // Parse and validate base URL
        let _base_url = Url::parse(&request.url)
            .map_err(|e| ScrapeError::InvalidUrl(format!("Invalid base URL: {}", e)))?;

        // Normalize the base URL to prevent duplicates
        let normalized_base_url = normalize_url(&request.url);

        // Initialize crawler config with bounds
        let config = CrawlerConfig::default();

        // Initialize circuit breaker and memory monitor
        let circuit_breaker = Arc::new(CircuitBreaker::new(config.circuit_breaker_threshold));
        let memory_monitor = Arc::new(MemoryMonitor::new(
            config.max_memory_mb,
            config.enable_memory_monitoring,
        ));

        // Shared state
        let visited = Arc::new(tokio::sync::RwLock::new(HashSet::new()));
        let url_depths = Arc::new(tokio::sync::RwLock::new(HashMap::new()));

        // Check robots.txt for the domain
        let robots_allowed = check_robots_txt(&request.url, request.ignore_sitemap).await;

        // Create rate limiter
        let rate_limit = std::env::var("CRAWL_RATE_LIMIT_PER_SEC")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(2);
        let rate_limiter = Arc::new(DomainRateLimiter::new(rate_limit));

        info!(
            "Rate limiting enabled: {} requests/second per domain",
            rate_limit
        );

        // Initialize pagination detector with configuration
        let pagination_config = PaginationConfig {
            max_pages: request.max_pagination_pages.unwrap_or(50) as usize,
            max_depth: request.max_depth as usize,
            detect_circular: true,
        };
        let pagination_detector = Arc::new(tokio::sync::Mutex::new(PaginationDetector::new(
            pagination_config,
        )));
        let detect_pagination = request.detect_pagination.unwrap_or(true);

        // Initialize content deduplicator
        let content_dedup = Arc::new(tokio::sync::Mutex::new(ContentDeduplicator::new()));

        // Channels for communication
        // Buffer size matches queue size to prevent unbounded growth
        let (url_tx, url_rx) = mpsc::channel::<(String, u32)>(config.max_queue_size);
        let (doc_tx, doc_rx) = mpsc::channel::<Document>(config.max_queue_size);

        // Add base URL to the queue
        url_depths.write().await.insert(normalized_base_url.clone(), 0);
        if let Err(e) = url_tx.send((normalized_base_url.clone(), 0)).await {
            return Err(ScrapeError::Internal(format!("Failed to enqueue base URL: {}", e)));
        }

        // Clone request for workers
        let request_clone = request.clone();

        // Wrap receivers in Arc<Mutex> for sharing among workers
        let url_rx = Arc::new(tokio::sync::Mutex::new(url_rx));
        let doc_tx_clone = doc_tx.clone();

        // Spawn scraping workers
        let mut worker_handles = Vec::new();

        for worker_id in 0..self.max_workers {
            let url_rx_clone = Arc::clone(&url_rx);
            let doc_tx_worker = doc_tx_clone.clone();
            let url_tx_worker = url_tx.clone();
            let visited_clone = Arc::clone(&visited);
            let url_depths_clone = Arc::clone(&url_depths);
            let circuit_breaker_clone = Arc::clone(&circuit_breaker);
            let memory_monitor_clone = Arc::clone(&memory_monitor);
            let rate_limiter_clone = Arc::clone(&rate_limiter);
            let scrape_semaphore_clone = Arc::clone(&self.scrape_semaphore);
            let pagination_detector_clone = Arc::clone(&pagination_detector);
            let content_dedup_clone = Arc::clone(&content_dedup);
            let config_clone = config.clone();
            let request_clone2 = request_clone.clone();

            let handle = tokio::spawn(async move {
                Self::scrape_worker(
                    worker_id,
                    url_rx_clone,
                    doc_tx_worker,
                    url_tx_worker,
                    visited_clone,
                    url_depths_clone,
                    circuit_breaker_clone,
                    memory_monitor_clone,
                    rate_limiter_clone,
                    scrape_semaphore_clone,
                    pagination_detector_clone,
                    content_dedup_clone,
                    config_clone,
                    request_clone2,
                    robots_allowed,
                    detect_pagination,
                )
                .await
            });

            worker_handles.push(handle);
        }

        // Drop the original senders so receivers know when to close
        drop(url_tx);
        drop(doc_tx);

        // Collect documents as they arrive
        let doc_limit = request.limit as usize;
        let mut documents = Vec::with_capacity(doc_limit.min(1000));
        let mut doc_rx = doc_rx;

        // Collect documents up to limit
        while let Some(doc) = doc_rx.recv().await {
            documents.push(doc);
            
            // Stop collecting once we reach the limit
            if documents.len() >= doc_limit {
                info!("Reached document limit of {}, stopping collection", doc_limit);
                break;
            }
        }

        // Wait for all workers to complete (or cancel them if we've reached the limit)
        for handle in worker_handles {
            // Abort remaining workers if we've reached the limit
            if documents.len() >= doc_limit {
                handle.abort();
            } else if let Err(e) = handle.await {
                if !e.is_cancelled() {
                    error!("Worker task failed: {}", e);
                }
            }
        }

        // Log final stats
        let visited_count = visited.read().await.len();
        info!(
            "Parallel crawl completed. Total pages crawled: {}, visited: {}",
            documents.len(),
            visited_count
        );

        Ok(documents)
    }

    /// Scraping worker that processes URLs from the queue
    #[allow(clippy::too_many_arguments)]
    async fn scrape_worker(
        worker_id: usize,
        url_rx: Arc<tokio::sync::Mutex<mpsc::Receiver<(String, u32)>>>,
        doc_tx: mpsc::Sender<Document>,
        url_tx: mpsc::Sender<(String, u32)>,
        visited: Arc<tokio::sync::RwLock<HashSet<String>>>,
        url_depths: Arc<tokio::sync::RwLock<HashMap<String, u32>>>,
        circuit_breaker: Arc<CircuitBreaker>,
        memory_monitor: Arc<MemoryMonitor>,
        rate_limiter: Arc<DomainRateLimiter>,
        scrape_semaphore: Arc<Semaphore>,
        pagination_detector: Arc<tokio::sync::Mutex<PaginationDetector>>,
        content_dedup: Arc<tokio::sync::Mutex<ContentDeduplicator>>,
        config: CrawlerConfig,
        request: CrawlRequest,
        robots_allowed: bool,
        detect_pagination: bool,
    ) -> Result<()> {
        debug!("Worker {} started", worker_id);

        let engine = HttpEngine::new()?;

        loop {
            // Acquire next URL from the shared queue
            let url_item = {
                let mut rx = url_rx.lock().await;
                rx.recv().await
            };

            let (current_url, current_depth) = match url_item {
                Some(item) => item,
                None => {
                    debug!("Worker {}: Queue closed, exiting", worker_id);
                    break;
                }
            };

            // Check and mark as visited atomically
            {
                let mut visited_write = visited.write().await;
                if !visited_write.insert(current_url.clone()) {
                    continue;
                }
            }

            // Check depth limit
            if current_depth > request.max_depth {
                debug!("Worker {}: Skipping URL due to depth limit: {}", worker_id, current_url);
                continue;
            }

            // Extract domain for circuit breaker
            let domain = match Url::parse(&current_url) {
                Ok(parsed) => parsed.host_str().unwrap_or("unknown").to_string(),
                Err(_) => "unknown".to_string(),
            };

            // Check circuit breaker
            if config.enable_circuit_breaker && circuit_breaker.should_skip(&domain) {
                warn!(
                    "Worker {}: Circuit breaker: Skipping domain {} due to excessive failures",
                    worker_id, domain
                );
                continue;
            }

            // Check robots.txt
            if !robots_allowed {
                match robots::is_allowed_default(&current_url).await {
                    Ok(allowed) => {
                        if !allowed {
                            warn!("Worker {}: Robots.txt disallows crawling: {}", worker_id, current_url);
                            continue;
                        }
                    }
                    Err(e) => {
                        warn!("Worker {}: Failed to check robots.txt for {}: {}", worker_id, current_url, e);
                    }
                }
            }

            // Check if URL should be crawled based on filters
            if !should_crawl_url(&current_url, &request.include_paths, &request.exclude_paths) {
                debug!(
                    "Worker {}: URL filtered out by include/exclude patterns: {}",
                    worker_id, current_url
                );
                continue;
            }

            // Check memory limit
            if config.enable_memory_monitoring {
                if let Err(e) = memory_monitor.check_memory_limit() {
                    warn!("Worker {}: Memory limit check failed: {}", worker_id, e);
                    return Err(e);
                }
            }

            // Acquire scrape semaphore permit
            let _permit = scrape_semaphore.acquire().await
                .map_err(|e| ScrapeError::Internal(format!("Failed to acquire scrape permit: {}", e)))?;

            // Apply rate limiting before scraping
            if let Err(e) = rate_limiter.wait_for_permission(&current_url).await {
                warn!("Worker {}: Rate limiter error for {}: {}", worker_id, current_url, e);
                continue;
            }

            // Scrape the URL with timeout
            info!(
                "Worker {}: Crawling URL: {} (depth: {})",
                worker_id, current_url, current_depth
            );

            // Determine engine mode from request
            let engine_mode = request.engine.as_deref().unwrap_or("http");

            // Scrape with retry for transient errors
            let mut scrape_result = timeout(
                Duration::from_secs(config.scrape_timeout_secs),
                scrape_url(&current_url, &engine, &config, engine_mode)
            ).await;
            for retry in 0..2 {
                let should_retry = match &scrape_result {
                    Ok(Err(e)) => e.is_transient(),
                    Err(_) => true, // timeout is transient
                    Ok(Ok(_)) => false,
                };
                if !should_retry {
                    break;
                }
                tracing::debug!("Worker {}: Retrying transient error for {} (attempt {})", worker_id, current_url, retry + 2);
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                scrape_result = timeout(
                    Duration::from_secs(config.scrape_timeout_secs),
                    scrape_url(&current_url, &engine, &config, engine_mode)
                ).await;
            }

            match scrape_result {
                Ok(Ok((document, links, html))) => {
                    // Record success in circuit breaker
                    if config.enable_circuit_breaker {
                        circuit_breaker.record_success(&domain);
                    }

                    // Check for duplicate content before sending
                    {
                        let mut dedup = content_dedup.lock().await;
                        if let Some(ref md) = document.markdown {
                            if dedup.is_duplicate(md) {
                                debug!("Worker {}: Skipping duplicate content: {}", worker_id, current_url);
                                // Still process links below but skip sending the document
                                // Process discovered links if we haven't reached max depth
                                if current_depth < request.max_depth {
                                    for link in links {
                                        let normalized_link = normalize_url(&link);
                                        {
                                            let visited_read = visited.read().await;
                                            let depths_read = url_depths.read().await;
                                            if visited_read.contains(&normalized_link) || depths_read.contains_key(&normalized_link) {
                                                continue;
                                            }
                                        }
                                        if is_same_domain(&normalized_link, &request.url) {
                                            {
                                                let mut depths_write = url_depths.write().await;
                                                if depths_write.len() < config.max_queue_size {
                                                    depths_write.insert(normalized_link.clone(), current_depth + 1);
                                                } else {
                                                    continue;
                                                }
                                            }
                                            if url_tx.send((normalized_link, current_depth + 1)).await.is_err() {
                                                return Ok(());
                                            }
                                        }
                                    }
                                }
                                continue;
                            }
                        }
                    }

                    // Send document to collector
                    if doc_tx.send(document).await.is_err() {
                        debug!("Worker {}: Document receiver closed, exiting", worker_id);
                        return Ok(());
                    }

                    // Process discovered links if we haven't reached max depth
                    if current_depth < request.max_depth {
                        // First, detect pagination links if enabled
                        let pagination_links = if detect_pagination {
                            let mut detector = pagination_detector.lock().await;
                            detector.detect_pagination(&html, &current_url)
                        } else {
                            Vec::new()
                        };

                        // Add pagination links with priority (same depth as current)
                        for link in &pagination_links {
                            let normalized_link = normalize_url(link);

                            // Skip if already visited or queued
                            {
                                let visited_read = visited.read().await;
                                let depths_read = url_depths.read().await;
                                if visited_read.contains(&normalized_link) || depths_read.contains_key(&normalized_link) {
                                    continue;
                                }
                            }

                            // Check domain restrictions
                            if is_same_domain(&normalized_link, &request.url) {
                                // Add to queue
                                {
                                    let mut depths_write = url_depths.write().await;
                                    depths_write.insert(normalized_link.clone(), current_depth);
                                }
                                
                                if url_tx.send((normalized_link, current_depth)).await.is_err() {
                                    debug!("Worker {}: URL receiver closed, exiting", worker_id);
                                    return Ok(());
                                }
                            }
                        }

                        // Then process regular links
                        for link in links {
                            // Normalize the link to prevent duplicates
                            let normalized_link = normalize_url(&link);

                            // Skip if already visited or queued
                            {
                                let visited_read = visited.read().await;
                                let depths_read = url_depths.read().await;
                                if visited_read.contains(&normalized_link) || depths_read.contains_key(&normalized_link) {
                                    continue;
                                }
                            }

                            // Skip if this is a pagination link (already processed)
                            if pagination_links.contains(&link) || pagination_links.contains(&normalized_link) {
                                continue;
                            }

                            // Check domain restrictions
                            let allow_link = if request.allow_external_links.unwrap_or(false) {
                                true
                            } else if request.allow_backward_links.unwrap_or(false) {
                                // Allow backward links means crawl entire domain
                                is_same_domain(&normalized_link, &request.url)
                            } else {
                                // Only allow links that are "forward" (same path or deeper)
                                is_same_domain(&normalized_link, &request.url)
                                    && is_forward_link(&normalized_link, &current_url)
                            };

                            if allow_link {
                                // Add to queue
                                {
                                    let mut depths_write = url_depths.write().await;
                                    if depths_write.len() < config.max_queue_size {
                                        depths_write.insert(normalized_link.clone(), current_depth + 1);
                                    } else {
                                        debug!("Worker {}: Queue limit reached, skipping link: {}", worker_id, link);
                                        continue;
                                    }
                                }
                                
                                if url_tx.send((normalized_link, current_depth + 1)).await.is_err() {
                                    debug!("Worker {}: URL receiver closed, exiting", worker_id);
                                    return Ok(());
                                }
                            }
                        }
                    }
                }
                Ok(Err(e)) => {
                    warn!("Worker {}: Failed to scrape {}: {}", worker_id, current_url, e);

                    // Handle 429 Too Many Requests
                    if let ScrapeError::RequestFailed(ref reqwest_err) = e {
                        if reqwest_err.status().map_or(false, |s| s == reqwest::StatusCode::TOO_MANY_REQUESTS) {
                            rate_limiter.throttle_domain(&current_url);
                        }
                    }

                    // Record failure in circuit breaker
                    if config.enable_circuit_breaker {
                        circuit_breaker.record_failure(&domain);
                    }
                }
                Err(_) => {
                    warn!("Worker {}: Timeout scraping {}", worker_id, current_url);

                    // Record failure in circuit breaker
                    if config.enable_circuit_breaker {
                        circuit_breaker.record_failure(&domain);
                    }
                }
            }
        }

        debug!("Worker {} finished", worker_id);
        Ok(())
    }
}

impl Default for ParallelCrawler {
    fn default() -> Self {
        Self::new()
    }
}

/// Check if robots.txt allows crawling
async fn check_robots_txt(url: &str, ignore_sitemap: Option<bool>) -> bool {
    if ignore_sitemap.unwrap_or(false) {
        return true;
    }

    match robots::is_allowed_default(url).await {
        Ok(allowed) => allowed,
        Err(e) => {
            warn!("Failed to check robots.txt: {}, allowing by default", e);
            true
        }
    }
}

/// Scrape a single URL and extract links
async fn scrape_url(url: &str, engine: &HttpEngine, config: &CrawlerConfig, engine_mode: &str) -> Result<(Document, Vec<String>, String)> {
    // Create a scrape request
    let scrape_request = ScrapeRequest {
        url: url.to_string(),
        formats: vec!["markdown".to_string(), "links".to_string()],
        headers: HashMap::new(),
        include_tags: vec![],
        exclude_tags: vec![],
        only_main_content: true,
        timeout: config.scrape_timeout_secs * 1000,
        wait_for: 0,
        remove_base64_images: true,
        skip_tls_verification: false,
        engine: "http".to_string(),
        wait_for_selector: None,
        actions: vec![],
        screenshot: false,
        screenshot_format: "png".to_string(),
    };

    // Scrape the URL
    let raw_result = engine.scrape(&scrape_request).await?;

    // Check if the HTTP result has thin content and we should try browser fallback
    if engine_mode == "auto" {
        let html_text_len = scraper::Html::parse_document(&raw_result.html)
            .root_element().text().collect::<String>().trim().len();

        if html_text_len < 100 {
            debug!("Thin content detected ({} chars) for {}, trying browser fallback", html_text_len, url);

            let fallback_request = ScrapeRequest {
                url: url.to_string(),
                formats: vec!["markdown".to_string(), "links".to_string()],
                headers: HashMap::new(),
                include_tags: vec![],
                exclude_tags: vec![],
                only_main_content: true,
                timeout: 30000,
                wait_for: 0,
                remove_base64_images: true,
                skip_tls_verification: false,
                engine: "auto".to_string(),
                wait_for_selector: None,
                actions: vec![],
                screenshot: false,
                screenshot_format: "png".to_string(),
            };

            if let Ok(response) = scrape_core_logic(&fallback_request).await {
                if let Some(doc) = response.data {
                    let has_good_content = doc.markdown.as_ref()
                        .map(|md| md.len() > 100)
                        .unwrap_or(false);

                    if has_good_content {
                        debug!("Browser fallback produced better content for {}", url);
                        let fallback_links = doc.links.clone().unwrap_or_default();
                        let html = raw_result.html.clone();
                        return Ok((doc, fallback_links, html));
                    }
                }
            }
        }
    }

    // Extract links from HTML
    let links = extract_links(&raw_result.html, url)?;

    // Store HTML for pagination detection
    let html = raw_result.html.clone();

    // Process the result into a document
    let document = format::process_scrape_result(raw_result, &scrape_request).await?;

    Ok((document, links, html))
}

/// Extract all links from HTML
fn extract_links(html: &str, base_url: &str) -> Result<Vec<String>> {
    let document = Html::parse_document(html);
    let selector = Selector::parse("a[href]")
        .map_err(|e| ScrapeError::Internal(format!("Failed to create link selector: {:?}", e)))?;

    let base = Url::parse(base_url)
        .map_err(|e| ScrapeError::InvalidUrl(format!("Invalid base URL: {}", e)))?;

    let mut links = Vec::new();

    for element in document.select(&selector) {
        if let Some(href) = element.value().attr("href") {
            // Skip javascript:, mailto:, tel:, etc.
            if href.starts_with("javascript:")
                || href.starts_with("mailto:")
                || href.starts_with("tel:")
                || href.starts_with('#')
            {
                continue;
            }

            // Parse and resolve the URL
            match base.join(href) {
                Ok(absolute_url) => {
                    let url_str = absolute_url.to_string();
                    // Remove fragment
                    let url_without_fragment = url_str.split('#').next().unwrap_or(&url_str);
                    links.push(url_without_fragment.to_string());
                }
                Err(_) => {
                    // Skip invalid URLs
                    continue;
                }
            }
        }
    }

    // Deduplicate
    links.sort();
    links.dedup();

    Ok(links)
}

/// Check if a link is "forward" (same path or deeper)
fn is_forward_link(link: &str, current: &str) -> bool {
    let link_parsed = match Url::parse(link) {
        Ok(u) => u,
        Err(_) => return false,
    };

    let current_parsed = match Url::parse(current) {
        Ok(u) => u,
        Err(_) => return false,
    };

    let link_path = link_parsed.path();
    let current_path = current_parsed.path();

    // A link is forward if:
    // 1. It has the same path as current, or
    // 2. It's a subpath of current (starts with current path + '/')
    if link_path == current_path {
        return true;
    }

    // Ensure we're checking path boundaries, not just string prefixes
    // /news/articles should match /news, but /newsletter should NOT match /news
    let current_with_slash = if current_path.ends_with('/') {
        current_path.to_string()
    } else {
        format!("{}/", current_path)
    };

    link_path.starts_with(&current_with_slash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_parallel_crawler_creation() {
        let crawler = ParallelCrawler::new();
        assert_eq!(crawler.max_workers, num_cpus::get());
    }

    #[tokio::test]
    async fn test_parallel_crawler_with_config() {
        let mut config = CrawlerConfig::default();
        config.max_concurrent_requests = 5;
        
        let crawler = ParallelCrawler::with_config(&config);
        assert_eq!(crawler.max_workers, 5);
    }

    #[test]
    fn test_extract_links() {
        let html = r##"
            <html>
                <body>
                    <a href="/page1">Page 1</a>
                    <a href="/page2">Page 2</a>
                    <a href="https://example.com/page3">Page 3</a>
                    <a href="javascript:void(0)">JS</a>
                    <a href="mailto:test@example.com">Email</a>
                    <a href="#section">Section</a>
                </body>
            </html>
        "##;

        let links = extract_links(html, "https://example.com").unwrap();

        assert!(links.contains(&"https://example.com/page1".to_string()));
        assert!(links.contains(&"https://example.com/page2".to_string()));
        assert!(links.contains(&"https://example.com/page3".to_string()));
        assert!(!links.iter().any(|l| l.contains("javascript:")));
        assert!(!links.iter().any(|l| l.contains("mailto:")));
        assert!(!links.iter().any(|l| l.contains('#')));
    }

    #[test]
    fn test_is_forward_link() {
        assert!(is_forward_link(
            "https://example.com/blog/post1",
            "https://example.com/blog"
        ));

        assert!(is_forward_link(
            "https://example.com/blog",
            "https://example.com/blog"
        ));

        assert!(!is_forward_link(
            "https://example.com/about",
            "https://example.com/blog"
        ));

        // NOT forward: path boundary check
        assert!(!is_forward_link(
            "https://example.com/newsletter",
            "https://example.com/news"
        ));
    }
}
