# Essence

[Website](https://essence.foundation) | [Docs](https://essence.foundation/docs) | [crates.io](https://crates.io/crates/essence-engine) | [GitHub](https://github.com/ruchit-p/essence)

A fast, open-source web retrieval engine built in Rust. Fetches pages via lightweight HTTP with automatic fallback to headless Chromium for JavaScript-heavy sites. Returns clean, LLM-ready Markdown.

**91% quality win rate** against Firecrawl across 35 real-world URLs, judged by LLM evaluation. **1.5x faster** on average.

## Install

```bash
cargo install essence-engine
```

## Why Essence?

- **Two-tier rendering** -- lightweight HTTP fetch for most pages, automatic Chromium fallback for SPAs and JS-heavy sites
- **LLM-optimized output** -- clean Markdown with noise removal, heading hierarchy, code preservation, and structured metadata
- **Structured extraction** -- pull typed JSON from pages using CSS selectors or LLM-based extraction with schema validation
- **6 REST endpoints + MCP server** -- scrape, crawl, map, search, extract, llms.txt -- usable by humans and AI agents alike
- **OpenAPI spec** -- auto-generated at `/api/docs/openapi.json` for SDK generation and tooling
- **Official SDKs** -- Python and TypeScript client libraries
- **Self-hosted & open-source** -- no API keys, no rate limits, no vendor lock-in
- **Respectful crawling** -- robots.txt compliance, per-domain rate limits, configurable politeness
- **Fast** -- median response under 1 second for HTTP-rendered pages

## Quick Start

### Prerequisites

- [Rust](https://rustup.rs/) (stable toolchain)
- [Chromium](https://www.chromium.org/) (optional -- only needed for browser engine fallback)

```bash
# macOS
brew install chromium

# Ubuntu/Debian
sudo apt install chromium-browser

# Or skip Chromium -- the HTTP engine handles most pages without a browser
```

### Build and Run

```bash
git clone https://github.com/ruchit-p/essence.git
cd essence/backend
cp .env.example .env
cargo build --release
cargo run --release
# Server starts at http://localhost:8080
```

### Docker

```bash
docker-compose up -d
# Service available at http://localhost:8080
```

### Try It

```bash
# Scrape a page to Markdown
curl -s -X POST http://localhost:8080/api/v1/scrape \
  -H "Content-Type: application/json" \
  -d '{"url": "https://example.com", "formats": ["markdown"]}' | jq .

# Discover all URLs on a site
curl -s -X POST http://localhost:8080/api/v1/map \
  -H "Content-Type: application/json" \
  -d '{"url": "https://example.com"}' | jq .

# Crawl a site (follow links, respect robots.txt)
curl -s -X POST http://localhost:8080/api/v1/crawl \
  -H "Content-Type: application/json" \
  -d '{"url": "https://example.com", "maxDepth": 2, "limit": 10}' | jq .

# Search the web
curl -s -X POST http://localhost:8080/api/v1/search \
  -H "Content-Type: application/json" \
  -d '{"query": "rust web scraping", "limit": 5}' | jq .

# Extract structured data
curl -s -X POST http://localhost:8080/api/v1/extract \
  -H "Content-Type: application/json" \
  -d '{"urls": ["https://example.com"], "mode": "css", "selectors": {"title": "h1", "links": "a"}, "schema": {"properties": {"title": {"type": "string"}, "links": {"type": "array"}}}}' | jq .
```

---

## API Reference

All endpoints accept JSON POST requests at `/api/v1/`.

### POST /api/v1/scrape

Single-page capture with automatic engine selection.

**Request:**

| Field | Type | Default | Description |
|---|---|---|---|
| `url` | string | *required* | URL to scrape |
| `formats` | string[] | `["markdown"]` | Output formats: `"markdown"`, `"html"`, `"links"`, `"metadata"` |
| `engine` | string | `"auto"` | Rendering engine: `"auto"`, `"http"`, or `"browser"` |
| `timeout` | int | `30000` | Timeout in milliseconds |
| `headers` | object | none | Custom HTTP headers |
| `onlyMainContent` | bool | `true` | Extract only the main content area |

**Response:**

```json
{
  "success": true,
  "data": {
    "markdown": "# Page Title\n\nPage content in clean Markdown...",
    "metadata": {
      "title": "Page Title",
      "description": "Page description",
      "language": "en",
      "url": "https://example.com",
      "statusCode": 200,
      "wordCount": 1234,
      "readingTime": 5
    },
    "links": ["https://example.com/about", "https://example.com/contact"],
    "images": ["https://example.com/hero.jpg"]
  }
}
```

### POST /api/v1/crawl

Multi-page traversal with dedup, robots.txt compliance, and rate limiting.

**Request:**

| Field | Type | Default | Description |
|---|---|---|---|
| `url` | string | *required* | Starting URL |
| `maxDepth` | int | `2` | Maximum link depth to follow |
| `limit` | int | `100` | Maximum pages to crawl |
| `includePaths` | string[] | none | Glob patterns to include (e.g. `["/blog/*"]`) |
| `excludePaths` | string[] | none | Glob patterns to exclude (e.g. `["/admin/*"]`) |
| `allowBackwardLinks` | bool | `false` | Follow links up the URL hierarchy |
| `allowExternalLinks` | bool | `false` | Follow links to other domains |
| `detectPagination` | bool | `true` | Automatically follow paginated content |

**Response:**

```json
{
  "success": true,
  "data": [
    {
      "markdown": "# Home\n\nWelcome...",
      "metadata": { "title": "Home", "url": "https://example.com", "statusCode": 200 }
    },
    {
      "markdown": "# About\n\nAbout us...",
      "metadata": { "title": "About", "url": "https://example.com/about", "statusCode": 200 }
    }
  ]
}
```

### POST /api/v1/map

URL discovery via sitemaps and in-page link extraction.

**Request:**

| Field | Type | Default | Description |
|---|---|---|---|
| `url` | string | *required* | Site URL to discover links from |
| `includeSubdomains` | bool | `true` | Include URLs from subdomains |
| `limit` | int | `5000` | Maximum URLs to return |
| `search` | string | none | Filter URLs by search query |
| `ignoreSitemap` | bool | `false` | Skip sitemap.xml discovery |

**Response:**

```json
{
  "success": true,
  "links": [
    "https://example.com/",
    "https://example.com/about",
    "https://example.com/blog",
    "https://example.com/blog/post-1"
  ]
}
```

### POST /api/v1/search

Web search via DuckDuckGo with optional scraping of result pages.

**Request:**

| Field | Type | Default | Description |
|---|---|---|---|
| `query` | string | *required* | Search query |
| `limit` | int | `10` | Number of search results |
| `scrapeResults` | bool | `false` | Scrape full content of each result |

**Response:**

```json
{
  "success": true,
  "data": [
    {
      "title": "Web Scraping in Rust",
      "url": "https://example.com/rust-scraping",
      "snippet": "A guide to web scraping with Rust..."
    }
  ]
}
```

### POST /api/v1/extract

Structured data extraction from web pages using CSS selectors, LLM-based extraction, or both.

**Request:**

| Field | Type | Default | Description |
|---|---|---|---|
| `urls` | string[] | *required* | URLs to extract from (1-10) |
| `schema` | object | none | JSON Schema for output structure |
| `selectors` | object | none | CSS selector → field name mappings |
| `prompt` | string | none | Natural language extraction instruction |
| `mode` | string | `"auto"` | `"auto"`, `"css"`, or `"llm"` |
| `llmBaseUrl` | string | none | OpenAI-compatible API URL (required for `"llm"` mode) |
| `llmModel` | string | `"gpt-4o-mini"` | LLM model name |
| `llmApiKey` | string | none | LLM API key |

**Response:**

```json
{
  "success": true,
  "data": [
    {
      "title": "Example Domain",
      "links": ["https://www.iana.org/domains/example"]
    }
  ]
}
```

### POST /api/v1/crawl/stream

Streaming variant of the crawl endpoint. Returns results via Server-Sent Events (SSE) as pages are crawled, rather than waiting for the full crawl to complete. Accepts the same parameters as `/api/v1/crawl`.

### GET /api/docs/openapi.json

Auto-generated OpenAPI 3.1 specification for all endpoints. Use with Swagger UI, Postman, or SDK generators.

### GET /health

Health check endpoint. Returns `200 OK` when the server is running. Used by Docker's `HEALTHCHECK`.

---

## MCP Server (AI Agent Integration)

Essence includes a built-in [Model Context Protocol](https://modelcontextprotocol.io/) server, allowing AI agents (Claude, Cursor, Windsurf, etc.) to use scrape, crawl, map, search, extract, and llmstxt as tools directly.

The MCP endpoint is available at `http://localhost:8080/mcp` using the Streamable HTTP transport. Start the Essence server first, then connect your MCP client.

### Available MCP Tools

| Tool | Description |
|---|---|
| `scrape` | Fetch a single page and return Markdown content. Params: `url` (required), `formats`, `engine`, `timeout_ms` |
| `crawl` | Crawl a website up to a given depth and page limit. Params: `url` (required), `max_depth`, `limit`, `include_paths`, `exclude_paths` |
| `map` | Discover URLs on a site via sitemaps and link extraction. Params: `url` (required), `search`, `limit`, `include_subdomains` |
| `search` | Search the web and optionally scrape results. Params: `query` (required), `limit`, `scrape_results` |
| `extract` | Extract structured data from pages. Params: `urls` (required), `schema`, `selectors`, `prompt`, `mode` |
| `llmstxt` | Generate llms.txt from a website. Params: `url` (required), `max_urls`, `llm_base_url`, `show_full_text` |

> **Note:** The MCP server uses `timeout_ms` while the REST API uses `timeout`. Both accept milliseconds.

### Setup with Claude Code

Add to your project's `.mcp.json` (or global `~/.claude/.mcp.json`):

```json
{
  "mcpServers": {
    "essence": {
      "type": "http",
      "url": "http://localhost:8080/mcp"
    }
  }
}
```

Then use Essence tools directly in Claude Code conversations -- Claude will call `scrape`, `crawl`, `map`, `search`, `extract`, and `llmstxt` as needed.

### Setup with Claude Desktop

Add to your Claude Desktop config (`~/Library/Application Support/Claude/claude_desktop_config.json` on macOS):

```json
{
  "mcpServers": {
    "essence": {
      "type": "http",
      "url": "http://localhost:8080/mcp"
    }
  }
}
```

### Setup with Cursor / Windsurf / Other MCP Clients

Point your MCP client at `http://localhost:8080/mcp` using the HTTP transport. No API key required.

---

## Architecture

### Two-Tier Rendering

```
Request → HTTP Engine (fast, lightweight)
              ↓ content density too low?
          Browser Engine (Chromium CDP)
              ↓
          Markdown Output (clean, LLM-ready)
```

1. **HTTP Engine** (default) -- lightweight fetch via reqwest. Used when the HTML contains sufficient content density.
2. **Browser Engine** (fallback) -- full Chromium automation via CDP. Activated for SPAs, anti-bot pages, and JavaScript-rendered content.

Auto-detection triggers browser fallback based on:
- Content density analysis (too little text in HTML)
- Hydration markers (`__NEXT_DATA__`, `__NUXT__`, etc.)
- Meta-refresh redirects
- Anti-fetch response headers

### Module Structure

```
backend/src/
  api/           # Endpoint handlers (scrape, crawl, map, search)
  engines/       # HTTP and Browser rendering engines
  format/        # Markdown and metadata output formatters
  crawler/       # URL frontier, robots.txt, rate limiting, sitemaps, pagination
  search/        # DuckDuckGo HTML search integration
  cache/         # In-memory caching (moka)
  rate_limit/    # Per-domain rate limiting (governor)
  utils/         # DNS cache, URL normalization, robots.txt parsing
  mcp.rs         # MCP server (Model Context Protocol)
  types.rs       # Request/response types
  error.rs       # Error types
  config.rs      # Configuration
  main.rs        # Server startup with graceful shutdown
```

---

## Configuration

Copy `backend/.env.example` to `backend/.env` and customize:

| Variable | Default | Description |
|---|---|---|
| `PORT` | `8080` | Server port |
| `RUST_LOG` | `essence=info` | Log level (`debug`, `info`, `warn`, `error`) |
| `BROWSER_HEADLESS` | `true` | Run Chromium in headless mode |
| `BROWSER_POOL_SIZE` | `5` | Number of browser instances in pool |
| `BROWSER_TIMEOUT_MS` | `30000` | Browser page timeout |
| `ENGINE_WATERFALL_ENABLED` | `true` | Enable HTTP-to-browser fallback |
| `ENGINE_WATERFALL_DELAY_MS` | `5000` | Delay before browser fallback |
| `CRAWL_RATE_LIMIT_PER_SEC` | `2` | Crawl rate limit per domain |
| `MAX_CONCURRENT_REQUESTS` | `10` | Max concurrent crawl requests |
| `MAX_PARALLEL_SCRAPES` | `5` | Parallel scrapes for search results |
| `MAX_REQUEST_SIZE_MB` | `1` | Maximum request body size |
| `CRAWL_MAX_DURATION_SEC` | `300` | Maximum crawl duration in seconds |
| `RETRY_MAX_ATTEMPTS` | `3` | Max retry attempts for failed requests |
| `RETRY_INITIAL_DELAY_MS` | `500` | Initial retry delay |
| `RETRY_MAX_DELAY_MS` | `30000` | Maximum retry delay |

---

## Testing

```bash
cd backend

# Unit tests (377 tests, no network required)
cargo test --lib

# Lint
cargo clippy -- -D warnings

# Integration tests (requires network)
cargo test --test integration -- --ignored

# Browser engine tests (requires Chromium)
cargo test --test browser_engine_tests

# Compile all tests without running
cargo test --no-run
```

---

## Benchmarks

Essence is benchmarked head-to-head against [Firecrawl](https://firecrawl.dev) across 35 real-world URLs spanning 7 categories, evaluated by an LLM judge on 5 quality dimensions.

| Metric | Essence | Target | Status |
|---|---|---|---|
| **Quality Win Rate** (LLM Judge) | **91.2%** (31-2-1) | >= 70% | Exceeded |
| **Speed Win Rate** | **74%** (26-9-0) | >= 50% | Exceeded |
| Success Rate | 100% | 100% | Met |

### Per-Category Quality (LLM Judge)

| Category | Essence Wins | Win Rate |
|---|---|---|
| Structured (Wikipedia, APIs) | 5/5 | **100%** |
| News (BBC, Reuters, Ars) | 5/5 | **100%** |
| Reference (HN, arXiv, dev.to) | 5/5 | **100%** |
| Content (essays, blogs) | 4/5 | **80%** |
| Dynamic (React, Next.js, Vercel) | 4/5 | **80%** |
| Docs (Rust, Python, Go, MDN) | 3/5 | **60%** |
| E-Commerce (Newegg, eBay, IKEA) | 3/5 | **60%** |

See the [full benchmark methodology](https://github.com/ruchit-p/essence/blob/master/docs/BENCHMARKS.md) for detailed results.

---

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup, code style, and PR process.

This repository includes a `CLAUDE.md` for AI-assisted development.

## License

[MIT](LICENSE)
