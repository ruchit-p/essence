# CLAUDE.md

Guidance for Claude Code when working with this repository.

## Project Overview

Essence is a web retrieval engine built in Rust with an HTTP-to-Browser fallback strategy. It provides LLM-ready Markdown output through automatic rendering escalation: lightweight HTTP fetching for most pages, full Chromium browser automation for JavaScript-heavy SPAs.

Four REST endpoints plus an MCP server for AI agent integration:

- **POST /api/v1/scrape** -- Single-page capture with automatic engine selection
- **POST /api/v1/crawl** -- Multi-page traversal with frontier, dedup, robots.txt, rate limits
- **POST /api/v1/map** -- URL discovery via sitemaps and in-page links
- **POST /api/v1/search** -- DuckDuckGo HTML search with optional result scraping
- **GET /mcp** -- MCP (Model Context Protocol) server for AI agent tool use

## Common Commands

```bash
cd backend

# Build
cargo build
cargo build --release

# Run (serves on port 8080)
cargo run --release

# Run unit tests (no network required)
cargo test --lib

# Compile all tests without running
cargo test --no-run

# Run integration tests (requires network)
cargo test --test integration -- --ignored

# Run browser engine tests (requires Chromium)
cargo test --test browser_engine_tests

# Environment setup
cp .env.example .env
```

## Architecture

### Two-Tier Rendering Strategy

1. **HTTP Engine** (fast path, majority of requests) -- lightweight HTTP fetch via reqwest. Used when HTML contains sufficient content density.
2. **Browser Engine** (fallback) -- full Chromium automation via chromiumoxide/CDP. Used for SPAs, anti-bot pages, JavaScript-rendered content.

Auto-detection based on content density analysis, hydration markers, meta-refresh detection, and anti-fetch headers.

### Module Structure

```
backend/src/
  api/           # Endpoint handlers (scrape, crawl, map, search)
  engines/       # HTTP and Browser rendering engines
  format/        # Markdown and metadata output formatters
  crawler/       # URL frontier, robots.txt, rate limiting, sitemaps, pagination
  search/        # DuckDuckGo HTML search integration
  cache/         # In-memory caching
  rate_limit/    # Rate limiting
  utils/         # DNS cache, URL normalization, robots.txt
  mcp.rs         # MCP server (exposes scrape/crawl/map/search as tools)
  types.rs       # Request/response types
  error.rs       # Error types
  main.rs        # Server startup with graceful shutdown
  lib.rs         # Library entry point
```

### API Parameters

Common request fields across endpoints:

- `engine`: `"auto"` | `"http"` | `"browser"` (default: `"auto"`)
- `formats`: `["markdown", "html", "links", "metadata"]`
- `timeout_ms`: request timeout in milliseconds
- `headers`: custom HTTP headers

## Key Technologies

- **Web framework**: axum + tower-http (compression, CORS, tracing)
- **Async runtime**: tokio
- **HTTP client**: reqwest (rustls-tls, gzip, brotli, cookies)
- **HTML parsing**: scraper, html5ever
- **Browser automation**: chromiumoxide (CDP)
- **Markdown conversion**: html2md
- **Content extraction**: readability
- **MCP server**: rmcp (streamable HTTP transport)
- **Rate limiting**: governor
- **Caching**: moka (in-memory)
- **DNS caching**: hickory-resolver + lru

## Testing

Test files in `backend/tests/`:

- `integration.rs` -- real-world scrape tests (marked `#[ignore]`, needs network)
- `scrape.rs` -- scrape endpoint unit tests with mock server
- `engine_parity.rs` -- engine comparison and fallback validation
- `browser_engine_tests.rs` -- browser-specific functionality
- `benchmark.rs` -- performance benchmarks
- `streaming_crawl_tests.rs` -- streaming crawl tests
- `crawler_bounds_tests.rs` -- crawler config and circuit breaker tests
- `memory_bounds_integration.rs` -- memory monitoring tests
- `test_markdown_cleaning.rs` -- markdown output quality tests

## Environment Variables

See `backend/.env.example` for the full list. Key variables:

| Variable | Default | Description |
|---|---|---|
| `PORT` | `8080` | Server port |
| `RUST_LOG` | `essence=info` | Log level |
| `BROWSER_HEADLESS` | `true` | Run Chromium headless |
| `BROWSER_POOL_SIZE` | `5` | Browser instance pool |
| `BROWSER_TIMEOUT_MS` | `30000` | Browser page timeout |
| `MAX_PARALLEL_SCRAPES` | `5` | Parallel scrapes for /search |
| `MAX_REQUEST_SIZE_MB` | `1` | Max request body size |

## Error Handling

Use types from `backend/src/error.rs`:

- `ScrapeError` for scraping failures
- Return `Result<T, ScrapeError>` from fallible operations

## Docker

```bash
docker-compose up -d    # Build and run on port 8080
docker-compose down     # Stop
```

The `docker-compose.yml` runs the Essence service only. No external dependencies (no Redis, no PostgreSQL).
