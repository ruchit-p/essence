#[derive(Debug, Clone)]
pub struct Settings {
    pub server: ServerSettings,
    pub browser: BrowserSettings,
    pub engine: EngineSettings,
    pub crawler: CrawlerSettings,
    pub retry: RetrySettings,
}

#[derive(Debug, Clone)]
pub struct ServerSettings {
    pub port: u16,
    pub host: String,
    pub max_request_size_mb: usize,
    pub log_level: String,
}

#[derive(Debug, Clone)]
pub struct BrowserSettings {
    pub headless: bool,
    pub pool_size: usize,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone)]
pub struct EngineSettings {
    pub waterfall_enabled: bool,
    pub waterfall_delay_ms: u64,
    /// Shorter waterfall delay for batch operations (crawl, llmstxt).
    /// Default: 1000ms. Keeps batch scraping fast while still falling back to browser.
    pub batch_waterfall_delay_ms: u64,
    pub auto_fallback_on_block: bool,
    /// Visible text character threshold above which HTTP results are accepted
    /// regardless of framework detection. Default: 1000 chars (~150 words).
    pub content_sufficient_chars: usize,
}

#[derive(Debug, Clone)]
pub struct CrawlerSettings {
    pub max_concurrent_requests: usize,
    pub max_duration_sec: u64,
    pub rate_limit_per_sec: usize,
}

#[derive(Debug, Clone)]
pub struct RetrySettings {
    pub max_attempts: usize,
    pub initial_delay_ms: u64,
    pub max_delay_ms: u64,
}

fn env_or<T: std::str::FromStr>(key: &str, default: T) -> T {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

impl Settings {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            server: ServerSettings {
                port: env_or("PORT", 8080),
                host: std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
                max_request_size_mb: env_or("MAX_REQUEST_SIZE_MB", 1),
                log_level: std::env::var("RUST_LOG")
                    .unwrap_or_else(|_| "essence=info".to_string()),
            },
            browser: BrowserSettings {
                headless: env_or("BROWSER_HEADLESS", true),
                pool_size: env_or("BROWSER_POOL_SIZE", 5),
                timeout_ms: env_or("BROWSER_TIMEOUT_MS", 30000),
            },
            engine: EngineSettings {
                waterfall_enabled: env_or("ENGINE_WATERFALL_ENABLED", true),
                waterfall_delay_ms: env_or("ENGINE_WATERFALL_DELAY_MS", 1500),
                batch_waterfall_delay_ms: env_or("ENGINE_BATCH_WATERFALL_DELAY_MS", 1000),
                auto_fallback_on_block: env_or("ESSENCE_ENGINE_AUTO_FALLBACK_ON_BLOCK", true),
                content_sufficient_chars: env_or("CONTENT_SUFFICIENT_CHARS", 1000),
            },
            crawler: CrawlerSettings {
                max_concurrent_requests: env_or("MAX_CONCURRENT_REQUESTS", 10),
                max_duration_sec: env_or("CRAWL_MAX_DURATION_SEC", 300),
                rate_limit_per_sec: env_or("CRAWL_RATE_LIMIT_PER_SEC", 2),
            },
            retry: RetrySettings {
                max_attempts: env_or("RETRY_MAX_ATTEMPTS", 3),
                initial_delay_ms: env_or("RETRY_INITIAL_DELAY_MS", 500),
                max_delay_ms: env_or("RETRY_MAX_DELAY_MS", 30000),
            },
        })
    }
}
