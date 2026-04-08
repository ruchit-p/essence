// Competitive Benchmark: Essence vs Firecrawl (Head-to-Head)
//
// Runs the same URL corpus through both Essence and Firecrawl, computes
// identical objective metrics, and produces a verifiable comparison report.
// Results are persisted to SQLite for historical tracking and dashboard generation.
//
// Run with: cargo test --test competitive_benchmark -- --ignored --nocapture
//
// Prerequisites:
//   - Essence: runs in-process (no setup needed)
//   - Firecrawl: self-hosted at FIRECRAWL_URL (default: http://localhost:3002)
//     Start with: cd firecrawl && docker compose up -d
//
// Environment variables:
//   FIRECRAWL_URL       - Firecrawl API base URL (default: http://localhost:3002)
//   FIRECRAWL_API_KEY   - Optional API key for Firecrawl cloud
//   BENCHMARK_TIMEOUT   - Per-URL timeout in ms (default: 30000)
//   BENCHMARK_THROTTLE  - Delay between requests in ms (default: 1000)
//   BENCHMARK_SUBSET    - Comma-separated categories to test (default: all)
//   SAVE_MARKDOWN       - Save raw markdown outputs for manual comparison (default: true)
//   LLM_JUDGE           - Enable LLM-as-judge evaluation via claude -p (default: false)
//   LLM_JUDGE_CONCURRENCY - Max parallel LLM evaluations (default: 15). Higher = faster but may hit rate limits.

mod api;
mod benchmark;

use api::{create_app, metrics::ScrapeMetrics, send_scrape_request};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Semaphore;
use tokio::time::{sleep, Duration};

// MARK: - Data Structures

/// A single URL entry from the benchmark corpus
#[derive(Debug, Clone)]
struct BenchmarkUrl {
    category: String,
    url: String,
    description: String,
    expected_min_words: usize,
}

/// Objective, verifiable metrics for a single scrape (engine-agnostic)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ObjectiveMetrics {
    /// Whether the scrape succeeded at all
    success: bool,
    /// Error message if failed
    error: Option<String>,

    // Content extraction metrics (higher = better)
    word_count: usize,
    markdown_length: usize,
    /// Number of headings preserved (# lines)
    heading_count: usize,
    /// Number of links preserved
    link_count: usize,
    /// Number of images preserved
    image_count: usize,
    /// Number of code blocks preserved
    code_block_count: usize,
    /// Number of tables preserved (markdown pipe tables)
    table_count: usize,

    // Quality metrics (derived, verifiable)
    /// Number of raw HTML closing tags leaked into markdown (lower = better)
    html_artifact_count: usize,
    /// Number of empty link texts like [](...) (lower = better)
    empty_link_count: usize,
    /// Number of base64 data URIs in output (lower = better)
    base64_count: usize,
    /// Content density: words per 1KB of markdown (higher = cleaner)
    content_density: f64,
    /// Number of markdown list items (diagnostic)
    list_count: usize,
    /// Whether title was extracted
    has_title: bool,
    /// Whether description was extracted
    has_description: bool,

    // Performance metrics
    response_time_ms: u128,

    // Content hash for reproducibility
    content_hash: String,
}

/// Head-to-head comparison result for a single URL
#[derive(Debug, Clone, Serialize, Deserialize)]
struct UrlComparison {
    url: String,
    category: String,
    description: String,
    expected_min_words: usize,

    essence: ObjectiveMetrics,
    firecrawl: ObjectiveMetrics,

    /// Per-dimension winner: "essence", "firecrawl", or "tie"
    winners: DimensionWinners,
    /// Heuristic winner (artifact-counting, diagnostic)
    overall_winner: String,
    /// Heuristic advantage score (-1.0 to 1.0, positive = Essence wins)
    essence_advantage: f64,
    /// Quality winner (LLM judge, authoritative)
    quality_winner: String,
    /// Speed winner
    speed_winner: String,
    /// Raw LLM verdict for DB/JSON
    llm_verdict: Option<serde_json::Value>,
}

/// Winner for each comparison dimension
#[derive(Debug, Clone, Serialize, Deserialize)]
struct DimensionWinners {
    word_count: String,
    heading_preservation: String,
    link_preservation: String,
    image_preservation: String,
    code_block_preservation: String,
    markdown_cleanliness: String,
    content_density: String,
    metadata_extraction: String,
    speed: String,
}

/// Aggregate comparison results per category
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CategoryComparison {
    category: String,
    url_count: usize,
    essence_wins: usize,
    firecrawl_wins: usize,
    ties: usize,
    essence_win_rate: f64,

    // Average metrics
    avg_essence_words: f64,
    avg_firecrawl_words: f64,
    avg_essence_speed_ms: f64,
    avg_firecrawl_speed_ms: f64,
    avg_essence_html_artifacts: f64,
    avg_firecrawl_html_artifacts: f64,
}

/// Full competitive benchmark output
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CompetitiveBenchmarkOutput {
    timestamp: String,
    firecrawl_url: String,
    total_urls: usize,

    // Heuristic results (artifact-counting, diagnostic)
    essence_wins: usize,
    firecrawl_wins: usize,
    ties: usize,
    essence_win_rate: f64,

    // Quality leaderboard (LLM judge, authoritative)
    quality_wins_essence: usize,
    quality_wins_firecrawl: usize,
    quality_ties: usize,
    quality_win_rate: f64,

    // Speed leaderboard
    speed_wins_essence: usize,
    speed_wins_firecrawl: usize,
    speed_ties: usize,
    speed_win_rate: f64,

    // Success rates
    essence_success_rate: f64,
    firecrawl_success_rate: f64,

    // Per-category results
    categories: Vec<CategoryComparison>,

    // Per-URL detailed results
    comparisons: Vec<UrlComparison>,

    // Dimension-level win rates (how often Essence wins on each metric)
    dimension_win_rates: HashMap<String, f64>,
}

// MARK: - Corpus Loading

/// Load the competitive benchmark corpus
fn load_benchmark_corpus() -> Vec<BenchmarkUrl> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let corpus_path = manifest_dir
        .parent()
        .unwrap()
        .join("docs/loop/benchmark_corpus.txt");

    let content = fs::read_to_string(&corpus_path).unwrap_or_else(|e| {
        panic!(
            "Failed to read benchmark corpus at {:?}: {}",
            corpus_path, e
        )
    });

    let subset_filter: Option<Vec<String>> = std::env::var("BENCHMARK_SUBSET")
        .ok()
        .map(|s| s.split(',').map(|c| c.trim().to_string()).collect());

    let mut urls = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split('|').map(|s| s.trim()).collect();
        if parts.len() >= 4 {
            let category = parts[0].to_string();
            if let Some(ref filter) = subset_filter {
                if !filter.contains(&category) {
                    continue;
                }
            }
            urls.push(BenchmarkUrl {
                category,
                url: parts[1].to_string(),
                description: parts[2].to_string(),
                expected_min_words: parts[3].parse().unwrap_or(50),
            });
        }
    }

    assert!(!urls.is_empty(), "Benchmark corpus is empty");
    urls
}

// MARK: - Objective Metric Computation

/// Compute objective metrics from raw markdown output and metadata.
/// These metrics are engine-agnostic and fully verifiable.
fn compute_objective_metrics(
    success: bool,
    error: Option<String>,
    markdown: &str,
    has_title: bool,
    has_description: bool,
    link_count: usize,
    image_count: usize,
    response_time_ms: u128,
) -> ObjectiveMetrics {
    if !success || markdown.is_empty() {
        return ObjectiveMetrics {
            success,
            error,
            word_count: 0,
            markdown_length: 0,
            heading_count: 0,
            link_count: 0,
            image_count: 0,
            code_block_count: 0,
            table_count: 0,
            html_artifact_count: 0,
            empty_link_count: 0,
            base64_count: 0,
            content_density: 0.0,
            list_count: 0,
            has_title,
            has_description,
            response_time_ms,
            content_hash: String::new(),
        };
    }

    let word_count = markdown
        .split_whitespace()
        .filter(|w| !w.is_empty())
        .count();

    let heading_count = markdown
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            trimmed.starts_with("# ")
                || trimmed.starts_with("## ")
                || trimmed.starts_with("### ")
                || trimmed.starts_with("#### ")
                || trimmed.starts_with("##### ")
                || trimmed.starts_with("###### ")
        })
        .count();

    let code_block_count = markdown.matches("```").count() / 2;

    // Count markdown pipe tables (lines with | that aren't in code blocks)
    let table_count = count_tables(markdown);

    let html_artifact_count = markdown.matches("</").count();

    let empty_link_count = markdown
        .match_indices("[](")
        .filter(|(pos, _)| *pos == 0 || markdown.as_bytes()[pos - 1] != b'!')
        .count();

    let base64_count =
        markdown.matches("data:image/").count() + markdown.matches("data:application/").count();

    let content_density = if markdown.len() > 0 {
        (word_count as f64) / (markdown.len() as f64 / 1024.0)
    } else {
        0.0
    };

    let list_count = markdown
        .lines()
        .filter(|l| {
            let t = l.trim_start();
            t.starts_with("- ")
                || t.starts_with("* ")
                || t.starts_with("+ ")
                || (t.len() > 2
                    && t.as_bytes()[0].is_ascii_digit()
                    && (t.contains(". ") || t.contains(") ")))
        })
        .count();

    let mut hasher = Sha256::new();
    hasher.update(markdown.as_bytes());
    let content_hash = format!("{:x}", hasher.finalize());

    ObjectiveMetrics {
        success,
        error,
        word_count,
        markdown_length: markdown.len(),
        heading_count,
        link_count,
        image_count,
        code_block_count,
        table_count,
        html_artifact_count,
        empty_link_count,
        base64_count,
        content_density,
        list_count,
        has_title,
        has_description,
        response_time_ms,
        content_hash,
    }
}

/// Count the number of markdown tables (groups of consecutive pipe-delimited lines)
fn count_tables(markdown: &str) -> usize {
    let mut count = 0;
    let mut in_table = false;
    let mut in_code_block = false;

    for line in markdown.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            in_code_block = !in_code_block;
            continue;
        }
        if in_code_block {
            continue;
        }
        if trimmed.contains('|') && trimmed.starts_with('|') {
            if !in_table {
                in_table = true;
                count += 1;
            }
        } else {
            in_table = false;
        }
    }
    count
}

// MARK: - Firecrawl Client

/// Scrape a URL using the Firecrawl API
async fn scrape_with_firecrawl(
    client: &reqwest::Client,
    firecrawl_url: &str,
    url: &str,
    timeout_ms: u64,
) -> (bool, String, bool, bool, usize, usize, u128, Option<String>) {
    let start = Instant::now();

    let payload = json!({
        "url": url,
        "formats": ["markdown"],
        "timeout": timeout_ms
    });

    let api_url = format!("{}/v1/scrape", firecrawl_url);
    let result = client
        .post(&api_url)
        .json(&payload)
        .timeout(Duration::from_millis(timeout_ms + 5000))
        .send()
        .await;

    let elapsed_ms = start.elapsed().as_millis();

    match result {
        Ok(resp) => {
            if let Ok(body) = resp.json::<serde_json::Value>().await {
                let success = body["success"].as_bool().unwrap_or(false);
                let markdown = body["data"]["markdown"].as_str().unwrap_or("").to_string();

                let metadata = &body["data"]["metadata"];
                let has_title = metadata["title"].is_string()
                    && !metadata["title"].as_str().unwrap_or("").is_empty();
                let has_description = metadata["description"].is_string()
                    && !metadata["description"].as_str().unwrap_or("").is_empty();

                // Count links from markdown since Firecrawl may not return them separately
                let link_count = markdown.matches("](").count();
                let image_count = markdown.matches("![").count();

                let error = if !success {
                    Some(body["error"].as_str().unwrap_or("Unknown").to_string())
                } else {
                    None
                };

                (
                    success,
                    markdown,
                    has_title,
                    has_description,
                    link_count,
                    image_count,
                    elapsed_ms,
                    error,
                )
            } else {
                (
                    false,
                    String::new(),
                    false,
                    false,
                    0,
                    0,
                    elapsed_ms,
                    Some("Failed to parse Firecrawl response".to_string()),
                )
            }
        }
        Err(e) => (
            false,
            String::new(),
            false,
            false,
            0,
            0,
            elapsed_ms,
            Some(format!("Firecrawl request failed: {}", e)),
        ),
    }
}

// MARK: - Comparison Logic

/// Determine winner for a numeric metric (higher = better)
fn winner_higher_better(essence: usize, firecrawl: usize) -> String {
    // Use a 10% tolerance band for ties
    let threshold = (essence.max(firecrawl) as f64 * 0.1).max(1.0) as usize;
    if essence > firecrawl + threshold {
        "essence".to_string()
    } else if firecrawl > essence + threshold {
        "firecrawl".to_string()
    } else {
        "tie".to_string()
    }
}

/// Determine winner for a numeric metric (lower = better)
fn winner_lower_better(essence: usize, firecrawl: usize) -> String {
    if essence == 0 && firecrawl == 0 {
        return "tie".to_string();
    }
    let threshold = (essence.max(firecrawl) as f64 * 0.1).max(1.0) as usize;
    if essence + threshold < firecrawl {
        "essence".to_string()
    } else if firecrawl + threshold < essence {
        "firecrawl".to_string()
    } else {
        "tie".to_string()
    }
}

/// Determine winner for speed (lower response time = better)
fn winner_speed(essence_ms: u128, firecrawl_ms: u128) -> String {
    // 20% tolerance for speed since network variance is high
    let threshold = (essence_ms.max(firecrawl_ms) as f64 * 0.2) as u128;
    if essence_ms + threshold < firecrawl_ms {
        "essence".to_string()
    } else if firecrawl_ms + threshold < essence_ms {
        "firecrawl".to_string()
    } else {
        "tie".to_string()
    }
}

/// Determine winner for content density (higher = better)
fn winner_density(essence: f64, firecrawl: f64) -> String {
    let threshold = essence.max(firecrawl) * 0.1;
    if essence > firecrawl + threshold {
        "essence".to_string()
    } else if firecrawl > essence + threshold {
        "firecrawl".to_string()
    } else {
        "tie".to_string()
    }
}

/// Determine winner for metadata extraction
fn winner_metadata(e: &ObjectiveMetrics, f: &ObjectiveMetrics) -> String {
    let e_score = e.has_title as u8 + e.has_description as u8;
    let f_score = f.has_title as u8 + f.has_description as u8;
    if e_score > f_score {
        "essence".to_string()
    } else if f_score > e_score {
        "firecrawl".to_string()
    } else {
        "tie".to_string()
    }
}

/// Compare two sets of metrics and determine winners per dimension
fn compare_metrics(essence: &ObjectiveMetrics, firecrawl: &ObjectiveMetrics) -> DimensionWinners {
    DimensionWinners {
        word_count: winner_higher_better(essence.word_count, firecrawl.word_count),
        heading_preservation: winner_higher_better(essence.heading_count, firecrawl.heading_count),
        link_preservation: winner_higher_better(essence.link_count, firecrawl.link_count),
        image_preservation: winner_higher_better(essence.image_count, firecrawl.image_count),
        code_block_preservation: winner_higher_better(
            essence.code_block_count,
            firecrawl.code_block_count,
        ),
        markdown_cleanliness: winner_lower_better(
            essence.html_artifact_count + essence.empty_link_count + essence.base64_count,
            firecrawl.html_artifact_count + firecrawl.empty_link_count + firecrawl.base64_count,
        ),
        content_density: winner_density(essence.content_density, firecrawl.content_density),
        metadata_extraction: winner_metadata(essence, firecrawl),
        speed: winner_speed(essence.response_time_ms, firecrawl.response_time_ms),
    }
}

/// Calculate the overall winner from dimension winners.
/// Weights reflect what matters for AI-agent-consumable scraping quality:
///   - Markdown cleanliness & code blocks: highest (clean output is everything)
///   - Speed, links, images, metadata: high (practical value)
///   - Headings: moderate (structural signal)
///   - Word count & density: low (more words ≠ better quality)
fn overall_winner(winners: &DimensionWinners) -> (String, f64) {
    let dimensions: Vec<(&str, f64)> = vec![
        (&winners.word_count, 0.5),
        (&winners.heading_preservation, 1.0),
        (&winners.link_preservation, 1.5),
        (&winners.image_preservation, 1.5),
        (&winners.code_block_preservation, 2.0),
        (&winners.markdown_cleanliness, 2.5),
        (&winners.content_density, 0.5),
        (&winners.metadata_extraction, 1.5),
        (&winners.speed, 2.0),
    ];

    let total_weight: f64 = dimensions.iter().map(|(_, w)| w).sum();
    let mut essence_score = 0.0;
    let mut firecrawl_score = 0.0;

    for (winner, weight) in &dimensions {
        match winner.as_ref() {
            "essence" => essence_score += weight,
            "firecrawl" => firecrawl_score += weight,
            _ => {
                // Ties split the weight
                essence_score += weight / 2.0;
                firecrawl_score += weight / 2.0;
            }
        }
    }

    // Normalize to -1.0 .. 1.0 range (positive = Essence advantage)
    let advantage = (essence_score - firecrawl_score) / total_weight;

    let winner = if advantage > 0.05 {
        "essence".to_string()
    } else if advantage < -0.05 {
        "firecrawl".to_string()
    } else {
        "tie".to_string()
    };

    (winner, advantage)
}

// MARK: - Markdown Output Saving

/// Optionally save raw markdown outputs for manual comparison
fn save_markdown_output(base_dir: &PathBuf, url: &str, engine: &str, markdown: &str) {
    let sanitized_url = url
        .replace("https://", "")
        .replace("http://", "")
        .replace('/', "_")
        .replace('?', "_")
        .replace('&', "_")
        .replace('=', "_");

    let dir = base_dir.join(engine);
    fs::create_dir_all(&dir).ok();
    let path = dir.join(format!("{}.md", sanitized_url));
    fs::write(&path, markdown).ok();
}

// MARK: - Main Benchmark Test

#[tokio::test]
#[ignore]
async fn competitive_benchmark_run() {
    let firecrawl_url =
        std::env::var("FIRECRAWL_URL").unwrap_or_else(|_| "http://localhost:3002".to_string());
    let timeout_ms: u64 = std::env::var("BENCHMARK_TIMEOUT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(30000);
    let throttle_ms: u64 = std::env::var("BENCHMARK_THROTTLE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1000);
    let save_markdown = std::env::var("SAVE_MARKDOWN")
        .map(|v| v != "false" && v != "0")
        .unwrap_or(true);

    let corpus = load_benchmark_corpus();
    let app = create_app();

    let http_client = reqwest::Client::builder()
        .timeout(Duration::from_millis(timeout_ms + 10000))
        .build()
        .expect("Failed to create HTTP client");

    // Verify Firecrawl is reachable
    println!("\n{}", "=".repeat(90));
    println!("COMPETITIVE BENCHMARK: Essence vs Firecrawl");
    println!("{}", "=".repeat(90));
    println!("Firecrawl URL: {}", firecrawl_url);
    println!("Timeout: {}ms | Throttle: {}ms", timeout_ms, throttle_ms);
    println!("Corpus: {} URLs", corpus.len());

    let firecrawl_reachable = http_client
        .get(&firecrawl_url)
        .timeout(Duration::from_secs(5))
        .send()
        .await
        .is_ok();

    if !firecrawl_reachable {
        println!("\nWARNING: Firecrawl not reachable at {}.", firecrawl_url);
        println!("Running in BASELINE-ONLY mode (Essence metrics only).");
        println!("To enable head-to-head: cd firecrawl && docker compose up -d");
    }

    println!("{}", "=".repeat(90));

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let output_dir = manifest_dir.parent().unwrap().join("docs/loop");
    let markdown_dir = output_dir.join("benchmark_outputs");

    let mut comparisons: Vec<UrlComparison> = Vec::new();

    // LLM judge setup: non-blocking, fires during scraping
    let llm_enabled =
        benchmark::llm_judge::is_enabled() && benchmark::llm_judge::is_claude_available();
    let llm_concurrency: usize = std::env::var("LLM_JUDGE_CONCURRENCY")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(15);
    if llm_enabled {
        println!("LLM Judge: ON (concurrency: {})", llm_concurrency);
    } else if benchmark::llm_judge::is_enabled() {
        println!("LLM Judge: ENABLED but `claude` CLI not found in PATH. Quality leaderboard will be empty.");
    } else {
        println!("LLM Judge: OFF (set LLM_JUDGE=true to enable)");
    }
    let llm_sem = Arc::new(Semaphore::new(llm_concurrency));
    let mut llm_handles: Vec<
        tokio::task::JoinHandle<(usize, String, benchmark::llm_judge::EvalResult)>,
    > = Vec::new();

    for (i, bench_url) in corpus.iter().enumerate() {
        println!(
            "\n[{}/{}] [{}] {}",
            i + 1,
            corpus.len(),
            bench_url.category,
            bench_url.url
        );

        // --- Scrape with Essence ---
        // Only request markdown (same as Firecrawl) for fair speed comparison.
        // Link/image counts are derived from the markdown text.
        let essence_payload = json!({
            "url": bench_url.url,
            "engine": "auto",
            "formats": ["markdown"],
            "timeout": timeout_ms
        });

        let essence_start = Instant::now();
        let essence_response = send_scrape_request(&app, essence_payload).await;
        let essence_elapsed = essence_start.elapsed();

        let e_success = essence_response["success"].as_bool().unwrap_or(false);
        let e_markdown = essence_response["data"]["markdown"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let e_metadata = &essence_response["data"]["metadata"];
        let e_has_title = e_metadata["title"].is_string()
            && !e_metadata["title"].as_str().unwrap_or("").is_empty();
        let e_has_description = e_metadata["description"].is_string()
            && !e_metadata["description"].as_str().unwrap_or("").is_empty();
        // Count links/images from markdown (same method as Firecrawl for fairness)
        let e_link_count = e_markdown.matches("](").count();
        let e_image_count = e_markdown.matches("![").count();
        let e_error = if !e_success {
            Some(
                essence_response["error"]
                    .as_str()
                    .unwrap_or("Unknown")
                    .to_string(),
            )
        } else {
            None
        };

        let essence_metrics = compute_objective_metrics(
            e_success,
            e_error,
            &e_markdown,
            e_has_title,
            e_has_description,
            e_link_count,
            e_image_count,
            essence_elapsed.as_millis(),
        );

        print!(
            "  Essence:   {} | {}ms | {} words | {} headings | {} links",
            if e_success { "OK" } else { "FAIL" },
            essence_elapsed.as_millis(),
            essence_metrics.word_count,
            essence_metrics.heading_count,
            essence_metrics.link_count
        );

        // --- Scrape with Firecrawl ---
        let (firecrawl_metrics, f_markdown_raw) = if firecrawl_reachable {
            let (
                f_success,
                f_markdown,
                f_has_title,
                f_has_description,
                f_link_count,
                f_image_count,
                f_elapsed,
                f_error,
            ) = scrape_with_firecrawl(&http_client, &firecrawl_url, &bench_url.url, timeout_ms)
                .await;

            if save_markdown {
                save_markdown_output(&markdown_dir, &bench_url.url, "essence", &e_markdown);
                save_markdown_output(&markdown_dir, &bench_url.url, "firecrawl", &f_markdown);
            }

            let metrics = compute_objective_metrics(
                f_success,
                f_error,
                &f_markdown,
                f_has_title,
                f_has_description,
                f_link_count,
                f_image_count,
                f_elapsed,
            );

            println!(
                "\n  Firecrawl: {} | {}ms | {} words | {} headings | {} links",
                if f_success { "OK" } else { "FAIL" },
                f_elapsed,
                metrics.word_count,
                metrics.heading_count,
                metrics.link_count
            );

            (metrics, f_markdown)
        } else {
            // Load from baselines if available, otherwise use empty metrics
            println!(" (no Firecrawl)");
            let metrics =
                load_firecrawl_baseline(&output_dir, &bench_url.url).unwrap_or_else(|| {
                    ObjectiveMetrics {
                        success: false,
                        error: Some("Firecrawl not available".to_string()),
                        word_count: 0,
                        markdown_length: 0,
                        heading_count: 0,
                        link_count: 0,
                        image_count: 0,
                        code_block_count: 0,
                        table_count: 0,
                        html_artifact_count: 0,
                        empty_link_count: 0,
                        base64_count: 0,
                        content_density: 0.0,
                        list_count: 0,
                        has_title: false,
                        has_description: false,
                        response_time_ms: 0,
                        content_hash: String::new(),
                    }
                });
            (metrics, String::new())
        };

        // --- Compare ---
        let winners = compare_metrics(&essence_metrics, &firecrawl_metrics);
        let (overall_win, advantage) = overall_winner(&winners);

        let winner_display = match overall_win.as_str() {
            "essence" => ">>> ESSENCE WINS <<<",
            "firecrawl" => "<<< FIRECRAWL WINS >>>",
            _ => "=== TIE ===",
        };
        println!("  Result: {} (advantage: {:.2})", winner_display, advantage);

        let speed_win = winners.speed.clone();

        comparisons.push(UrlComparison {
            url: bench_url.url.clone(),
            category: bench_url.category.clone(),
            description: bench_url.description.clone(),
            expected_min_words: bench_url.expected_min_words,
            essence: essence_metrics,
            firecrawl: firecrawl_metrics,
            winners,
            overall_winner: overall_win,
            essence_advantage: advantage,
            quality_winner: "pending".to_string(),
            speed_winner: speed_win,
            llm_verdict: None,
        });

        // Fire off non-blocking LLM evaluation for this URL pair
        if llm_enabled && !e_markdown.is_empty() && !f_markdown_raw.is_empty() {
            let sem = llm_sem.clone();
            let url = bench_url.url.clone();
            let desc = bench_url.description.clone();
            let e_md = e_markdown.clone();
            let f_md = f_markdown_raw.clone();
            let idx = comparisons.len() - 1;
            llm_handles.push(tokio::spawn(async move {
                let _permit = sem.acquire().await.expect("semaphore closed");
                let url_ret = url.clone();
                let result = tokio::task::spawn_blocking(move || {
                    benchmark::llm_judge::evaluate(&url, &desc, &e_md, &f_md)
                })
                .await
                .expect("spawn_blocking panicked");
                (idx, url_ret, result)
            }));
        }

        // Throttle between requests
        sleep(Duration::from_millis(throttle_ms)).await;
    }

    // MARK: - Collect LLM Judge Results (non-blocking, fired during scraping)

    if !llm_handles.is_empty() {
        println!(
            "\nWaiting for {} LLM judge evaluations to complete...",
            llm_handles.len()
        );
        let llm_results = futures::future::join_all(llm_handles).await;
        let mut llm_ok = 0usize;
        let mut llm_err = 0usize;
        for result in llm_results {
            if let Ok((idx, comp_url, eval_result)) = result {
                if let Some(ref err) = eval_result.error {
                    eprintln!("  LLM {} ... ERROR: {}", comp_url, err);
                    llm_err += 1;
                } else {
                    let winner = eval_result.raw_json["overall_winner"]
                        .as_str()
                        .unwrap_or("tie");
                    println!(
                        "  LLM {} ... {} ({}ms)",
                        comp_url, winner, eval_result.elapsed_ms
                    );
                    comparisons[idx].quality_winner = winner.to_string();
                    comparisons[idx].llm_verdict = Some(eval_result.raw_json);
                    llm_ok += 1;
                }
            }
        }
        println!("LLM Judge: {} succeeded, {} failed", llm_ok, llm_err);
    }

    // MARK: - Aggregate Results

    // Heuristic leaderboard (artifact-counting, diagnostic)
    let essence_wins = comparisons
        .iter()
        .filter(|c| c.overall_winner == "essence")
        .count();
    let firecrawl_wins = comparisons
        .iter()
        .filter(|c| c.overall_winner == "firecrawl")
        .count();
    let ties = comparisons
        .iter()
        .filter(|c| c.overall_winner == "tie")
        .count();
    let total = comparisons.len();
    let essence_win_rate = essence_wins as f64 / total.max(1) as f64;

    // Quality leaderboard (LLM judge)
    let quality_wins_essence = comparisons
        .iter()
        .filter(|c| c.quality_winner == "essence")
        .count();
    let quality_wins_firecrawl = comparisons
        .iter()
        .filter(|c| c.quality_winner == "firecrawl")
        .count();
    let quality_ties = comparisons
        .iter()
        .filter(|c| c.quality_winner == "tie")
        .count();
    let quality_evaluated = quality_wins_essence + quality_wins_firecrawl + quality_ties;
    let quality_win_rate = if quality_evaluated > 0 {
        quality_wins_essence as f64 / quality_evaluated as f64
    } else {
        0.0
    };

    // Speed leaderboard
    let speed_wins_essence = comparisons
        .iter()
        .filter(|c| c.speed_winner == "essence")
        .count();
    let speed_wins_firecrawl = comparisons
        .iter()
        .filter(|c| c.speed_winner == "firecrawl")
        .count();
    let speed_ties = comparisons
        .iter()
        .filter(|c| c.speed_winner == "tie")
        .count();
    let speed_win_rate = speed_wins_essence as f64 / total.max(1) as f64;

    let essence_successes = comparisons.iter().filter(|c| c.essence.success).count();
    let firecrawl_successes = comparisons.iter().filter(|c| c.firecrawl.success).count();

    // Per-category aggregation
    let mut cat_groups: HashMap<String, Vec<&UrlComparison>> = HashMap::new();
    for comp in &comparisons {
        cat_groups
            .entry(comp.category.clone())
            .or_default()
            .push(comp);
    }

    let mut categories: Vec<CategoryComparison> = cat_groups
        .iter()
        .map(|(cat, comps)| {
            let e_wins = comps
                .iter()
                .filter(|c| c.overall_winner == "essence")
                .count();
            let f_wins = comps
                .iter()
                .filter(|c| c.overall_winner == "firecrawl")
                .count();
            let t = comps.iter().filter(|c| c.overall_winner == "tie").count();

            CategoryComparison {
                category: cat.clone(),
                url_count: comps.len(),
                essence_wins: e_wins,
                firecrawl_wins: f_wins,
                ties: t,
                essence_win_rate: e_wins as f64 / comps.len().max(1) as f64,
                avg_essence_words: comps
                    .iter()
                    .map(|c| c.essence.word_count as f64)
                    .sum::<f64>()
                    / comps.len() as f64,
                avg_firecrawl_words: comps
                    .iter()
                    .map(|c| c.firecrawl.word_count as f64)
                    .sum::<f64>()
                    / comps.len() as f64,
                avg_essence_speed_ms: comps
                    .iter()
                    .map(|c| c.essence.response_time_ms as f64)
                    .sum::<f64>()
                    / comps.len() as f64,
                avg_firecrawl_speed_ms: comps
                    .iter()
                    .map(|c| c.firecrawl.response_time_ms as f64)
                    .sum::<f64>()
                    / comps.len() as f64,
                avg_essence_html_artifacts: comps
                    .iter()
                    .map(|c| c.essence.html_artifact_count as f64)
                    .sum::<f64>()
                    / comps.len() as f64,
                avg_firecrawl_html_artifacts: comps
                    .iter()
                    .map(|c| c.firecrawl.html_artifact_count as f64)
                    .sum::<f64>()
                    / comps.len() as f64,
            }
        })
        .collect();

    categories.sort_by(|a, b| b.essence_win_rate.partial_cmp(&a.essence_win_rate).unwrap());

    // Dimension-level win rates
    let mut dimension_wins: HashMap<String, (usize, usize)> = HashMap::new();
    for comp in &comparisons {
        let dims = vec![
            ("word_count", &comp.winners.word_count),
            ("heading_preservation", &comp.winners.heading_preservation),
            ("link_preservation", &comp.winners.link_preservation),
            ("image_preservation", &comp.winners.image_preservation),
            (
                "code_block_preservation",
                &comp.winners.code_block_preservation,
            ),
            ("markdown_cleanliness", &comp.winners.markdown_cleanliness),
            ("content_density", &comp.winners.content_density),
            ("metadata_extraction", &comp.winners.metadata_extraction),
            ("speed", &comp.winners.speed),
        ];
        for (dim, winner) in dims {
            let entry = dimension_wins.entry(dim.to_string()).or_insert((0, 0));
            if winner == "essence" {
                entry.0 += 1;
            }
            entry.1 += 1;
        }
    }
    let dimension_win_rates: HashMap<String, f64> = dimension_wins
        .iter()
        .map(|(k, (wins, total))| (k.clone(), *wins as f64 / *total as f64))
        .collect();

    // MARK: - Print Summary

    println!("\n{}", "=".repeat(90));
    println!("COMPETITIVE BENCHMARK RESULTS");
    println!("{}", "=".repeat(90));

    if quality_evaluated > 0 {
        println!("\n  === QUALITY LEADERBOARD (LLM Judge) ===");
        println!(
            "  Essence {} - {} Firecrawl ({} ties) | {:.1}% quality win rate",
            quality_wins_essence,
            quality_wins_firecrawl,
            quality_ties,
            quality_win_rate * 100.0
        );
    }

    println!("\n  === SPEED LEADERBOARD ===");
    println!(
        "  Essence {} - {} Firecrawl ({} ties) | {:.1}% speed win rate",
        speed_wins_essence,
        speed_wins_firecrawl,
        speed_ties,
        speed_win_rate * 100.0
    );

    println!("\n  === HEURISTIC (Artifact Counting — diagnostic) ===");
    println!(
        "  Essence {} - {} Firecrawl ({} ties) | {:.1}% heuristic win rate",
        essence_wins,
        firecrawl_wins,
        ties,
        essence_win_rate * 100.0
    );
    println!(
        "\n  Success Rates: Essence {:.1}% | Firecrawl {:.1}%",
        essence_successes as f64 / total as f64 * 100.0,
        firecrawl_successes as f64 / total as f64 * 100.0
    );

    println!("\n  Per-Category:");
    println!(
        "  {:15} {:>5} {:>5} {:>5} {:>5}  {:>10}",
        "Category", "URLs", "E Win", "F Win", "Tie", "E WinRate"
    );
    println!("  {}", "-".repeat(55));
    for cat in &categories {
        println!(
            "  {:15} {:>5} {:>5} {:>5} {:>5}  {:>9.1}%",
            cat.category,
            cat.url_count,
            cat.essence_wins,
            cat.firecrawl_wins,
            cat.ties,
            cat.essence_win_rate * 100.0
        );
    }

    println!("\n  Per-Dimension Win Rates:");
    let mut dims: Vec<_> = dimension_win_rates.iter().collect();
    dims.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap());
    for (dim, rate) in &dims {
        let bar_len = (*rate * 30.0) as usize;
        let bar: String = "#".repeat(bar_len);
        println!("  {:25} {:>5.1}% {}", dim, *rate * 100.0, bar);
    }

    // Print Firecrawl wins (areas to improve)
    let fc_wins: Vec<_> = comparisons
        .iter()
        .filter(|c| c.overall_winner == "firecrawl")
        .collect();
    if !fc_wins.is_empty() {
        println!("\n  AREAS WHERE FIRECRAWL WINS (improvement targets):");
        for comp in &fc_wins {
            println!(
                "    [{}] {} (advantage: {:.2})",
                comp.category, comp.url, comp.essence_advantage
            );
            // Show which dimensions Firecrawl won
            let dims = [
                ("words", &comp.winners.word_count),
                ("headings", &comp.winners.heading_preservation),
                ("links", &comp.winners.link_preservation),
                ("images", &comp.winners.image_preservation),
                ("code", &comp.winners.code_block_preservation),
                ("cleanliness", &comp.winners.markdown_cleanliness),
                ("density", &comp.winners.content_density),
                ("metadata", &comp.winners.metadata_extraction),
                ("speed", &comp.winners.speed),
            ];
            let fc_dims: Vec<&&str> = dims
                .iter()
                .filter(|(_, w)| w.as_str() == "firecrawl")
                .map(|(name, _)| name)
                .collect();
            if !fc_dims.is_empty() {
                println!("      Firecrawl leads in: {:?}", fc_dims);
            }
        }
    }

    println!("\n{}", "=".repeat(90));

    // MARK: - Save Results

    let output = CompetitiveBenchmarkOutput {
        timestamp: chrono::Utc::now().to_rfc3339(),
        firecrawl_url: firecrawl_url.clone(),
        total_urls: total,
        essence_wins,
        firecrawl_wins,
        ties,
        essence_win_rate,
        quality_wins_essence,
        quality_wins_firecrawl,
        quality_ties,
        quality_win_rate,
        speed_wins_essence,
        speed_wins_firecrawl,
        speed_ties,
        speed_win_rate,
        essence_success_rate: essence_successes as f64 / total as f64,
        firecrawl_success_rate: firecrawl_successes as f64 / total as f64,
        categories,
        comparisons: comparisons.clone(),
        dimension_win_rates,
    };

    let scores_path = output_dir.join("competitive_scores.json");
    let json = serde_json::to_string_pretty(&output).unwrap();
    fs::write(&scores_path, &json).expect("Failed to write competitive_scores.json");
    println!("Results written to: {:?}", scores_path);

    // Save Firecrawl baselines if we got live data
    if firecrawl_reachable {
        save_firecrawl_baselines(&output_dir, &comparisons);
        println!("Firecrawl baselines updated.");
    }

    // Write improvement targets (Firecrawl wins) as actionable items
    let targets: Vec<serde_json::Value> = comparisons
        .iter()
        .filter(|c| c.overall_winner == "firecrawl")
        .map(|c| {
            let weak_dims: Vec<String> = [
                ("word_count", &c.winners.word_count),
                ("heading_preservation", &c.winners.heading_preservation),
                ("link_preservation", &c.winners.link_preservation),
                ("image_preservation", &c.winners.image_preservation),
                (
                    "code_block_preservation",
                    &c.winners.code_block_preservation,
                ),
                ("markdown_cleanliness", &c.winners.markdown_cleanliness),
                ("content_density", &c.winners.content_density),
                ("metadata_extraction", &c.winners.metadata_extraction),
                ("speed", &c.winners.speed),
            ]
            .iter()
            .filter(|(_, w)| w.as_str() == "firecrawl")
            .map(|(name, _)| name.to_string())
            .collect();

            json!({
                "url": c.url,
                "category": c.category,
                "essence_advantage": c.essence_advantage,
                "firecrawl_word_count": c.firecrawl.word_count,
                "essence_word_count": c.essence.word_count,
                "firecrawl_headings": c.firecrawl.heading_count,
                "essence_headings": c.essence.heading_count,
                "weak_dimensions": weak_dims,
                "priority": if c.essence_advantage < -0.3 { "high" }
                    else if c.essence_advantage < -0.1 { "medium" }
                    else { "low" }
            })
        })
        .collect();

    let targets_path = output_dir.join("improvement_targets.json");
    let targets_json = serde_json::to_string_pretty(&targets).unwrap();
    fs::write(&targets_path, &targets_json).expect("Failed to write improvement_targets.json");
    println!("Improvement targets written to: {:?}", targets_path);

    // MARK: - SQLite Persistence

    let db_path = output_dir.join("benchmark.db");
    match benchmark::db::open_db(&db_path) {
        Ok(conn) => {
            let timestamp = &output.timestamp;
            match benchmark::db::insert_run(
                &conn,
                timestamp,
                total,
                essence_wins,
                firecrawl_wins,
                ties,
                essence_win_rate,
                output.essence_success_rate,
                output.firecrawl_success_rate,
                quality_win_rate,
                speed_win_rate,
            ) {
                Ok(run_id) => {
                    println!("SQLite: Run #{} saved to {:?}", run_id, db_path);

                    // Serialize comparisons to JSON values for the generic insert API
                    let comp_values: Vec<serde_json::Value> = comparisons
                        .iter()
                        .map(|c| serde_json::to_value(c).unwrap())
                        .collect();
                    if let Err(e) = benchmark::db::insert_url_results(&conn, run_id, &comp_values) {
                        eprintln!("SQLite: Failed to insert URL results: {}", e);
                    }

                    // MARK: - LLM Verdicts to DB (already collected non-blocking)

                    for comp in &comparisons {
                        if let Some(ref verdict) = comp.llm_verdict {
                            if let Err(e) = benchmark::db::insert_llm_verdict(
                                &conn, run_id, &comp.url, verdict, "claude",
                                0, // elapsed tracked per-verdict during collection
                            ) {
                                eprintln!(
                                    "SQLite: Failed to insert LLM verdict for {}: {}",
                                    comp.url, e
                                );
                            }
                        }
                    }

                    // MARK: - Dashboard Generation

                    match benchmark::db::load_dashboard_data(&conn) {
                        Ok(dashboard_data) => {
                            let dashboard_path = output_dir.join("dashboard.html");
                            benchmark::dashboard::generate(&dashboard_data, &dashboard_path);
                            println!("Dashboard written to: {:?}", dashboard_path);
                        }
                        Err(e) => eprintln!("Failed to load dashboard data: {}", e),
                    }
                }
                Err(e) => eprintln!("SQLite: Failed to insert run: {}", e),
            }
        }
        Err(e) => eprintln!("SQLite: Failed to open database at {:?}: {}", db_path, e),
    }

    println!("\n{}", "=".repeat(90));
}

// MARK: - Firecrawl Baseline Persistence

/// Save Firecrawl metrics as baselines for offline comparison
fn save_firecrawl_baselines(output_dir: &PathBuf, comparisons: &[UrlComparison]) {
    let baselines: HashMap<String, &ObjectiveMetrics> = comparisons
        .iter()
        .filter(|c| c.firecrawl.success)
        .map(|c| (c.url.clone(), &c.firecrawl))
        .collect();

    let output = json!({
        "captured_date": chrono::Utc::now().format("%Y-%m-%d").to_string(),
        "note": "Firecrawl baseline metrics captured during competitive benchmark. Refresh by running the benchmark with Firecrawl running.",
        "url_count": baselines.len(),
        "results": baselines
    });

    let path = output_dir.join("firecrawl_baselines.json");
    let json = serde_json::to_string_pretty(&output).unwrap();
    fs::write(&path, &json).ok();
}

/// Load a Firecrawl baseline for a specific URL (for offline comparison)
fn load_firecrawl_baseline(output_dir: &PathBuf, url: &str) -> Option<ObjectiveMetrics> {
    let path = output_dir.join("firecrawl_baselines.json");
    let content = fs::read_to_string(&path).ok()?;
    let data: serde_json::Value = serde_json::from_str(&content).ok()?;
    let result = data["results"].get(url)?;
    serde_json::from_value(result.clone()).ok()
}

// MARK: - Essence-Only Quick Benchmark (no Firecrawl dependency)

/// Run Essence-only benchmark and write scores for the quality loop.
/// Use this when Firecrawl is not available to still track Essence improvements.
#[tokio::test]
#[ignore]
async fn essence_benchmark_run() {
    let corpus = load_benchmark_corpus();
    let app = create_app();

    println!("\n{}", "=".repeat(90));
    println!("ESSENCE BENCHMARK (standalone, {} URLs)", corpus.len());
    println!("{}", "=".repeat(90));

    let mut results: Vec<serde_json::Value> = Vec::new();

    for (i, bench_url) in corpus.iter().enumerate() {
        let payload = json!({
            "url": bench_url.url,
            "engine": "auto",
            "formats": ["markdown", "html", "links", "images"],
            "timeout": 30000
        });

        let start = Instant::now();
        let response = send_scrape_request(&app, payload).await;
        let elapsed = start.elapsed();

        let success = response["success"].as_bool().unwrap_or(false);
        let markdown = response["data"]["markdown"].as_str().unwrap_or("");
        let metrics = ScrapeMetrics::from_response(bench_url.url.clone(), &response, elapsed);

        let objective = compute_objective_metrics(
            success,
            if !success {
                Some(response["error"].as_str().unwrap_or("Unknown").to_string())
            } else {
                None
            },
            markdown,
            metrics.has_title,
            metrics.has_description,
            metrics.link_count,
            metrics.image_count,
            elapsed.as_millis(),
        );

        println!(
            "[{}/{}] [{}] {} | {} | {}ms | {} words | {} headings",
            i + 1,
            corpus.len(),
            bench_url.category,
            bench_url.url,
            if success { "OK" } else { "FAIL" },
            elapsed.as_millis(),
            objective.word_count,
            objective.heading_count
        );

        results.push(json!({
            "url": bench_url.url,
            "category": bench_url.category,
            "description": bench_url.description,
            "metrics": objective
        }));

        sleep(Duration::from_millis(500)).await;
    }

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let output_dir = manifest_dir.parent().unwrap().join("docs/loop");
    let path = output_dir.join("essence_benchmark.json");
    let json = serde_json::to_string_pretty(&results).unwrap();
    fs::write(&path, &json).expect("Failed to write essence_benchmark.json");
    println!("\nEssence benchmark written to: {:?}", path);
}

// MARK: - Dashboard Regeneration (no scraping, uses existing DB)

#[tokio::test]
#[ignore]
async fn regenerate_dashboard() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let output_dir = manifest_dir.parent().unwrap().join("docs/loop");
    let db_path = output_dir.join("benchmark.db");

    if !db_path.exists() {
        println!(
            "No benchmark.db found at {:?}. Run the competitive benchmark first.",
            db_path
        );
        return;
    }

    match benchmark::db::open_db(&db_path) {
        Ok(conn) => match benchmark::db::load_dashboard_data(&conn) {
            Ok(data) => {
                let dashboard_path = output_dir.join("dashboard.html");
                benchmark::dashboard::generate(&data, &dashboard_path);
                println!("Dashboard regenerated: {:?}", dashboard_path);
                println!(
                    "Runs: {}, URL results: {}, LLM verdicts: {}",
                    data.runs.len(),
                    data.latest_url_results.len(),
                    data.latest_llm_verdicts.len()
                );
            }
            Err(e) => eprintln!("Failed to load dashboard data: {}", e),
        },
        Err(e) => eprintln!("Failed to open database: {}", e),
    }
}
