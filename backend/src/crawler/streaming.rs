use crate::{
    api::scrape::scrape_core_logic,
    crawler::config::{CircuitBreaker, CrawlerConfig, MemoryMonitor},
    crawler::dedup::ContentDeduplicator,
    crawler::filter::{is_same_domain, should_crawl_url},
    crawler::pagination::{PaginationConfig, PaginationDetector},
    crawler::prioritizer::{PrioritizedUrl, UrlPrioritizer},
    crawler::rate_limiter::DomainRateLimiter,
    engines::{http::HttpEngine, ScrapeEngine},
    error::{Result, ScrapeError},
    format,
    types::{CrawlEvent, CrawlRequest, Document, ScrapeRequest},
    crawler::url_normalization::normalize_url,
    utils::robots,
};
use scraper::{Html, Selector};
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};
use url::Url;

/// Crawl a website and stream documents as they're scraped
pub async fn crawl_website_stream(
    request: CrawlRequest,
    tx: mpsc::Sender<Result<CrawlEvent>>,
) -> Result<()> {
    info!("Starting streaming crawl from URL: {}", request.url);

    // Parse and validate base URL
    let _base_url = Url::parse(&request.url)
        .map_err(|e| ScrapeError::InvalidUrl(format!("Invalid base URL: {}", e)))?;

    // Normalize the base URL to prevent duplicates
    let normalized_base_url = normalize_url(&request.url);

    // Initialize crawler config with bounds
    let config = CrawlerConfig::default();

    // Initialize circuit breaker and memory monitor
    let circuit_breaker = CircuitBreaker::new(config.circuit_breaker_threshold);
    let memory_monitor = MemoryMonitor::new(config.max_memory_mb, config.enable_memory_monitoring);

    // Initialize URL prioritizer for intelligent crawling
    let url_prioritizer = UrlPrioritizer::new();

    // Initialize data structures with capacity hints
    let mut visited = HashSet::new();
    let mut queue = BinaryHeap::with_capacity(config.max_queue_size.min(1000));
    let mut url_depths: HashMap<String, u32> = HashMap::new();
    let mut success_count = 0usize;
    let mut error_count = 0usize;

    // Add normalized base URL to priority queue
    let base_prioritized_url = PrioritizedUrl::new(normalized_base_url.clone(), 0, &url_prioritizer);
    queue.push(base_prioritized_url);
    url_depths.insert(normalized_base_url, 0);

    // Check robots.txt for the domain
    let robots_cache = check_robots_txt(&request.url, request.ignore_sitemap).await;

    // Create HTTP engine for scraping
    let engine = HttpEngine::new()?;

    // Initialize pagination detector with configuration
    let pagination_config = PaginationConfig {
        max_pages: request.max_pagination_pages.unwrap_or(50) as usize,
        max_depth: request.max_depth as usize,
        detect_circular: true,
    };
    let mut pagination_detector = PaginationDetector::new(pagination_config);
    let detect_pagination = request.detect_pagination.unwrap_or(true);

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

    // Initialize content deduplicator
    let mut content_dedup = ContentDeduplicator::new();

    // Priority-based crawl (BFS with intelligent ordering)
    while let Some(prioritized_url) = queue.pop() {
        let current_url = prioritized_url.url;
        let current_depth = prioritized_url.depth;

        // Check memory limit periodically (every 10 iterations)
        if (success_count + error_count).is_multiple_of(10) {
            if let Err(e) = memory_monitor.check_memory_limit() {
                warn!("Memory limit check failed: {}", e);
                let error_msg = e.to_string();
                let _ = tx.send(Ok(CrawlEvent::Error {
                    url: current_url.clone(),
                    error: error_msg.clone(),
                })).await;
                return Err(ScrapeError::ResourceLimit(error_msg));
            }
        }

        // Check if we've reached the document limit
        if success_count >= request.limit as usize {
            info!("Reached crawl limit of {} pages", request.limit);
            break;
        }

        // Skip if already visited
        if visited.contains(&current_url) {
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
                "Circuit breaker: Skipping domain {} due to excessive failures ({})",
                domain,
                circuit_breaker.get_failure_count(&domain)
            );
            visited.insert(current_url.clone());
            continue;
        }

        // Skip if depth exceeds max_depth
        if current_depth > request.max_depth {
            debug!("Skipping URL due to depth limit: {}", current_url);
            continue;
        }

        // Check robots.txt
        if !robots_cache {
            match robots::is_allowed_default(&current_url).await {
                Ok(allowed) => {
                    if !allowed {
                        warn!("Robots.txt disallows crawling: {}", current_url);
                        visited.insert(current_url.clone());
                        continue;
                    }
                }
                Err(e) => {
                    warn!("Failed to check robots.txt for {}: {}", current_url, e);
                }
            }
        }

        // Check if URL should be crawled based on filters
        if !should_crawl_url(&current_url, &request.include_paths, &request.exclude_paths) {
            debug!(
                "URL filtered out by include/exclude patterns: {}",
                current_url
            );
            visited.insert(current_url.clone());
            continue;
        }

        // Mark as visited (URL is already normalized from queue)
        visited.insert(current_url.clone());

        // Apply rate limiting before scraping
        if let Err(e) = rate_limiter.wait_for_permission(&current_url).await {
            warn!("Rate limiter error for {}: {}", current_url, e);
            continue;
        }

        // Send status update
        let status_event = CrawlEvent::Status {
            pages_crawled: success_count,
            queue_size: queue.len(),
            current_url: Some(current_url.clone()),
        };
        if tx.send(Ok(status_event)).await.is_err() {
            // Client disconnected
            info!("Client disconnected, stopping crawl");
            break;
        }

        // Scrape the URL
        info!(
            "Crawling URL: {} (depth: {}, total: {})",
            current_url,
            current_depth,
            success_count
        );

        // Determine engine mode from request
        let engine_mode = request.engine.as_deref().unwrap_or("http");

        // Scrape with retry for transient errors
        let mut scrape_result = scrape_url(&current_url, &engine, &config, engine_mode).await;
        for retry in 0..2 {
            if scrape_result.is_ok() || !scrape_result.as_ref().err().map_or(false, |e| e.is_transient()) {
                break;
            }
            tracing::debug!("Retrying transient error for {} (attempt {})", current_url, retry + 2);
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            scrape_result = scrape_url(&current_url, &engine, &config, engine_mode).await;
        }

        match scrape_result {
            Ok((document, links, html)) => {
                // Record success in circuit breaker
                if config.enable_circuit_breaker {
                    circuit_breaker.record_success(&domain);
                }

                // Check for duplicate content before counting and sending
                if let Some(ref md) = document.markdown {
                    if content_dedup.is_duplicate(md) {
                        debug!("Skipping duplicate content: {}", current_url);
                        // Still process links for duplicate content
                        if current_depth < request.max_depth {
                            for link in links {
                                let normalized_link = normalize_url(&link);
                                if !visited.contains(&normalized_link)
                                    && !url_depths.contains_key(&normalized_link)
                                    && is_same_domain(&normalized_link, &request.url)
                                {
                                    if queue.len() < config.max_queue_size {
                                        let prioritized_link = PrioritizedUrl::new(
                                            normalized_link.clone(),
                                            current_depth + 1,
                                            &url_prioritizer,
                                        );
                                        queue.push(prioritized_link);
                                        url_depths.insert(normalized_link, current_depth + 1);
                                    }
                                }
                            }
                        }
                        continue;
                    }
                }

                success_count += 1;

                // Send document event immediately
                let doc_event = CrawlEvent::Document {
                    url: document.url.clone().unwrap_or_else(|| current_url.clone()),
                    title: document.title.clone(),
                    markdown: document.markdown.clone(),
                    metadata: Box::new(document.metadata.clone()),
                };

                if tx.send(Ok(doc_event)).await.is_err() {
                    // Client disconnected
                    info!("Client disconnected, stopping crawl");
                    break;
                }

                // Process discovered links if we haven't reached max depth
                if current_depth < request.max_depth {
                    // First, detect pagination links if enabled
                    let pagination_links = if detect_pagination {
                        pagination_detector.detect_pagination(&html, &current_url)
                    } else {
                        Vec::new()
                    };

                    // Add pagination links with priority (same depth as current)
                    for link in &pagination_links {
                        let normalized_link = normalize_url(link);

                        // Skip if already visited or queued
                        if visited.contains(&normalized_link)
                            || url_depths.contains_key(&normalized_link)
                        {
                            continue;
                        }

                        // Check domain restrictions
                        if is_same_domain(&normalized_link, &request.url) {
                            // Check queue size limit before adding
                            if queue.len() >= config.max_queue_size {
                                warn!(
                                    "Queue limit reached ({}/{}), skipping pagination link: {}",
                                    queue.len(),
                                    config.max_queue_size,
                                    link
                                );
                                continue;
                            }

                            // Add pagination links with high priority (same depth as current)
                            let prioritized_link = PrioritizedUrl::new(
                                normalized_link.clone(),
                                current_depth,
                                &url_prioritizer,
                            );
                            queue.push(prioritized_link);
                            url_depths.insert(normalized_link, current_depth);
                            debug!("Added pagination link to queue: {}", link);
                        }
                    }

                    // Then process regular links
                    for link in links {
                        // Normalize the link to prevent duplicates
                        let normalized_link = normalize_url(&link);

                        // Skip if already visited or queued
                        if visited.contains(&normalized_link)
                            || url_depths.contains_key(&normalized_link)
                        {
                            continue;
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
                            // Check queue size limit before adding
                            if queue.len() >= config.max_queue_size {
                                debug!(
                                    "Queue limit reached ({}/{}), skipping link: {}",
                                    queue.len(),
                                    config.max_queue_size,
                                    link
                                );
                                continue;
                            }

                            // Apply backpressure: when queue is at threshold, skip secondary links
                            let backpressure_limit =
                                (config.max_queue_size * config.backpressure_threshold as usize) / 100;
                            if queue.len() >= backpressure_limit {
                                debug!(
                                    "Queue at backpressure threshold ({}/{}), slowing link discovery",
                                    queue.len(),
                                    config.max_queue_size
                                );
                                // Only add if this is a direct child (depth + 1)
                                // Skip secondary/deeper links
                                if current_depth + 1 < request.max_depth {
                                    let prioritized_link = PrioritizedUrl::new(
                                        normalized_link.clone(),
                                        current_depth + 1,
                                        &url_prioritizer,
                                    );
                                    queue.push(prioritized_link);
                                    url_depths.insert(normalized_link, current_depth + 1);
                                }
                            } else {
                                let prioritized_link = PrioritizedUrl::new(
                                    normalized_link.clone(),
                                    current_depth + 1,
                                    &url_prioritizer,
                                );
                                queue.push(prioritized_link);
                                url_depths.insert(normalized_link, current_depth + 1);
                            }
                        }
                    }
                }
            }
            Err(e) => {
                warn!("Failed to scrape {}: {}", current_url, e);

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

                error_count += 1;

                // Send error event
                let error_event = CrawlEvent::Error {
                    url: current_url.clone(),
                    error: e.to_string(),
                };

                if tx.send(Ok(error_event)).await.is_err() {
                    // Client disconnected
                    info!("Client disconnected, stopping crawl");
                    break;
                }
            }
        }
    }

    // Send final completion event
    let complete_event = CrawlEvent::Complete {
        total_pages: success_count + error_count,
        success: success_count,
        errors: error_count,
    };

    let _ = tx.send(Ok(complete_event)).await;

    // Log final stats
    info!(
        "Crawl stats - Queue size: {}, Circuit breaker failures: {}, Memory: {:.2}%",
        queue.len(),
        circuit_breaker.get_total_failures(),
        memory_monitor.get_memory_percentage()
    );

    info!(
        "Streaming crawl completed. Total pages crawled: {}, errors: {}",
        success_count, error_count
    );

    Ok(())
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
