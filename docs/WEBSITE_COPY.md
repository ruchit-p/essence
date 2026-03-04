# Essence -- Website Marketing Copy

*Use this copy for the landing page, product pages, and marketing materials.*

---

## Hero Section

### Headline
**The fastest open-source web scraper for LLMs.**

### Subheadline
Essence converts any web page into clean, LLM-ready Markdown. Built in Rust with intelligent HTTP-to-browser fallback. Self-hosted, no API keys, no rate limits.

### CTA
Get Started -- it's free and open-source.

---

## Stats Bar

| 91% Quality Win Rate | 1.5x Faster | 100% Open Source | Zero Dependencies |
|---|---|---|---|
| Wins 31 of 34 quality comparisons against Firecrawl, judged by LLM evaluation | Median response time of 700ms vs 1,043ms | MIT licensed, self-hosted, no vendor lock-in | No Redis, no Postgres, no external services required |

---

## "Why Essence?" Section

### Better output. Faster.

We benchmarked Essence head-to-head against Firecrawl across 35 real-world URLs -- news sites, documentation, e-commerce, SPAs, wikis, blogs, and more. An LLM judge evaluated each pair on content quality, noise removal, readability, structure, and completeness.

**Essence won 91% of quality comparisons.** It also responded 1.5x faster at the median.

### Benchmark Highlights

**Quality (LLM Judge -- 35 URLs, 7 categories):**
- 100% win rate on structured content (Wikipedia, API references)
- 100% win rate on news sites (BBC, Reuters, Ars Technica, The Guardian, Wired)
- 100% win rate on reference sites (Hacker News, arXiv, DEV Community)
- 80% win rate on long-form content and dynamic SPAs
- Overall: 31 wins, 2 ties, 2 losses

**Speed:**
- 2.8x faster on news sites (511ms vs 1,406ms)
- 1.8x faster on documentation (401ms vs 713ms)
- 1.5x faster overall (700ms vs 1,043ms median)
- Faster on 74% of URLs tested

---

## Features Section

### Two-Tier Rendering

Most scrapers choose one approach: lightweight HTTP or heavyweight browser. Essence uses both.

It starts with a fast HTTP fetch (sub-second for most pages). If the content density is too low -- a sign of JavaScript-rendered content -- it automatically escalates to full Chromium browser automation. You get speed when possible and reliability when needed.

### LLM-Optimized Markdown

Essence doesn't just convert HTML to Markdown. It produces output specifically optimized for LLM consumption:

- **Noise removal** -- strips navigation, ads, footers, cookie banners, "Loading..." placeholders, and boilerplate
- **Code preservation** -- inline code spans and code blocks are protected through the conversion pipeline
- **Clean structure** -- proper heading hierarchy, normalized links, deduplicated list items
- **Metadata extraction** -- title, description, Open Graph tags, word count, reading time, detected frameworks

### Four Endpoints, One Server

| Endpoint | What it does |
|---|---|
| **Scrape** | Fetch a single page. Returns Markdown, HTML, links, images, metadata. |
| **Crawl** | Traverse a site. Follows links with depth control, dedup, robots.txt compliance, rate limiting, and pagination detection. |
| **Map** | Discover URLs. Combines sitemap parsing with in-page link extraction. |
| **Search** | Search the web via DuckDuckGo. Optionally scrape each result for full content. |

### MCP Server for AI Agents

Essence includes a built-in Model Context Protocol (MCP) server. Connect Claude, Cursor, Windsurf, or any MCP client to give your AI agent the ability to scrape, crawl, map, and search the web.

No SDKs, no wrappers -- just point your MCP client at `http://localhost:8080/mcp`.

### Self-Hosted, Zero Dependencies

Essence is a single Rust binary. No Redis, no PostgreSQL, no external services. Run it with `cargo run` or `docker-compose up`. Configure via environment variables.

---

## Comparison Table

| Feature | Essence | Firecrawl |
|---|---|---|
| LLM-ready Markdown | Yes | Yes |
| Open source | MIT | AGPL |
| Self-hosted | Single binary, zero deps | Requires Redis, multiple services |
| Browser fallback | Automatic HTTP-to-Chromium | Requires explicit configuration |
| MCP server | Built-in | Separate package |
| API key required | No | Yes (cloud) |
| Rate limits | None (self-hosted) | Tiered pricing |
| Quality (LLM judge) | **91% win rate** | 6% win rate |
| Median speed | **700ms** | 1,043ms |
| Crawling | robots.txt, rate limits, pagination | robots.txt, rate limits |
| Search | Built-in DuckDuckGo | Separate service |
| Language | Rust | TypeScript/Python |
| Pricing | Free forever | Free tier + paid plans |

---

## How It Works Section

```
Your app  -->  POST /api/v1/scrape  -->  Essence
                                          |
                                    HTTP Engine (fast)
                                          |
                                    Content dense enough?
                                     /            \
                                   Yes              No
                                    |                |
                              Return Markdown    Browser Engine
                                                (Chromium CDP)
                                                     |
                                               Return Markdown
```

1. **Send a URL** -- POST to any of the 4 endpoints
2. **Smart rendering** -- Essence tries HTTP first. If the page is a JavaScript SPA, it automatically falls back to headless Chromium
3. **Get clean Markdown** -- receive structured output with metadata, ready for your LLM pipeline

---

## Integration Section

### Works with everything

```bash
# cURL
curl -X POST http://localhost:8080/api/v1/scrape \
  -H "Content-Type: application/json" \
  -d '{"url": "https://example.com"}'
```

```python
# Python
import requests
response = requests.post("http://localhost:8080/api/v1/scrape", json={
    "url": "https://example.com",
    "formats": ["markdown", "metadata"]
})
markdown = response.json()["data"]["markdown"]
```

```javascript
// JavaScript
const response = await fetch("http://localhost:8080/api/v1/scrape", {
  method: "POST",
  headers: { "Content-Type": "application/json" },
  body: JSON.stringify({ url: "https://example.com" })
});
const { data } = await response.json();
```

```json
// MCP (Claude, Cursor, Windsurf)
{
  "mcpServers": {
    "essence": {
      "type": "http",
      "url": "http://localhost:8080/mcp"
    }
  }
}
```

---

## Social Proof / Metrics Section

### By the Numbers

- **91.2%** quality win rate vs Firecrawl (LLM judge, 35 URLs)
- **1.5x** faster median response time
- **700ms** median scrape time
- **377** unit tests
- **35** benchmark URLs across **7** content categories
- **100%** success rate on all tested URLs
- **0** external dependencies (no Redis, no Postgres)

---

## CTA Section

### Get started in 60 seconds

```bash
git clone https://github.com/ruchit-p/essence.git
cd essence/backend
cp .env.example .env
cargo run --release
# Server running at http://localhost:8080
```

Or use Docker:

```bash
docker-compose up -d
```

[View on GitHub](https://github.com/ruchit-p/essence) | [Read the docs](https://github.com/ruchit-p/essence#readme) | [See benchmarks](https://github.com/ruchit-p/essence/blob/master/docs/BENCHMARKS.md)
