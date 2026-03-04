use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use std::collections::{BinaryHeap, HashMap, HashSet};

/// Simulate URL prioritization score calculation
fn calculate_priority_score(url: &str, depth: usize, is_sitemap: bool) -> i64 {
    let mut score: i64 = 1000;

    // Penalize depth
    score -= (depth as i64) * 100;

    // Boost sitemap URLs
    if is_sitemap {
        score += 500;
    }

    // Boost based on URL patterns
    if url.contains("/docs/") || url.contains("/documentation/") {
        score += 200;
    }
    if url.contains("/api/") {
        score += 150;
    }
    if url.contains("/blog/") {
        score += 100;
    }

    // Penalize query parameters
    if url.contains('?') {
        score -= 50;
    }

    // Penalize fragments
    if url.contains('#') {
        score -= 25;
    }

    score
}

#[derive(Debug, Eq, PartialEq)]
struct PrioritizedUrl {
    url: String,
    priority: i64,
    depth: usize,
}

impl Ord for PrioritizedUrl {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.priority.cmp(&other.priority)
    }
}

impl PartialOrd for PrioritizedUrl {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Generate test URLs
fn generate_urls(count: usize) -> Vec<String> {
    let mut urls = Vec::new();
    for i in 0..count {
        urls.push(format!("https://example.com/page-{}", i));
        if i % 5 == 0 {
            urls.push(format!("https://example.com/docs/page-{}", i));
        }
        if i % 10 == 0 {
            urls.push(format!("https://example.com/api/endpoint-{}", i));
        }
        if i % 7 == 0 {
            urls.push(format!("https://example.com/blog/post-{}", i));
        }
    }
    urls
}

fn bench_url_deduplication(c: &mut Criterion) {
    let mut group = c.benchmark_group("url_deduplication");

    for count in [100, 1000, 10000].iter() {
        let urls = generate_urls(*count);
        let duplicate_urls: Vec<_> = urls.iter().cycle().take(count * 2).cloned().collect();

        group.bench_with_input(BenchmarkId::from_parameter(count), count, |b, _| {
            b.iter(|| {
                let mut seen = HashSet::new();
                for url in black_box(&duplicate_urls) {
                    seen.insert(url);
                }
                seen
            })
        });
    }

    group.finish();
}

fn bench_priority_queue_insertion(c: &mut Criterion) {
    let mut group = c.benchmark_group("priority_queue_insertion");

    for count in [100, 1000, 5000].iter() {
        let urls = generate_urls(*count);

        group.bench_with_input(BenchmarkId::from_parameter(count), count, |b, _| {
            b.iter(|| {
                let mut queue = BinaryHeap::new();
                for (i, url) in black_box(&urls).iter().enumerate() {
                    let priority = calculate_priority_score(url, i % 5, i % 20 == 0);
                    queue.push(PrioritizedUrl {
                        url: url.clone(),
                        priority,
                        depth: i % 5,
                    });
                }
                queue
            })
        });
    }

    group.finish();
}

fn bench_priority_calculation(c: &mut Criterion) {
    let mut group = c.benchmark_group("priority_calculation");

    let test_urls = vec![
        "https://example.com/page-1",
        "https://example.com/docs/guide",
        "https://example.com/api/v1/endpoint",
        "https://example.com/blog/post?id=123",
        "https://example.com/page#section",
    ];

    for depth in [0, 1, 2, 5].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(depth), depth, |b, d| {
            b.iter(|| {
                for url in black_box(&test_urls) {
                    calculate_priority_score(url, *d, false);
                }
            })
        });
    }

    group.finish();
}

fn bench_url_frontier_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("url_frontier_operations");

    for count in [100, 500, 1000].iter() {
        let urls = generate_urls(*count);

        group.bench_with_input(BenchmarkId::from_parameter(count), count, |b, _| {
            b.iter(|| {
                let mut frontier = BinaryHeap::new();
                let mut visited = HashSet::new();

                // Simulate frontier operations: add, dedupe, pop
                for (i, url) in black_box(&urls).iter().enumerate() {
                    if !visited.contains(url) {
                        visited.insert(url.clone());
                        let priority = calculate_priority_score(url, i % 5, i % 20 == 0);
                        frontier.push(PrioritizedUrl {
                            url: url.clone(),
                            priority,
                            depth: i % 5,
                        });
                    }
                }

                // Pop half the URLs
                for _ in 0..(count / 2) {
                    frontier.pop();
                }
            })
        });
    }

    group.finish();
}

fn bench_domain_tracking(c: &mut Criterion) {
    let mut group = c.benchmark_group("domain_tracking");

    for count in [100, 500, 1000].iter() {
        let urls = generate_urls(*count);

        group.bench_with_input(BenchmarkId::from_parameter(count), count, |b, _| {
            b.iter(|| {
                let mut domain_counts: HashMap<String, usize> = HashMap::new();

                for url in black_box(&urls) {
                    if let Ok(parsed) = url::Url::parse(url) {
                        if let Some(domain) = parsed.domain() {
                            *domain_counts.entry(domain.to_string()).or_insert(0) += 1;
                        }
                    }
                }

                domain_counts
            })
        });
    }

    group.finish();
}

fn bench_url_filtering(c: &mut Criterion) {
    let mut group = c.benchmark_group("url_filtering");

    let urls = generate_urls(1000);
    let patterns = vec!["/docs/", "/api/", "/blog/"];

    group.bench_function("filter_by_pattern", |b| {
        b.iter(|| {
            let filtered: Vec<_> = black_box(&urls)
                .iter()
                .filter(|url| patterns.iter().any(|p| url.contains(p)))
                .collect();
            filtered
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_url_deduplication,
    bench_priority_queue_insertion,
    bench_priority_calculation,
    bench_url_frontier_operations,
    bench_domain_tracking,
    bench_url_filtering
);

criterion_main!(benches);
