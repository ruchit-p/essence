use crate::error::{Result, ScrapeError};
use reqwest::Client;
use robotstxt::DefaultMatcher;
use std::time::Duration;
use url::Url;

/// Check if a URL is allowed by robots.txt
pub async fn is_allowed(url: &str, user_agent: &str) -> Result<bool> {
    let parsed_url =
        Url::parse(url).map_err(|e| ScrapeError::InvalidUrl(format!("Invalid URL: {}", e)))?;

    // Construct robots.txt URL
    let robots_url = format!(
        "{}://{}/robots.txt",
        parsed_url.scheme(),
        parsed_url
            .host_str()
            .ok_or_else(|| ScrapeError::InvalidUrl("No host in URL".to_string()))?
    );

    // Fetch robots.txt
    let client = Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| ScrapeError::Internal(format!("Failed to create HTTP client: {}", e)))?;

    let response = match client.get(&robots_url).send().await {
        Ok(resp) => resp,
        Err(_) => {
            // If robots.txt doesn't exist or can't be fetched, allow by default
            return Ok(true);
        }
    };

    if !response.status().is_success() {
        // If robots.txt doesn't exist, allow by default
        return Ok(true);
    }

    let robots_txt = response
        .text()
        .await
        .map_err(|e| ScrapeError::Internal(format!("Failed to read robots.txt: {}", e)))?;

    // Parse and check robots.txt
    let mut matcher = DefaultMatcher::default();
    let path = parsed_url.path();

    Ok(matcher.one_agent_allowed_by_robots(&robots_txt, user_agent, path))
}

/// Check robots.txt with default user agent
pub async fn is_allowed_default(url: &str) -> Result<bool> {
    is_allowed(url, "Essence").await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_robots_url_construction() {
        let url = "https://example.com/page";
        let parsed = Url::parse(url).unwrap();
        let robots_url = format!(
            "{}://{}/robots.txt",
            parsed.scheme(),
            parsed.host_str().unwrap()
        );
        assert_eq!(robots_url, "https://example.com/robots.txt");
    }
}
