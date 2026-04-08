// Test modules for integration tests
pub mod caching;
pub mod circuit_breaker;
pub mod scraping;

#[cfg(feature = "database")]
pub mod database;
