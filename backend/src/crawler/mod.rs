#![allow(clippy::module_inception)]

pub mod config;
pub mod crawler;
pub mod filter;
pub mod mapper;
pub mod pagination;
pub mod parallel;
pub mod prioritizer;
pub mod rate_limiter;
pub mod sitemap;
pub mod streaming;
pub mod url_normalization;

pub use config::{CircuitBreaker, CrawlerConfig, MemoryMonitor};
pub use crawler::crawl_website;
pub use filter::matches_pattern;
pub use mapper::discover_urls;
pub use pagination::PaginationDetector;
pub use parallel::ParallelCrawler;
pub use prioritizer::{PrioritizedUrl, UrlPrioritizer};
pub use rate_limiter::DomainRateLimiter;
pub use sitemap::SitemapParser;
pub use streaming::crawl_website_stream;
pub use url_normalization::{generate_url_permutations, normalize_url};
