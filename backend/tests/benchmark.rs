// Enhanced benchmarking suite for scraping operations
// Supports URL loading, repeats, throttling, engine tagging, and drift detection
// Run with: cargo test --test benchmark -- --ignored --nocapture
//
// Environment variables:
// - ESSENCE_BENCH_REPEATS: Number of times to repeat each URL (default: 1)
// - ESSENCE_BENCH_THROTTLE_MS: Delay between requests in milliseconds (default: 0)
// - ESSENCE_ENGINE_TAG: Tag to identify the engine/version (e.g., "v1.0", "chromium")

mod api;

use api::{
    create_app,
    metrics::{MetricsCollection, ScrapeMetrics},
    send_scrape_request,
};
use serde_json::json;
use std::env;
use std::fs;
use std::time::{Duration, Instant};
use tokio::time::sleep;

/// Test URL entry from the corpus
#[derive(Debug, Clone)]
struct TestUrl {
    category: String,
    url: String,
    description: String,
}

/// Load URLs from the test corpus file
fn load_test_urls() -> Vec<TestUrl> {
    let corpus_path = "/Volumes/Flashdrive/essence/docs/research/test_urls.txt";

    let content = match fs::read_to_string(corpus_path) {
        Ok(content) => content,
        Err(e) => {
            eprintln!("⚠️  Failed to read test_urls.txt: {}", e);
            eprintln!("   Using fallback URLs instead");
            return get_fallback_urls();
        }
    };

    let mut urls = Vec::new();

    for line in content.lines() {
        let line = line.trim();

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Parse pipe-separated format: category | url | description
        let parts: Vec<&str> = line.split('|').map(|s| s.trim()).collect();

        if parts.len() >= 3 {
            urls.push(TestUrl {
                category: parts[0].to_string(),
                url: parts[1].to_string(),
                description: parts[2].to_string(),
            });
        }
    }

    if urls.is_empty() {
        eprintln!("⚠️  No URLs parsed from test_urls.txt");
        eprintln!("   Using fallback URLs instead");
        return get_fallback_urls();
    }

    println!("✓ Loaded {} URLs from test corpus", urls.len());
    urls
}

/// Fallback URLs if corpus file cannot be loaded
fn get_fallback_urls() -> Vec<TestUrl> {
    vec![
        TestUrl {
            category: "static_docs".to_string(),
            url: "https://example.com".to_string(),
            description: "Example.com - minimal static page".to_string(),
        },
        TestUrl {
            category: "http".to_string(),
            url: "https://httpbin.org/html".to_string(),
            description: "httpbin - basic HTML response".to_string(),
        },
    ]
}

/// Get environment configuration
fn get_bench_config() -> BenchConfig {
    let repeats = env::var("ESSENCE_BENCH_REPEATS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);

    let throttle_ms = env::var("ESSENCE_BENCH_THROTTLE_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    let engine_tag = env::var("ESSENCE_ENGINE_TAG").ok();

    BenchConfig {
        repeats,
        throttle_ms,
        engine_tag,
    }
}

#[derive(Debug, Clone)]
struct BenchConfig {
    repeats: usize,
    throttle_ms: u64,
    engine_tag: Option<String>,
}

/// Run a single scrape benchmark and collect metrics
async fn benchmark_scrape(test_url: &TestUrl, config: &BenchConfig) -> ScrapeMetrics {
    let app = create_app();

    let payload = json!({
        "url": test_url.url,
        "formats": ["markdown", "html", "links", "images"]
    });

    let start = Instant::now();
    let response = send_scrape_request(&app, payload).await;
    let elapsed = start.elapsed();

    // Create metrics from response or error
    let mut metrics = if response["success"].as_bool().unwrap_or(false) {
        ScrapeMetrics::from_response(test_url.url.clone(), &response, elapsed)
    } else {
        let error = response["error"]
            .as_str()
            .unwrap_or("Unknown error")
            .to_string();
        ScrapeMetrics::from_error(test_url.url.clone(), error, elapsed)
    };

    metrics.category = Some(test_url.category.clone());
    metrics.test_name = Some(test_url.description.clone());

    if let Some(ref tag) = config.engine_tag {
        metrics.engine_tag = Some(tag.clone());
    }

    metrics
}

#[tokio::test]
#[ignore]
async fn benchmark_all_urls() {
    let config = get_bench_config();
    let test_urls = load_test_urls();

    println!("\n{}", "=".repeat(80));
    println!("COMPREHENSIVE SCRAPE BENCHMARK");
    println!("{}", "=".repeat(80));
    println!("Total URLs:       {}", test_urls.len());
    println!("Repeats per URL:  {}", config.repeats);
    println!("Throttle:         {}ms", config.throttle_ms);
    if let Some(ref tag) = config.engine_tag {
        println!("Engine tag:       {}", tag);
    }
    println!("{}", "=".repeat(80));
    println!();

    let mut all_metrics = Vec::new();
    let total_runs = test_urls.len() * config.repeats;
    let mut run_count = 0;

    for test_url in &test_urls {
        for repeat in 1..=config.repeats {
            run_count += 1;

            let repeat_label = if config.repeats > 1 {
                format!(" (repeat {}/{})", repeat, config.repeats)
            } else {
                String::new()
            };

            println!(
                "[{}/{}] {} | {}{}",
                run_count, total_runs, test_url.category, test_url.url, repeat_label
            );

            let metrics = benchmark_scrape(test_url, &config).await;

            println!(
                "  ✓ {} | {}ms | {} chars | {} words | hash: {}",
                if metrics.success { "SUCCESS" } else { "FAILED" },
                metrics.response_time_ms,
                metrics.markdown_length,
                metrics.word_count,
                metrics
                    .content_hash
                    .as_ref()
                    .map(|h| &h[..8])
                    .unwrap_or("none")
            );

            if let Some(error) = &metrics.error {
                println!("  ✗ Error: {}", error);
            }

            all_metrics.push(metrics);

            // Apply throttling if configured
            if config.throttle_ms > 0 && run_count < total_runs {
                sleep(Duration::from_millis(config.throttle_ms)).await;
            }

            println!();
        }
    }

    // Create metrics collection
    let collection = MetricsCollection::new(all_metrics);

    // Print summary
    collection.print_summary();

    // Save results
    let timestamp = chrono::Utc::now().format("%Y-%m-%d_%H-%M-%S");
    let tag_suffix = config
        .engine_tag
        .as_ref()
        .map(|t| format!("_{}", t))
        .unwrap_or_default();

    let csv_path = format!(
        "/Volumes/Flashdrive/essence/bench/results/benchmark_all{}_{}_{}.csv",
        tag_suffix, config.repeats, timestamp
    );
    let json_path = format!(
        "/Volumes/Flashdrive/essence/bench/results/benchmark_all{}_{}_{}.json",
        tag_suffix, config.repeats, timestamp
    );

    match collection.save_csv(&csv_path) {
        Ok(_) => println!("✓ CSV saved: {}", csv_path),
        Err(e) => eprintln!("✗ Failed to save CSV: {}", e),
    }

    match collection.save_json(&json_path) {
        Ok(_) => println!("✓ JSON saved: {}", json_path),
        Err(e) => eprintln!("✗ Failed to save JSON: {}", e),
    }

    println!();
}

#[tokio::test]
#[ignore]
async fn benchmark_by_category() {
    let config = get_bench_config();
    let test_urls = load_test_urls();

    // Get category from environment or default to first category
    let target_category = env::var("ESSENCE_BENCH_CATEGORY").unwrap_or_else(|_| {
        test_urls
            .first()
            .map(|u| u.category.clone())
            .unwrap_or_default()
    });

    let category_urls: Vec<&TestUrl> = test_urls
        .iter()
        .filter(|u| u.category == target_category)
        .collect();

    if category_urls.is_empty() {
        eprintln!("✗ No URLs found for category: {}", target_category);
        return;
    }

    println!("\n{}", "=".repeat(80));
    println!("CATEGORY BENCHMARK - {}", target_category.to_uppercase());
    println!("{}", "=".repeat(80));
    println!("URLs in category: {}", category_urls.len());
    println!("Repeats per URL:  {}", config.repeats);
    println!("{}", "=".repeat(80));
    println!();

    let mut all_metrics = Vec::new();

    for test_url in &category_urls {
        for repeat in 1..=config.repeats {
            println!(
                "Testing: {} (repeat {}/{})",
                test_url.url, repeat, config.repeats
            );

            let metrics = benchmark_scrape(test_url, &config).await;

            println!(
                "  ✓ {}ms | {} words | ratio: {:.2}%",
                metrics.response_time_ms,
                metrics.word_count,
                metrics.extraction_ratio.unwrap_or(0.0) * 100.0
            );

            all_metrics.push(metrics);

            if config.throttle_ms > 0 {
                sleep(Duration::from_millis(config.throttle_ms)).await;
            }
        }
    }

    let collection = MetricsCollection::new(all_metrics);
    collection.print_summary();

    // Save category-specific results
    let timestamp = chrono::Utc::now().format("%Y-%m-%d_%H-%M-%S");
    let csv_path = format!(
        "/Volumes/Flashdrive/essence/bench/results/category_{}_{}.csv",
        target_category, timestamp
    );

    if collection.save_csv(&csv_path).is_ok() {
        println!("✓ Category results saved: {}", csv_path);
    }

    println!();
}

#[tokio::test]
#[ignore]
async fn benchmark_drift_detection() {
    let config = BenchConfig {
        repeats: 5,       // Run each URL 5 times to detect drift
        throttle_ms: 500, // 500ms between requests
        engine_tag: env::var("ESSENCE_ENGINE_TAG").ok(),
    };

    let drift_urls = vec![
        TestUrl {
            category: "drift_test".to_string(),
            url: "https://example.com".to_string(),
            description: "Example.com - stable content expected".to_string(),
        },
        TestUrl {
            category: "drift_test".to_string(),
            url: "https://httpbin.org/html".to_string(),
            description: "httpbin.org - stable HTML response".to_string(),
        },
    ];

    println!("\n{}", "=".repeat(80));
    println!("CONTENT DRIFT DETECTION TEST");
    println!("{}", "=".repeat(80));
    println!(
        "Running {} URLs x {} repeats = {} total runs",
        drift_urls.len(),
        config.repeats,
        drift_urls.len() * config.repeats
    );
    println!("{}", "=".repeat(80));
    println!();

    let mut all_metrics = Vec::new();

    for test_url in &drift_urls {
        println!("Testing drift for: {}", test_url.url);

        for repeat in 1..=config.repeats {
            print!("  Run {}/{}... ", repeat, config.repeats);
            std::io::Write::flush(&mut std::io::stdout()).ok();

            let metrics = benchmark_scrape(test_url, &config).await;

            println!(
                "{}ms | hash: {}",
                metrics.response_time_ms,
                metrics
                    .content_hash
                    .as_ref()
                    .map(|h| &h[..16])
                    .unwrap_or("none")
            );

            all_metrics.push(metrics);

            if repeat < config.repeats {
                sleep(Duration::from_millis(config.throttle_ms)).await;
            }
        }
        println!();
    }

    let collection = MetricsCollection::new(all_metrics);
    collection.print_summary();

    // Save drift detection results
    let timestamp = chrono::Utc::now().format("%Y-%m-%d_%H-%M-%S");
    let csv_path = format!(
        "/Volumes/Flashdrive/essence/bench/results/drift_detection_{}.csv",
        timestamp
    );

    if collection.save_csv(&csv_path).is_ok() {
        println!("✓ Drift detection results saved: {}", csv_path);
    }

    println!();
}

#[tokio::test]
#[ignore]
async fn benchmark_quick_check() {
    println!("\n{}", "=".repeat(80));
    println!("QUICK BENCHMARK - SMOKE TEST");
    println!("{}", "=".repeat(80));

    let config = get_bench_config();
    let quick_urls = [TestUrl {
            category: "static_docs".to_string(),
            url: "https://example.com".to_string(),
            description: "Example.com - quick check".to_string(),
        },
        TestUrl {
            category: "http".to_string(),
            url: "https://httpbin.org/html".to_string(),
            description: "httpbin.org - HTML response".to_string(),
        }];

    let mut all_metrics = Vec::new();

    for (i, test_url) in quick_urls.iter().enumerate() {
        println!("[{}/{}] {}", i + 1, quick_urls.len(), test_url.url);

        let metrics = benchmark_scrape(test_url, &config).await;

        println!(
            "  ✓ {} | {}ms",
            if metrics.success { "SUCCESS" } else { "FAILED" },
            metrics.response_time_ms
        );

        all_metrics.push(metrics);
    }

    let collection = MetricsCollection::new(all_metrics);
    collection.print_summary();

    println!();
}

#[tokio::test]
#[ignore]
async fn benchmark_performance_percentiles() {
    println!("\n{}", "=".repeat(80));
    println!("PERFORMANCE PERCENTILE ANALYSIS");
    println!("{}", "=".repeat(80));

    let config = BenchConfig {
        repeats: 20, // Run 20 times for statistical significance
        throttle_ms: 100,
        engine_tag: env::var("ESSENCE_ENGINE_TAG").ok(),
    };

    let test_url = TestUrl {
        category: "performance".to_string(),
        url: "https://example.com".to_string(),
        description: "Performance analysis".to_string(),
    };

    println!(
        "Running {} iterations of: {}\n",
        config.repeats, test_url.url
    );

    let mut all_metrics = Vec::new();

    for i in 1..=config.repeats {
        print!("[{}/{}] ", i, config.repeats);
        std::io::Write::flush(&mut std::io::stdout()).ok();

        let metrics = benchmark_scrape(&test_url, &config).await;

        println!("{}ms", metrics.response_time_ms);

        all_metrics.push(metrics);

        if i < config.repeats {
            sleep(Duration::from_millis(config.throttle_ms)).await;
        }
    }

    let collection = MetricsCollection::new(all_metrics);
    collection.print_summary();

    // Additional percentile analysis
    println!("\nDetailed Percentile Analysis:");
    println!(
        "  p50 (median):   {}ms",
        collection.summary.p50_response_time_ms
    );
    println!(
        "  p90:            {}ms",
        collection.summary.p90_response_time_ms
    );
    println!(
        "  p99:            {}ms",
        collection.summary.p99_response_time_ms
    );

    // Calculate variance and standard deviation
    let response_times: Vec<f64> = collection
        .metrics
        .iter()
        .map(|m| m.response_time_ms as f64)
        .collect();

    let mean = collection.summary.avg_response_time_ms;
    let variance = response_times
        .iter()
        .map(|&t| {
            let diff = t - mean;
            diff * diff
        })
        .sum::<f64>()
        / response_times.len() as f64;

    let std_dev = variance.sqrt();

    println!("\nStatistical Measures:");
    println!("  Mean:           {:.2}ms", mean);
    println!("  Std deviation:  {:.2}ms", std_dev);
    println!("  Variance:       {:.2}", variance);
    println!("  Coef of var:    {:.2}%", (std_dev / mean) * 100.0);

    println!();
}
