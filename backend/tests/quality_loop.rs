// Quality Loop Integration Test
// Reads test corpus, scrapes each URL, computes quality scores, writes to docs/loop/scores.json
// Run with: cargo test --test quality_loop -- --ignored --nocapture

mod api;

use api::{create_app, metrics::ScrapeMetrics, send_scrape_request};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;
use tokio::time::{sleep, Duration};

/// Test URL entry from the corpus
#[derive(Debug, Clone)]
struct TestUrl {
    category: String,
    url: String,
    description: String,
    expected_min_words: usize,
}

/// Quality scores for a single URL (each 0.0 - 10.0)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct UrlScore {
    url: String,
    category: String,
    description: String,
    success: bool,
    error: Option<String>,

    // Raw metrics
    word_count: usize,
    markdown_length: usize,
    html_length: usize,
    response_time_ms: u128,
    has_title: bool,
    has_description: bool,
    link_count: usize,
    image_count: usize,
    content_hash: Option<String>,

    // Computed quality scores (0.0 - 10.0)
    markdown_quality: f64,
    completeness: f64,
    speed: f64,
    reliability: f64,
    overall: f64,
}

/// Aggregate scores per category
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CategoryScore {
    url_count: usize,
    success_count: usize,
    success_rate: f64,
    avg_markdown_quality: f64,
    avg_completeness: f64,
    avg_speed: f64,
    avg_reliability: f64,
    avg_overall: f64,
}

/// Full scores output
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ScoresOutput {
    timestamp: String,
    cycle: u32,
    total_urls: usize,
    total_success: usize,
    overall_score: f64,
    categories: HashMap<String, CategoryScore>,
    urls: Vec<UrlScore>,
}

/// Load the test corpus from docs/loop/test_corpus.txt
fn load_corpus() -> Vec<TestUrl> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let corpus_path = manifest_dir
        .parent()
        .unwrap()
        .join("docs/loop/test_corpus.txt");

    let content = fs::read_to_string(&corpus_path)
        .unwrap_or_else(|e| panic!("Failed to read test corpus at {:?}: {}", corpus_path, e));

    let mut urls = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split('|').map(|s| s.trim()).collect();
        if parts.len() >= 4 {
            urls.push(TestUrl {
                category: parts[0].to_string(),
                url: parts[1].to_string(),
                description: parts[2].to_string(),
                expected_min_words: parts[3].parse().unwrap_or(50),
            });
        }
    }

    assert!(!urls.is_empty(), "Test corpus is empty");
    urls
}

/// Compute markdown quality score (0-10)
fn score_markdown_quality(metrics: &ScrapeMetrics, markdown: &str) -> f64 {
    if !metrics.success || markdown.is_empty() {
        return 0.0;
    }

    let mut score: f64 = 5.0; // Start at baseline

    // Word count bonus (more content = better extraction)
    if metrics.word_count > 500 {
        score += 1.0;
    }
    if metrics.word_count > 1000 {
        score += 0.5;
    }

    // Title/description extraction
    if metrics.has_title {
        score += 0.5;
    }
    if metrics.has_description {
        score += 0.5;
    }

    // Penalize HTML artifacts in markdown (use closing tags as reliable indicator)
    let html_tag_count = markdown.matches("</").count();
    let html_artifact_ratio = html_tag_count as f64 / markdown.len().max(1) as f64;
    if html_artifact_ratio > 0.01 {
        score -= 2.0;
    } else if html_artifact_ratio > 0.001 {
        score -= 1.0;
    }

    // Penalize excessive blank lines (more than 3 consecutive)
    if markdown.contains("\n\n\n\n") {
        score -= 0.5;
    }

    // Penalize if markdown is suspiciously short relative to HTML
    // Only apply when word count is also low - a page with 500+ clean words
    // shouldn't be penalized for having a large HTML source (good extraction)
    if let Some(ratio) = metrics.extraction_ratio {
        if ratio < 0.05 && metrics.word_count < 200 {
            score -= 1.5;
        } else if ratio < 0.1 && metrics.word_count < 200 {
            score -= 0.5;
        }
    }

    // Bonus for links extracted (sign of good structural preservation)
    if metrics.link_count > 5 {
        score += 0.5;
    }

    // Bonus for proper heading structure
    if markdown.contains("# ") {
        score += 0.5;
    }

    // Bonus for code blocks (good structural preservation of code content)
    if markdown.contains("```") {
        score += 0.5;
    }

    // Bonus for image extraction (sign of good content preservation)
    if metrics.image_count > 0 {
        score += 0.5;
    }

    // Bonus for clean markdown (zero HTML artifacts)
    if html_tag_count == 0 && markdown.len() > 100 {
        score += 0.5;
    }

    score.clamp(0.0, 10.0)
}

/// Compute completeness score (0-10)
fn score_completeness(metrics: &ScrapeMetrics, expected_min_words: usize) -> f64 {
    if !metrics.success {
        return 0.0;
    }

    let word_ratio = metrics.word_count as f64 / expected_min_words.max(1) as f64;

    if word_ratio >= 2.0 {
        10.0
    } else if word_ratio >= 1.5 {
        9.0
    } else if word_ratio >= 1.0 {
        8.0
    } else if word_ratio >= 0.75 {
        6.0
    } else if word_ratio >= 0.5 {
        4.0
    } else if word_ratio >= 0.25 {
        2.0
    } else {
        1.0
    }
}

/// Compute speed score (0-10)
fn score_speed(response_time_ms: u128) -> f64 {
    match response_time_ms {
        0..=1000 => 10.0,
        1001..=2000 => 9.0,
        2001..=3000 => 8.0,
        3001..=5000 => 7.0,
        5001..=8000 => 5.0,
        8001..=15000 => 3.0,
        15001..=30000 => 1.0,
        _ => 0.0,
    }
}

/// Compute reliability score (0-10) for a single run
fn score_reliability(success: bool) -> f64 {
    if success {
        10.0
    } else {
        0.0
    }
}

/// Read the current cycle number from scores.json if it exists
fn read_current_cycle() -> u32 {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let scores_path = manifest_dir
        .parent()
        .unwrap()
        .join("docs/loop/scores.json");

    if let Ok(content) = fs::read_to_string(&scores_path) {
        if let Ok(scores) = serde_json::from_str::<ScoresOutput>(&content) {
            return scores.cycle + 1;
        }
    }
    1 // First cycle
}

#[tokio::test]
#[ignore]
async fn quality_loop_run() {
    let corpus = load_corpus();
    let app = create_app();
    let cycle = read_current_cycle();

    println!("\n{}", "=".repeat(80));
    println!("ESSENCE QUALITY LOOP - CYCLE {}", cycle);
    println!("{}", "=".repeat(80));
    println!("Test corpus: {} URLs", corpus.len());
    println!("{}", "=".repeat(80));

    let mut url_scores: Vec<UrlScore> = Vec::new();

    for (i, test_url) in corpus.iter().enumerate() {
        println!(
            "\n[{}/{}] [{}] {}",
            i + 1,
            corpus.len(),
            test_url.category,
            test_url.url
        );

        let payload = json!({
            "url": test_url.url,
            "formats": ["markdown", "html", "links", "images"],
            "timeout": 30000
        });

        let start = Instant::now();
        let response = send_scrape_request(&app, payload).await;
        let elapsed = start.elapsed();

        let success = response["success"].as_bool().unwrap_or(false);
        let markdown = response["data"]["markdown"].as_str().unwrap_or("");
        let metrics = ScrapeMetrics::from_response(test_url.url.clone(), &response, elapsed);

        let mq = score_markdown_quality(&metrics, markdown);
        let comp = score_completeness(&metrics, test_url.expected_min_words);
        let spd = score_speed(elapsed.as_millis());
        let rel = score_reliability(success);
        let overall = (mq + comp + spd + rel) / 4.0;

        let error = if !success {
            Some(response["error"].as_str().unwrap_or("Unknown").to_string())
        } else {
            None
        };

        println!(
            "  {} | {}ms | {} words | Q:{:.1} C:{:.1} S:{:.1} R:{:.1} = {:.1}",
            if success { "OK" } else { "FAIL" },
            elapsed.as_millis(),
            metrics.word_count,
            mq,
            comp,
            spd,
            rel,
            overall
        );

        if let Some(ref err) = error {
            println!("  ERROR: {}", err);
        }

        url_scores.push(UrlScore {
            url: test_url.url.clone(),
            category: test_url.category.clone(),
            description: test_url.description.clone(),
            success,
            error,
            word_count: metrics.word_count,
            markdown_length: metrics.markdown_length,
            html_length: metrics.html_length,
            response_time_ms: elapsed.as_millis(),
            has_title: metrics.has_title,
            has_description: metrics.has_description,
            link_count: metrics.link_count,
            image_count: metrics.image_count,
            content_hash: metrics.content_hash.clone(),
            markdown_quality: mq,
            completeness: comp,
            speed: spd,
            reliability: rel,
            overall,
        });

        // 500ms throttle between requests
        sleep(Duration::from_millis(500)).await;
    }

    // Compute category aggregates
    let mut categories: HashMap<String, CategoryScore> = HashMap::new();
    let mut cat_groups: HashMap<String, Vec<&UrlScore>> = HashMap::new();

    for score in &url_scores {
        cat_groups
            .entry(score.category.clone())
            .or_default()
            .push(score);
    }

    for (cat, scores) in &cat_groups {
        let count = scores.len();
        let success_count = scores.iter().filter(|s| s.success).count();
        categories.insert(
            cat.clone(),
            CategoryScore {
                url_count: count,
                success_count,
                success_rate: success_count as f64 / count as f64,
                avg_markdown_quality: scores.iter().map(|s| s.markdown_quality).sum::<f64>()
                    / count as f64,
                avg_completeness: scores.iter().map(|s| s.completeness).sum::<f64>()
                    / count as f64,
                avg_speed: scores.iter().map(|s| s.speed).sum::<f64>() / count as f64,
                avg_reliability: scores.iter().map(|s| s.reliability).sum::<f64>() / count as f64,
                avg_overall: scores.iter().map(|s| s.overall).sum::<f64>() / count as f64,
            },
        );
    }

    let total_success = url_scores.iter().filter(|s| s.success).count();
    let overall_score =
        url_scores.iter().map(|s| s.overall).sum::<f64>() / url_scores.len() as f64;

    // Print summary
    println!("\n{}", "=".repeat(80));
    println!("CYCLE {} RESULTS", cycle);
    println!("{}", "=".repeat(80));
    println!(
        "Overall: {:.1}/10.0 | Success: {}/{}",
        overall_score,
        total_success,
        url_scores.len()
    );
    println!();

    let mut cats: Vec<_> = categories.iter().collect();
    cats.sort_by(|a, b| a.1.avg_overall.partial_cmp(&b.1.avg_overall).unwrap());

    for (cat, score) in &cats {
        println!(
            "  {:12} | Q:{:.1} C:{:.1} S:{:.1} R:{:.1} = {:.1} | {}/{} success",
            cat,
            score.avg_markdown_quality,
            score.avg_completeness,
            score.avg_speed,
            score.avg_reliability,
            score.avg_overall,
            score.success_count,
            score.url_count
        );
    }

    // Print failures
    let failures: Vec<&UrlScore> = url_scores.iter().filter(|s| !s.success).collect();
    if !failures.is_empty() {
        println!("\nFailed URLs:");
        for f in &failures {
            println!(
                "  [{}] {} - {}",
                f.category,
                f.url,
                f.error.as_deref().unwrap_or("unknown")
            );
        }
    }

    // Print lowest-scoring successful URLs
    let mut successful: Vec<&UrlScore> = url_scores.iter().filter(|s| s.success).collect();
    successful.sort_by(|a, b| a.overall.partial_cmp(&b.overall).unwrap());
    println!("\nLowest scoring (successful) URLs:");
    for s in successful.iter().take(5) {
        println!(
            "  {:.1} | [{}] {} - Q:{:.1} C:{:.1} S:{:.1}",
            s.overall, s.category, s.url, s.markdown_quality, s.completeness, s.speed
        );
    }

    // Write scores.json
    let output = ScoresOutput {
        timestamp: chrono::Utc::now().to_rfc3339(),
        cycle,
        total_urls: url_scores.len(),
        total_success,
        overall_score,
        categories,
        urls: url_scores,
    };

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let scores_path = manifest_dir
        .parent()
        .unwrap()
        .join("docs/loop/scores.json");
    let json = serde_json::to_string_pretty(&output).unwrap();
    fs::write(&scores_path, &json).expect("Failed to write scores.json");
    println!("\nScores written to: {:?}", scores_path);

    // Write failures.json
    let failures_data: Vec<serde_json::Value> = output
        .urls
        .iter()
        .filter(|u| !u.success || u.overall < 5.0)
        .map(|u| {
            json!({
                "url": u.url,
                "category": u.category,
                "success": u.success,
                "error": u.error,
                "overall_score": u.overall,
                "markdown_quality": u.markdown_quality,
                "completeness": u.completeness,
                "speed": u.speed,
                "word_count": u.word_count,
                "diagnosis": "needs_investigation"
            })
        })
        .collect();

    let failures_path = manifest_dir
        .parent()
        .unwrap()
        .join("docs/loop/failures.json");
    let failures_json = serde_json::to_string_pretty(&failures_data).unwrap();
    fs::write(&failures_path, &failures_json).expect("Failed to write failures.json");
    println!("Failures written to: {:?}", failures_path);

    println!("\n{}", "=".repeat(80));
}
