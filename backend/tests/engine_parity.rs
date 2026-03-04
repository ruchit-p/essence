// Engine parity testing - Compare two running engines via HTTP
// Compare content, performance, and quality metrics between engine versions
//
// Run with: cargo test --test engine_parity -- --ignored --nocapture
//
// Environment variables:
// - ENGINE_A_BASE: Base URL for engine A (e.g., "http://localhost:3000")
// - ENGINE_B_BASE: Base URL for engine B (e.g., "http://localhost:3001")
// - ESSENCE_PARITY_URLS: Comma-separated URLs to test (optional)

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::time::{Duration, Instant};

/// Parity test result for a single URL
#[derive(Debug, Clone)]
struct ParityResult {
    url: String,
    engine_a_success: bool,
    engine_b_success: bool,
    engine_a_time_ms: u128,
    engine_b_time_ms: u128,
    engine_a_hash: Option<String>,
    engine_b_hash: Option<String>,
    engine_a_word_count: usize,
    engine_b_word_count: usize,
    engine_a_markdown_len: usize,
    engine_b_markdown_len: usize,
    content_match: bool,
    time_diff_ms: i128,
    time_diff_pct: f64,
}

impl ParityResult {
    /// Calculate hash of markdown content
    fn calculate_hash(content: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Count words in text
    fn count_words(text: &str) -> usize {
        text.split_whitespace().filter(|w| !w.is_empty()).count()
    }

    /// Create parity result from two engine responses
    fn from_responses(url: String, engine_a: EngineResponse, engine_b: EngineResponse) -> Self {
        let engine_a_hash = engine_a.markdown.as_ref().map(|m| Self::calculate_hash(m));
        let engine_b_hash = engine_b.markdown.as_ref().map(|m| Self::calculate_hash(m));

        let content_match =
            engine_a_hash.is_some() && engine_b_hash.is_some() && engine_a_hash == engine_b_hash;

        let time_diff_ms = engine_b.response_time_ms as i128 - engine_a.response_time_ms as i128;
        let time_diff_pct = if engine_a.response_time_ms > 0 {
            (time_diff_ms as f64 / engine_a.response_time_ms as f64) * 100.0
        } else {
            0.0
        };

        ParityResult {
            url,
            engine_a_success: engine_a.success,
            engine_b_success: engine_b.success,
            engine_a_time_ms: engine_a.response_time_ms,
            engine_b_time_ms: engine_b.response_time_ms,
            engine_a_hash,
            engine_b_hash,
            engine_a_word_count: engine_a
                .markdown
                .as_ref()
                .map(|m| Self::count_words(m))
                .unwrap_or(0),
            engine_b_word_count: engine_b
                .markdown
                .as_ref()
                .map(|m| Self::count_words(m))
                .unwrap_or(0),
            engine_a_markdown_len: engine_a.markdown.as_ref().map(|m| m.len()).unwrap_or(0),
            engine_b_markdown_len: engine_b.markdown.as_ref().map(|m| m.len()).unwrap_or(0),
            content_match,
            time_diff_ms,
            time_diff_pct,
        }
    }

    /// Print comparison for this result
    fn print_comparison(&self) {
        println!("\nURL: {}", self.url);
        println!("{}", "-".repeat(80));

        // Success comparison
        println!(
            "Success:       A: {:5}  |  B: {:5}  {}",
            self.engine_a_success,
            self.engine_b_success,
            if self.engine_a_success == self.engine_b_success {
                "✓"
            } else {
                "✗ MISMATCH"
            }
        );

        // Performance comparison
        let faster = if self.time_diff_ms < 0 { "A" } else { "B" };
        println!(
            "Time (ms):     A: {:5}  |  B: {:5}  Δ: {:+6} ({:+.1}%) - {} faster",
            self.engine_a_time_ms,
            self.engine_b_time_ms,
            self.time_diff_ms,
            self.time_diff_pct,
            faster
        );

        // Content comparison
        println!(
            "Word count:    A: {:5}  |  B: {:5}  {}",
            self.engine_a_word_count,
            self.engine_b_word_count,
            if self.engine_a_word_count == self.engine_b_word_count {
                "✓"
            } else {
                "✗ DIFF"
            }
        );

        println!(
            "Markdown len:  A: {:5}  |  B: {:5}  {}",
            self.engine_a_markdown_len,
            self.engine_b_markdown_len,
            if self.engine_a_markdown_len == self.engine_b_markdown_len {
                "✓"
            } else {
                "✗ DIFF"
            }
        );

        // Hash comparison
        if let (Some(ref hash_a), Some(ref hash_b)) = (&self.engine_a_hash, &self.engine_b_hash) {
            println!("Content hash:  A: {}...", &hash_a[..16]);
            println!(
                "               B: {}...  {}",
                &hash_b[..16],
                if self.content_match {
                    "✓ MATCH"
                } else {
                    "✗ DIFFERENT CONTENT"
                }
            );
        }
    }
}

/// Response from a single engine
#[derive(Debug, Clone)]
struct EngineResponse {
    success: bool,
    response_time_ms: u128,
    markdown: Option<String>,
    error: Option<String>,
}

/// Load test URLs from environment or use defaults
fn get_parity_test_urls() -> Vec<String> {
    // First try environment variable
    if let Ok(urls_str) = env::var("ESSENCE_PARITY_URLS") {
        return urls_str.split(',').map(|s| s.trim().to_string()).collect();
    }

    // Default test URLs
    vec![
        "https://example.com".to_string(),
        "https://example.org".to_string(),
        "https://httpbin.org/html".to_string(),
    ]
}

/// Call a single engine
async fn call_engine(base_url: &str, test_url: &str) -> EngineResponse {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .unwrap();

    let payload = json!({
        "url": test_url,
        "formats": ["markdown", "html"]
    });

    let start = Instant::now();

    let result = client
        .post(format!("{}/v1/scrape", base_url))
        .json(&payload)
        .send()
        .await;

    let elapsed = start.elapsed();

    match result {
        Ok(response) => {
            let json: Value = response.json().await.unwrap_or(json!({}));
            let success = json["success"].as_bool().unwrap_or(false);

            let markdown = if success {
                json["data"]["markdown"].as_str().map(|s| s.to_string())
            } else {
                None
            };

            let error = if !success {
                json["error"].as_str().map(|s| s.to_string())
            } else {
                None
            };

            EngineResponse {
                success,
                response_time_ms: elapsed.as_millis(),
                markdown,
                error,
            }
        }
        Err(e) => EngineResponse {
            success: false,
            response_time_ms: elapsed.as_millis(),
            markdown: None,
            error: Some(e.to_string()),
        },
    }
}

#[tokio::test]
#[ignore]
async fn test_engine_parity() {
    let engine_a_base = env::var("ENGINE_A_BASE")
        .expect("ENGINE_A_BASE environment variable must be set (e.g., http://localhost:3000)");

    let engine_b_base = env::var("ENGINE_B_BASE")
        .expect("ENGINE_B_BASE environment variable must be set (e.g., http://localhost:3001)");

    let test_urls = get_parity_test_urls();

    println!("\n{}", "=".repeat(80));
    println!("ENGINE PARITY TEST");
    println!("{}", "=".repeat(80));
    println!("Engine A: {}", engine_a_base);
    println!("Engine B: {}", engine_b_base);
    println!("Test URLs: {}", test_urls.len());
    println!("{}", "=".repeat(80));

    let mut results = Vec::new();

    for (i, test_url) in test_urls.iter().enumerate() {
        println!("\n[{}/{}] Testing: {}", i + 1, test_urls.len(), test_url);

        // Call both engines
        let (engine_a_resp, engine_b_resp) = tokio::join!(
            call_engine(&engine_a_base, test_url),
            call_engine(&engine_b_base, test_url)
        );

        let result = ParityResult::from_responses(test_url.clone(), engine_a_resp, engine_b_resp);

        result.print_comparison();

        results.push(result);
    }

    // Print summary
    print_parity_summary(&results);

    // Save results
    save_parity_results(&results);
}

/// Print summary of parity testing
fn print_parity_summary(results: &[ParityResult]) {
    println!("\n{}", "=".repeat(80));
    println!("PARITY TEST SUMMARY");
    println!("{}", "=".repeat(80));

    let total = results.len();
    let content_matches = results.iter().filter(|r| r.content_match).count();
    let both_success = results
        .iter()
        .filter(|r| r.engine_a_success && r.engine_b_success)
        .count();

    let avg_time_a: f64 = results
        .iter()
        .map(|r| r.engine_a_time_ms as f64)
        .sum::<f64>()
        / total as f64;

    let avg_time_b: f64 = results
        .iter()
        .map(|r| r.engine_b_time_ms as f64)
        .sum::<f64>()
        / total as f64;

    let time_diff_pct = ((avg_time_b - avg_time_a) / avg_time_a) * 100.0;

    println!("Total URLs tested:        {}", total);
    println!(
        "Both engines succeeded:   {} ({:.1}%)",
        both_success,
        (both_success as f64 / total as f64) * 100.0
    );
    println!(
        "Content matches:          {} ({:.1}%)",
        content_matches,
        (content_matches as f64 / total as f64) * 100.0
    );
    println!();
    println!("Performance:");
    println!("  Engine A avg time:      {:.2}ms", avg_time_a);
    println!("  Engine B avg time:      {:.2}ms", avg_time_b);
    println!(
        "  Difference:             {:+.2}ms ({:+.1}%)",
        avg_time_b - avg_time_a,
        time_diff_pct
    );
    println!();

    // List content mismatches
    let mismatches: Vec<&ParityResult> = results
        .iter()
        .filter(|r| r.engine_a_success && r.engine_b_success && !r.content_match)
        .collect();

    if !mismatches.is_empty() {
        println!("⚠️  Content Mismatches:");
        for result in mismatches {
            println!("  - {}", result.url);
            println!(
                "    A: {} words, {} chars",
                result.engine_a_word_count, result.engine_a_markdown_len
            );
            println!(
                "    B: {} words, {} chars",
                result.engine_b_word_count, result.engine_b_markdown_len
            );
        }
    }

    println!("{}", "=".repeat(80));
    println!();
}

/// Save parity results to CSV
fn save_parity_results(results: &[ParityResult]) {
    let timestamp = chrono::Utc::now().format("%Y-%m-%d_%H-%M-%S");
    let csv_path = format!(
        "/Volumes/Flashdrive/essence/bench/results/engine_parity_{}.csv",
        timestamp
    );

    // Ensure directory exists
    if let Some(parent) = std::path::Path::new(&csv_path).parent() {
        fs::create_dir_all(parent).ok();
    }

    let mut csv = String::new();
    csv.push_str("url,engine_a_success,engine_b_success,engine_a_time_ms,engine_b_time_ms,time_diff_ms,time_diff_pct,engine_a_words,engine_b_words,engine_a_chars,engine_b_chars,content_match,engine_a_hash,engine_b_hash\n");

    for result in results {
        csv.push_str(&format!(
            "\"{}\",{},{},{},{},{},{:.2},{},{},{},{},{},\"{}\",\"{}\"\n",
            result.url,
            result.engine_a_success,
            result.engine_b_success,
            result.engine_a_time_ms,
            result.engine_b_time_ms,
            result.time_diff_ms,
            result.time_diff_pct,
            result.engine_a_word_count,
            result.engine_b_word_count,
            result.engine_a_markdown_len,
            result.engine_b_markdown_len,
            result.content_match,
            result.engine_a_hash.as_deref().unwrap_or(""),
            result.engine_b_hash.as_deref().unwrap_or(""),
        ));
    }

    match fs::write(&csv_path, csv) {
        Ok(_) => println!("✓ Parity results saved: {}", csv_path),
        Err(e) => eprintln!("✗ Failed to save parity results: {}", e),
    }
}

#[tokio::test]
#[ignore]
async fn test_engine_parity_extended() {
    let engine_a_base =
        env::var("ENGINE_A_BASE").expect("ENGINE_A_BASE environment variable must be set");

    let engine_b_base =
        env::var("ENGINE_B_BASE").expect("ENGINE_B_BASE environment variable must be set");

    // Extended URL list from test corpus
    let test_urls = ["https://example.com",
        "https://example.org",
        "https://quotes.toscrape.com",
        "https://books.toscrape.com",
        "https://httpbin.org/html",
        "https://httpbin.org/delay/2",
        "https://developer.mozilla.org/en-US/docs/Web/HTML",
        "https://ogp.me"];

    println!("\n{}", "=".repeat(80));
    println!("EXTENDED ENGINE PARITY TEST");
    println!("{}", "=".repeat(80));
    println!("Engine A: {}", engine_a_base);
    println!("Engine B: {}", engine_b_base);
    println!("Test URLs: {}", test_urls.len());
    println!("{}", "=".repeat(80));

    let mut results = Vec::new();

    for (i, test_url) in test_urls.iter().enumerate() {
        println!("\n[{}/{}] Testing: {}", i + 1, test_urls.len(), test_url);

        let (engine_a_resp, engine_b_resp) = tokio::join!(
            call_engine(&engine_a_base, test_url),
            call_engine(&engine_b_base, test_url)
        );

        let result =
            ParityResult::from_responses(test_url.to_string(), engine_a_resp, engine_b_resp);

        result.print_comparison();

        results.push(result);

        // Small delay between requests
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    print_parity_summary(&results);
    save_parity_results(&results);
}
