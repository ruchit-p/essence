// Enhanced metrics collection for scraping integration tests and benchmarks
// Includes percentiles, content hashing, extraction ratios, and engine tagging

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::time::Duration;

/// Comprehensive metrics collected from a scrape operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrapeMetrics {
    /// The URL that was scraped
    pub url: String,

    /// Final URL after redirects
    #[serde(skip_serializing_if = "Option::is_none")]
    pub final_url: Option<String>,

    /// Whether the scrape was successful
    pub success: bool,

    /// HTTP status code returned
    pub status_code: u16,

    /// Content-Type header value
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,

    /// Response time in milliseconds
    pub response_time_ms: u128,

    /// Number of attempts made (for retry logic)
    #[serde(default = "default_attempts")]
    pub attempts: u32,

    /// Length of the markdown content in characters
    pub markdown_length: usize,

    /// Length of the HTML content in characters
    #[serde(default)]
    pub html_length: usize,

    /// Extraction ratio (markdown/html) - how efficiently we converted
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extraction_ratio: Option<f64>,

    /// Word count in markdown
    #[serde(default)]
    pub word_count: usize,

    /// SHA-256 hash of markdown content (for drift detection)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,

    /// Whether a title was extracted
    pub has_title: bool,

    /// Whether a description was extracted
    pub has_description: bool,

    /// Number of links extracted
    pub link_count: usize,

    /// Number of images extracted
    pub image_count: usize,

    /// Error message if the scrape failed
    pub error: Option<String>,

    /// Category of the test (e.g., "sandbox", "static_docs")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,

    /// Test name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub test_name: Option<String>,

    /// Engine tag (e.g., "v1.0", "chromium", "reqwest")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub engine_tag: Option<String>,
}

fn default_attempts() -> u32 {
    1
}

impl ScrapeMetrics {
    /// Create a new ScrapeMetrics instance from a scrape response
    pub fn from_response(url: String, response: &Value, response_time: Duration) -> Self {
        let success = response["success"].as_bool().unwrap_or(false);

        let status_code = response["data"]["metadata"]["statusCode"]
            .as_u64()
            .unwrap_or(0) as u16;

        let markdown = response["data"]["markdown"].as_str().unwrap_or("");

        let html = response["data"]["html"].as_str().unwrap_or("");

        let markdown_length = markdown.len();
        let html_length = html.len();

        // Calculate extraction ratio
        let extraction_ratio = if html_length > 0 {
            Some(markdown_length as f64 / html_length as f64)
        } else {
            None
        };

        // Calculate word count (simple whitespace split)
        let word_count = markdown
            .split_whitespace()
            .filter(|w| !w.is_empty())
            .count();

        // Calculate SHA-256 hash of markdown content
        let content_hash = if !markdown.is_empty() {
            let mut hasher = Sha256::new();
            hasher.update(markdown.as_bytes());
            let result = hasher.finalize();
            Some(format!("{:x}", result))
        } else {
            None
        };

        let metadata = &response["data"]["metadata"];
        let has_title =
            metadata["title"].is_string() && !metadata["title"].as_str().unwrap_or("").is_empty();
        let has_description = metadata["description"].is_string()
            && !metadata["description"].as_str().unwrap_or("").is_empty();

        let final_url = metadata["url"].as_str().map(|s| s.to_string());

        let content_type = metadata["contentType"].as_str().map(|s| s.to_string());

        let api_link_count = response["data"]["links"]
            .as_array()
            .map(|arr| arr.len())
            .unwrap_or(0);
        // Fallback: count links from markdown if API links array is empty
        // This handles pages where links are inside noscript tags (e.g., Discourse)
        let md_link_count = markdown.matches("](").count();
        let link_count = if api_link_count > 0 {
            api_link_count
        } else {
            md_link_count
        };

        let api_image_count = response["data"]["images"]
            .as_array()
            .map(|arr| arr.len())
            .unwrap_or(0);
        // Fallback: count images from markdown if API images array is empty
        let md_image_count = markdown.matches("![").count();
        let image_count = if api_image_count > 0 {
            api_image_count
        } else {
            md_image_count
        };

        let error = if !success {
            Some(
                response["error"]
                    .as_str()
                    .unwrap_or("Unknown error")
                    .to_string(),
            )
        } else {
            None
        };

        ScrapeMetrics {
            url,
            final_url,
            success,
            status_code,
            content_type,
            response_time_ms: response_time.as_millis(),
            attempts: 1,
            markdown_length,
            html_length,
            extraction_ratio,
            word_count,
            content_hash,
            has_title,
            has_description,
            link_count,
            image_count,
            error,
            category: None,
            test_name: None,
            engine_tag: None,
        }
    }

    /// Create a metrics instance for a failed scrape
    pub fn from_error(url: String, error: String, response_time: Duration) -> Self {
        ScrapeMetrics {
            url,
            final_url: None,
            success: false,
            status_code: 0,
            content_type: None,
            response_time_ms: response_time.as_millis(),
            attempts: 1,
            markdown_length: 0,
            html_length: 0,
            extraction_ratio: None,
            word_count: 0,
            content_hash: None,
            has_title: false,
            has_description: false,
            link_count: 0,
            image_count: 0,
            error: Some(error),
            category: None,
            test_name: None,
            engine_tag: None,
        }
    }

    /// Set the category for this metric
    pub fn with_category(mut self, category: impl Into<String>) -> Self {
        self.category = Some(category.into());
        self
    }

    /// Set the test name for this metric
    pub fn with_test_name(mut self, test_name: impl Into<String>) -> Self {
        self.test_name = Some(test_name.into());
        self
    }

    /// Set the engine tag for this metric
    pub fn with_engine_tag(mut self, engine_tag: impl Into<String>) -> Self {
        self.engine_tag = Some(engine_tag.into());
        self
    }

    /// Set the number of attempts
    pub fn with_attempts(mut self, attempts: u32) -> Self {
        self.attempts = attempts;
        self
    }
}

/// Collection of metrics for multiple scrape operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsCollection {
    /// Timestamp when metrics collection started
    pub timestamp: String,

    /// All collected metrics
    pub metrics: Vec<ScrapeMetrics>,

    /// Summary statistics
    pub summary: MetricsSummary,
}

/// Summary statistics for a collection of metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSummary {
    /// Total number of scrapes attempted
    pub total_scrapes: usize,

    /// Number of successful scrapes
    pub successful_scrapes: usize,

    /// Number of failed scrapes
    pub failed_scrapes: usize,

    /// Success rate as a percentage
    pub success_rate: f64,

    /// Average response time in milliseconds
    pub avg_response_time_ms: f64,

    /// Minimum response time in milliseconds
    pub min_response_time_ms: u128,

    /// Maximum response time in milliseconds
    pub max_response_time_ms: u128,

    /// Median response time (p50)
    pub p50_response_time_ms: u128,

    /// 90th percentile response time
    pub p90_response_time_ms: u128,

    /// 99th percentile response time
    pub p99_response_time_ms: u128,

    /// Average markdown length
    pub avg_markdown_length: f64,

    /// Average HTML length
    pub avg_html_length: f64,

    /// Average extraction ratio
    pub avg_extraction_ratio: f64,

    /// Average word count
    pub avg_word_count: f64,

    /// Total links extracted
    pub total_links: usize,

    /// Total images extracted
    pub total_images: usize,

    /// Total attempts across all scrapes
    pub total_attempts: u32,
}

impl MetricsCollection {
    /// Create a new metrics collection
    pub fn new(metrics: Vec<ScrapeMetrics>) -> Self {
        let summary = Self::calculate_summary(&metrics);
        let timestamp = chrono::Utc::now().to_rfc3339();

        MetricsCollection {
            timestamp,
            metrics,
            summary,
        }
    }

    /// Calculate percentile value from sorted data
    fn percentile(sorted_data: &[u128], percentile: f64) -> u128 {
        if sorted_data.is_empty() {
            return 0;
        }
        let index = (percentile / 100.0 * (sorted_data.len() - 1) as f64).round() as usize;
        sorted_data[index.min(sorted_data.len() - 1)]
    }

    /// Calculate summary statistics from metrics
    fn calculate_summary(metrics: &[ScrapeMetrics]) -> MetricsSummary {
        let total_scrapes = metrics.len();
        let successful_scrapes = metrics.iter().filter(|m| m.success).count();
        let failed_scrapes = total_scrapes - successful_scrapes;

        let success_rate = if total_scrapes > 0 {
            (successful_scrapes as f64 / total_scrapes as f64) * 100.0
        } else {
            0.0
        };

        let mut response_times: Vec<u128> = metrics.iter().map(|m| m.response_time_ms).collect();
        response_times.sort();

        let avg_response_time_ms = if !response_times.is_empty() {
            response_times.iter().sum::<u128>() as f64 / response_times.len() as f64
        } else {
            0.0
        };

        let min_response_time_ms = response_times.first().copied().unwrap_or(0);
        let max_response_time_ms = response_times.last().copied().unwrap_or(0);

        // Calculate percentiles
        let p50_response_time_ms = Self::percentile(&response_times, 50.0);
        let p90_response_time_ms = Self::percentile(&response_times, 90.0);
        let p99_response_time_ms = Self::percentile(&response_times, 99.0);

        let markdown_lengths: Vec<usize> = metrics.iter().map(|m| m.markdown_length).collect();

        let avg_markdown_length = if !markdown_lengths.is_empty() {
            markdown_lengths.iter().sum::<usize>() as f64 / markdown_lengths.len() as f64
        } else {
            0.0
        };

        let html_lengths: Vec<usize> = metrics.iter().map(|m| m.html_length).collect();

        let avg_html_length = if !html_lengths.is_empty() {
            html_lengths.iter().sum::<usize>() as f64 / html_lengths.len() as f64
        } else {
            0.0
        };

        let extraction_ratios: Vec<f64> =
            metrics.iter().filter_map(|m| m.extraction_ratio).collect();

        let avg_extraction_ratio = if !extraction_ratios.is_empty() {
            extraction_ratios.iter().sum::<f64>() / extraction_ratios.len() as f64
        } else {
            0.0
        };

        let word_counts: Vec<usize> = metrics.iter().map(|m| m.word_count).collect();

        let avg_word_count = if !word_counts.is_empty() {
            word_counts.iter().sum::<usize>() as f64 / word_counts.len() as f64
        } else {
            0.0
        };

        let total_links = metrics.iter().map(|m| m.link_count).sum();
        let total_images = metrics.iter().map(|m| m.image_count).sum();
        let total_attempts = metrics.iter().map(|m| m.attempts).sum();

        MetricsSummary {
            total_scrapes,
            successful_scrapes,
            failed_scrapes,
            success_rate,
            avg_response_time_ms,
            min_response_time_ms,
            max_response_time_ms,
            p50_response_time_ms,
            p90_response_time_ms,
            p99_response_time_ms,
            avg_markdown_length,
            avg_html_length,
            avg_extraction_ratio,
            avg_word_count,
            total_links,
            total_images,
            total_attempts,
        }
    }

    /// Save metrics to a JSON file
    pub fn save_json(&self, path: impl AsRef<Path>) -> Result<(), std::io::Error> {
        let json = serde_json::to_string_pretty(self)?;
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;
        file.write_all(json.as_bytes())?;
        Ok(())
    }

    /// Save metrics to a CSV file
    pub fn save_csv(&self, path: impl AsRef<Path>) -> Result<(), std::io::Error> {
        let path = path.as_ref();

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;

        // Write CSV header
        writeln!(
            file,
            "url,final_url,success,status_code,content_type,response_time_ms,attempts,markdown_length,html_length,extraction_ratio,word_count,content_hash,has_title,has_description,link_count,image_count,error,category,test_name,engine_tag"
        )?;

        // Write each metric as a CSV row
        for metric in &self.metrics {
            writeln!(
                file,
                "\"{}\",\"{}\",{},{},\"{}\",{},{},{},{},{},{},\"{}\",{},{},{},{},\"{}\",\"{}\",\"{}\",\"{}\"",
                metric.url,
                metric.final_url.as_deref().unwrap_or(""),
                metric.success,
                metric.status_code,
                metric.content_type.as_deref().unwrap_or(""),
                metric.response_time_ms,
                metric.attempts,
                metric.markdown_length,
                metric.html_length,
                metric.extraction_ratio.map(|r| format!("{:.4}", r)).unwrap_or_default(),
                metric.word_count,
                metric.content_hash.as_deref().unwrap_or(""),
                metric.has_title,
                metric.has_description,
                metric.link_count,
                metric.image_count,
                metric.error.as_deref().unwrap_or(""),
                metric.category.as_deref().unwrap_or(""),
                metric.test_name.as_deref().unwrap_or(""),
                metric.engine_tag.as_deref().unwrap_or(""),
            )?;
        }

        Ok(())
    }

    /// Print a summary report to stdout
    pub fn print_summary(&self) {
        println!("\n{}", "=".repeat(80));
        println!("SCRAPE METRICS SUMMARY");
        println!("{}", "=".repeat(80));
        println!("Timestamp: {}", self.timestamp);
        println!("\nOverall Statistics:");
        println!("  Total scrapes:      {}", self.summary.total_scrapes);
        println!("  Successful:         {}", self.summary.successful_scrapes);
        println!("  Failed:             {}", self.summary.failed_scrapes);
        println!("  Success rate:       {:.2}%", self.summary.success_rate);
        println!("  Total attempts:     {}", self.summary.total_attempts);
        println!("\nPerformance (Response Time):");
        println!(
            "  Min:                {}ms",
            self.summary.min_response_time_ms
        );
        println!(
            "  Avg:                {:.2}ms",
            self.summary.avg_response_time_ms
        );
        println!(
            "  Max:                {}ms",
            self.summary.max_response_time_ms
        );
        println!(
            "  p50 (median):       {}ms",
            self.summary.p50_response_time_ms
        );
        println!(
            "  p90:                {}ms",
            self.summary.p90_response_time_ms
        );
        println!(
            "  p99:                {}ms",
            self.summary.p99_response_time_ms
        );
        println!("\nContent Extraction:");
        println!(
            "  Avg markdown:       {:.0} chars",
            self.summary.avg_markdown_length
        );
        println!(
            "  Avg HTML:           {:.0} chars",
            self.summary.avg_html_length
        );
        println!(
            "  Avg extraction:     {:.2}%",
            self.summary.avg_extraction_ratio * 100.0
        );
        println!(
            "  Avg word count:     {:.0} words",
            self.summary.avg_word_count
        );
        println!("  Total links:        {}", self.summary.total_links);
        println!("  Total images:       {}", self.summary.total_images);
        println!("{}", "=".repeat(80));

        // Print failures if any
        let failures: Vec<&ScrapeMetrics> = self.metrics.iter().filter(|m| !m.success).collect();

        if !failures.is_empty() {
            println!("\nFailed Scrapes:");
            for (i, failure) in failures.iter().enumerate() {
                println!(
                    "  {}. {} - {}",
                    i + 1,
                    failure.url,
                    failure.error.as_deref().unwrap_or("Unknown error")
                );
            }
        }

        // Detect content drift if multiple runs of same URL
        self.detect_content_drift();

        println!();
    }

    /// Detect content drift between runs of the same URL
    pub fn detect_content_drift(&self) {
        use std::collections::HashMap;

        let mut url_hashes: HashMap<String, Vec<String>> = HashMap::new();

        for metric in &self.metrics {
            if let Some(hash) = &metric.content_hash {
                url_hashes
                    .entry(metric.url.clone())
                    .or_default()
                    .push(hash.clone());
            }
        }

        let drifts: Vec<(&String, &Vec<String>)> = url_hashes
            .iter()
            .filter(|(_, hashes)| {
                hashes.len() > 1
                    && hashes
                        .iter()
                        .collect::<std::collections::HashSet<_>>()
                        .len()
                        > 1
            })
            .collect();

        if !drifts.is_empty() {
            println!("\n⚠️  Content Drift Detected:");
            for (url, hashes) in drifts {
                println!("  URL: {}", url);
                println!("    Different content hashes: {}", hashes.len());
                for (i, hash) in hashes.iter().enumerate() {
                    println!("      Run {}: {}...", i + 1, &hash[..16]);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_metrics_from_response() {
        let response = json!({
            "success": true,
            "data": {
                "markdown": "# Test\n\nThis is a test page with content.",
                "html": "<html><body><h1>Test</h1><p>This is a test page with content.</p></body></html>",
                "metadata": {
                    "statusCode": 200,
                    "title": "Test Page",
                    "description": "A test page for testing",
                    "url": "https://example.com/final",
                    "contentType": "text/html; charset=utf-8"
                },
                "links": ["https://example.com/page1", "https://example.com/page2"],
                "images": ["https://example.com/image1.png"]
            }
        });

        let metrics = ScrapeMetrics::from_response(
            "https://example.com".to_string(),
            &response,
            Duration::from_millis(500),
        );

        assert!(metrics.success);
        assert_eq!(metrics.status_code, 200);
        assert_eq!(metrics.response_time_ms, 500);
        assert!(metrics.markdown_length > 0);
        assert!(metrics.html_length > 0);
        assert!(metrics.extraction_ratio.is_some());
        assert!(metrics.word_count > 0);
        assert!(metrics.content_hash.is_some());
        assert!(metrics.has_title);
        assert!(metrics.has_description);
        assert_eq!(metrics.link_count, 2);
        assert_eq!(metrics.image_count, 1);
        assert!(metrics.error.is_none());
        assert_eq!(
            metrics.final_url,
            Some("https://example.com/final".to_string())
        );
        assert_eq!(
            metrics.content_type,
            Some("text/html; charset=utf-8".to_string())
        );
    }

    #[test]
    fn test_percentile_calculation() {
        let data = vec![100, 200, 300, 400, 500, 600, 700, 800, 900, 1000];
        assert_eq!(MetricsCollection::percentile(&data, 50.0), 600);
        assert_eq!(MetricsCollection::percentile(&data, 90.0), 900);
        assert_eq!(MetricsCollection::percentile(&data, 99.0), 1000);
    }

    #[test]
    fn test_metrics_with_engine_tag() {
        let metrics = ScrapeMetrics::from_error(
            "https://example.com".to_string(),
            "Test error".to_string(),
            Duration::from_millis(100),
        )
        .with_engine_tag("chromium-v1.0")
        .with_attempts(3);

        assert_eq!(metrics.engine_tag, Some("chromium-v1.0".to_string()));
        assert_eq!(metrics.attempts, 3);
    }
}
