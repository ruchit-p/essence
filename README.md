# Essence

A fast, open-source web retrieval engine built in Rust. Fetches pages via lightweight HTTP with automatic fallback to headless Chromium for JavaScript-heavy sites. Returns clean, LLM-ready Markdown.

**91% quality win rate** against Firecrawl across 35 real-world URLs, judged by LLM evaluation. **69% faster** on average.

## Why Essence?

- **Two-tier rendering** -- lightweight HTTP fetch for most pages, automatic Chromium fallback for SPAs and JS-heavy sites
- **LLM-optimized output** -- clean Markdown with noise removal, heading hierarchy, code preservation, and structured metadata
- **4 REST endpoints + MCP server** -- scrape, crawl, map, search -- usable by humans and AI agents alike
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
  -d '{"url": "https://example.com", "max_depth": 2, "limit": 10}' | jq .

# Search the web
curl -s -X POST http://localhost:8080/api/v1/search \
  -H "Content-Type: application/json" \
  -d '{"query": "rust web scraping", "num_results": 5}' | jq .
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
| `only_main_content` | bool | `false` | Extract only the main content area |

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
      "status_code": 200,
      "word_count": 1234,
      "reading_time": 5
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
| `max_depth` | int | `3` | Maximum link depth to follow |
| `limit` | int | `100` | Maximum pages to crawl |
| `include_paths` | string[] | none | Glob patterns to include (e.g. `["/blog/*"]`) |
| `exclude_paths` | string[] | none | Glob patterns to exclude (e.g. `["/admin/*"]`) |
| `allow_backward_links` | bool | `false` | Follow links up the URL hierarchy |
| `allow_external_links` | bool | `false` | Follow links to other domains |
| `detect_pagination` | bool | `true` | Automatically follow paginated content |

**Response:**

```json
{
  "success": true,
  "data": [
    {
      "markdown": "# Home\n\nWelcome...",
      "metadata": { "title": "Home", "url": "https://example.com", "status_code": 200 }
    },
    {
      "markdown": "# About\n\nAbout us...",
      "metadata": { "title": "About", "url": "https://example.com/about", "status_code": 200 }
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
| `include_subdomains` | bool | `false` | Include URLs from subdomains |
| `limit` | int | `5000` | Maximum URLs to return |
| `search` | string | none | Filter URLs by search query |
| `ignore_sitemap` | bool | `false` | Skip sitemap.xml discovery |

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
| `num_results` | int | `10` | Number of search results |
| `scrape_top` | int | `0` | Scrape full content of top N results |

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

---

## MCP Server (AI Agent Integration)

Essence includes a built-in [Model Context Protocol](https://modelcontextprotocol.io/) server, allowing AI agents (Claude, Cursor, Windsurf, etc.) to use scrape, crawl, map, and search as tools directly.

The MCP endpoint is available at `http://localhost:8080/mcp` using the Streamable HTTP transport. Start the Essence server first, then connect your MCP client.

### Available MCP Tools

| Tool | Description |
|---|---|
| `scrape` | Fetch a single page and return Markdown content. Params: `url` (required), `formats`, `engine`, `timeout_ms` |
| `crawl` | Crawl a website up to a given depth and page limit. Params: `url` (required), `max_depth`, `limit`, `include_paths`, `exclude_paths` |
| `map` | Discover URLs on a site via sitemaps and link extraction. Params: `url` (required), `search`, `limit`, `include_subdomains` |
| `search` | Search the web and optionally scrape results. Params: `query` (required), `limit`, `scrape_results` |

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

Then use Essence tools directly in Claude Code conversations -- Claude will call `scrape`, `crawl`, `map`, and `search` as needed.

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
| **Speed Win Rate** | **68.6%** (24-7-5) | >= 50% | Exceeded |
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

See [docs/BENCHMARKS.md](docs/BENCHMARKS.md) for full methodology and results.

---

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup, code style, and PR process.

This repository includes a `CLAUDE.md` for AI-assisted development, plus Claude Code skills (`.claude/skills/`) for common workflows like setup, testing, and code exploration.

## License

[MIT](LICENSE)
