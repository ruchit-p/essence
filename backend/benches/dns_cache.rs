//! Benchmark for DNS caching performance
//!
//! This benchmark measures the latency improvement from DNS caching.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use essence::utils::dns_cache::DnsCache;
use tokio::runtime::Runtime;

fn bench_dns_lookup(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let cache = rt.block_on(async { DnsCache::new().unwrap() });

    let domains = vec![
        "google.com",
        "github.com",
        "rust-lang.org",
        "crates.io",
        "docs.rs",
    ];

    let mut group = c.benchmark_group("dns_lookup");

    for domain in &domains {
        // First lookup - cache miss
        group.bench_with_input(
            BenchmarkId::new("cache_miss", domain),
            domain,
            |b, &domain| {
                b.to_async(&rt).iter(|| async {
                    cache.clear().await;
                    let result = cache.lookup(black_box(domain)).await;
                    black_box(result)
                });
            },
        );

        // Second lookup - cache hit
        group.bench_with_input(
            BenchmarkId::new("cache_hit", domain),
            domain,
            |b, &domain| {
                b.to_async(&rt).iter(|| async {
                    // Prime the cache
                    let _ = cache.lookup(domain).await;
                    // Now measure cached lookup
                    let result = cache.lookup(black_box(domain)).await;
                    black_box(result)
                });
            },
        );
    }

    group.finish();
}

fn bench_cache_hit_rate(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("mixed_lookup_pattern", |b| {
        b.to_async(&rt).iter(|| async {
            let cache = DnsCache::new().unwrap();

            // Simulate real-world pattern: some repeated, some new domains
            let _ = cache.lookup("google.com").await;
            let _ = cache.lookup("github.com").await;
            let _ = cache.lookup("google.com").await; // hit
            let _ = cache.lookup("rust-lang.org").await;
            let _ = cache.lookup("github.com").await; // hit
            let _ = cache.lookup("google.com").await; // hit

            let stats = cache.stats().await;
            black_box(stats)
        });
    });
}

criterion_group!(benches, bench_dns_lookup, bench_cache_hit_rate);
criterion_main!(benches);
