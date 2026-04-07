use crate::{
    crawler::sitemap,
    error::{Result, ScrapeError},
    types::MapRequest,
    utils::etld::extract_etld_plus_one_from_host,
};
use reqwest::Client;
use scraper::{Html, Selector};
use std::collections::HashSet;
use url::Url;

/// Discover URLs from a given URL
pub async fn discover_urls(url: &str, options: &MapRequest) -> Result<Vec<String>> {
    let base_url =
        Url::parse(url).map_err(|e| ScrapeError::InvalidUrl(format!("Invalid URL: {}", e)))?;

    let client = Client::builder()
        .user_agent("Mozilla/5.0 (compatible; Essence/0.1.0; +https://essence.foundation)")
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| ScrapeError::Internal(format!("Failed to build HTTP client: {}", e)))?;

    let mut all_urls = HashSet::new();

    // 1. Try sitemap discovery first (unless explicitly ignored)
    // Use the site root (scheme + host) for sitemap discovery, since sitemaps
    // live at the domain root, not under arbitrary paths.
    let site_root = format!("{}://{}", base_url.scheme(), base_url.host_str().unwrap_or(""));
    if !options.ignore_sitemap.unwrap_or(false) {
        match sitemap::fetch_sitemap(&site_root, &client).await {
            Ok(sitemap_urls) => {
                if !sitemap_urls.is_empty() {
                    tracing::info!("Found {} URLs from sitemap for {}", sitemap_urls.len(), url);
                    all_urls.extend(sitemap_urls);
                } else {
                    tracing::debug!("No sitemap URLs found for {}", url);
                }
            }
            Err(e) => {
                tracing::debug!("Sitemap fetch failed for {}: {}", url, e);
            }
        }
    }

    // 2. Fetch the page and extract links from HTML (always do this for comprehensive coverage)
    let response = client.get(url).send().await.map_err(|e| {
        if e.is_timeout() {
            ScrapeError::Timeout
        } else {
            ScrapeError::RequestFailed(e)
        }
    })?;

    // Use the final URL after redirects to resolve relative links correctly.
    // e.g., docs.anthropic.com → platform.claude.com/docs/ means relative hrefs
    // should be resolved against platform.claude.com, not docs.anthropic.com.
    let final_url = response.url().clone();
    tracing::debug!("Final URL after redirects: {}", final_url);

    let html_content = response
        .text()
        .await
        .map_err(|e| ScrapeError::Internal(format!("Failed to read HTML content: {}", e)))?;

    // Parse HTML and extract links
    let document = Html::parse_document(&html_content);
    let link_selector = Selector::parse("a[href]")
        .map_err(|e| ScrapeError::Internal(format!("Invalid selector: {:?}", e)))?;

    // Use both the original base_url and the final_url for domain filtering
    let filter_base = &final_url;

    let mut in_page_links = 0;
    for element in document.select(&link_selector) {
        if let Some(href) = element.value().attr("href") {
            // Resolve relative URLs against the final (post-redirect) URL
            if let Ok(absolute_url) = final_url.join(href) {
                let url_str = absolute_url.to_string();

                // Filter by subdomain option
                if let Some(include_subdomains) = options.include_subdomains {
                    if !include_subdomains {
                        // Only include URLs from the same domain (no subdomains)
                        if let (Some(base_host), Some(url_host)) =
                            (filter_base.host_str(), absolute_url.host_str())
                        {
                            if base_host != url_host {
                                continue;
                            }
                        }
                    } else {
                        // Include subdomains - check if it's the same base domain
                        if let (Some(base_host), Some(url_host)) =
                            (filter_base.host_str(), absolute_url.host_str())
                        {
                            let base_domain = extract_etld_plus_one_from_host(base_host);
                            let url_domain = extract_etld_plus_one_from_host(url_host);
                            if base_domain != url_domain {
                                continue;
                            }
                        }
                    }
                }

                if all_urls.insert(url_str) {
                    in_page_links += 1;
                }
            }
        }
    }

    tracing::info!(
        "Found {} in-page links for {} (total unique: {})",
        in_page_links,
        url,
        all_urls.len()
    );

    // 3. Filter by search query if provided
    let mut filtered_urls: Vec<String> = if let Some(search) = &options.search {
        all_urls
            .into_iter()
            .filter(|url| url.to_lowercase().contains(&search.to_lowercase()))
            .collect()
    } else {
        all_urls.into_iter().collect()
    };

    // 4. Sort for consistent output
    filtered_urls.sort();

    // 5. Apply limit
    let limit = options.limit.unwrap_or(5000) as usize;
    if filtered_urls.len() > limit {
        filtered_urls.truncate(limit);
    }

    Ok(filtered_urls)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_etld_plus_one_co_uk_subdomain_matching() {
        // .co.uk is a multi-part public suffix; subdomains of the same
        // registrable domain should match each other.
        let base = extract_etld_plus_one_from_host("www.example.co.uk");
        let sub = extract_etld_plus_one_from_host("api.example.co.uk");
        assert_eq!(base, sub, "subdomains of example.co.uk should share the same eTLD+1");
        assert_eq!(base, "example.co.uk");
    }

    #[test]
    fn test_etld_plus_one_github_io_isolation() {
        // github.io is in the Public Suffix List, so user1.github.io and
        // user2.github.io are *different* registrable domains.
        let a = extract_etld_plus_one_from_host("user1.github.io");
        let b = extract_etld_plus_one_from_host("user2.github.io");
        assert_ne!(a, b, "different github.io subdomains must be isolated");
    }

    #[test]
    fn test_etld_plus_one_basic() {
        assert_eq!(extract_etld_plus_one_from_host("example.com"), "example.com");
        assert_eq!(extract_etld_plus_one_from_host("blog.example.com"), "example.com");
        assert_eq!(extract_etld_plus_one_from_host("api.blog.example.com"), "example.com");
        assert_eq!(extract_etld_plus_one_from_host("localhost"), "localhost");
    }

    #[test]
    fn test_url_filtering() {
        let base_url = Url::parse("https://example.com").unwrap();

        // Test subdomain filtering logic
        let url_same_domain = Url::parse("https://example.com/page").unwrap();
        let url_subdomain = Url::parse("https://blog.example.com/page").unwrap();
        let url_different = Url::parse("https://different.com/page").unwrap();

        assert_eq!(
            base_url.host_str().unwrap(),
            url_same_domain.host_str().unwrap()
        );
        assert_ne!(
            base_url.host_str().unwrap(),
            url_subdomain.host_str().unwrap()
        );
        assert_ne!(
            base_url.host_str().unwrap(),
            url_different.host_str().unwrap()
        );
    }
}
