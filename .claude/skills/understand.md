# /understand -- Explore and explain the Essence codebase

<skill>
description: Understand the Essence codebase architecture, how a specific feature works, or how to make changes. Pass a topic to focus on, or omit for a full overview.
user_invocable: true
arg_description: "Optional topic: architecture, api, mcp, engines, crawler, markdown, testing, or a specific file/feature"
</skill>

## Behavior

Based on the user's argument, explore the relevant parts of the codebase and explain how they work. If no argument is given, provide a high-level architecture overview.

## Architecture Overview (default)

Read and summarize the project structure from `CLAUDE.md` and the key source files:

- **Entry point**: `backend/src/main.rs` -- Axum server setup, MCP service mount, graceful shutdown
- **API layer**: `backend/src/api/` -- endpoint handlers for scrape, crawl, map, search
- **Engines**: `backend/src/engines/` -- HTTP engine (reqwest) and Browser engine (chromiumoxide/CDP)
- **Formatting**: `backend/src/format/` -- HTML-to-Markdown conversion, metadata extraction, cleanup pipeline
- **Crawler**: `backend/src/crawler/` -- URL frontier, robots.txt, rate limiting, sitemap parsing, pagination
- **MCP**: `backend/src/mcp.rs` -- Model Context Protocol server exposing 4 tools
- **Types**: `backend/src/types.rs` -- all request/response structs
- **Errors**: `backend/src/error.rs` -- ScrapeError enum

## Topic-Specific Guides

### `architecture`
Read main.rs, lib.rs, and the module structure. Explain the request flow from HTTP request to Markdown output.

### `api`
Read `backend/src/api/` handlers. Explain each endpoint, how requests are validated, and how responses are constructed.

### `mcp`
Read `backend/src/mcp.rs`. Explain the 4 MCP tools, their parameters, how they map to the REST API, and the rmcp framework.

### `engines`
Read `backend/src/engines/`. Explain the HTTP engine, browser engine, auto-detection logic, and waterfall fallback.

### `crawler`
Read `backend/src/crawler/`. Explain the URL frontier, dedup, robots.txt compliance, rate limiting, sitemap discovery, and pagination.

### `markdown`
Read `backend/src/format/`. Explain the HTML-to-Markdown pipeline, cleanup stages (HTML stripping, noise removal, code protection, link normalization).

### `testing`
List test files in `backend/tests/` and explain each. Cover unit tests vs integration tests vs benchmarks.

## Notes

- Always read the actual source files -- don't rely on summaries alone
- The CLAUDE.md file has the canonical project overview
- Key safety rules from the quality loop are documented in the project memory
- When explaining, include file paths and line numbers so users can navigate to the source
