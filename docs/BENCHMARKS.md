# Essence vs Firecrawl: Benchmark Results

Head-to-head comparison of Essence and [Firecrawl](https://firecrawl.dev) across 35 real-world URLs spanning 7 content categories.

**Bottom line:** Essence wins **91.2% of quality comparisons** (LLM judge) and is **1.5x faster** (median response time).

## Summary

| Metric | Essence | Firecrawl |
|---|---|---|
| **Quality Win Rate** (LLM Judge) | **91.2%** (31 wins) | 5.7% (2 wins) |
| **Speed Win Rate** | **74%** (26 wins) | 26% (9 wins) |
| Median Response Time | **700ms** | 1,043ms |
| Success Rate | 100% | 100% |

*2 ties on quality. Evaluated March 2026.*

## Quality: Per-Category Breakdown

An LLM judge evaluated each URL pair across 5 dimensions (content relevance, noise removal, readability, structural coherence, information completeness) and selected an overall winner.

| Category | URLs | Essence Wins | Firecrawl Wins | Ties | Essence Win Rate |
|---|---|---|---|---|---|
| Structured (Wikipedia, Stripe API) | 5 | **5** | 0 | 0 | **100%** |
| News (BBC, Reuters, Ars Technica) | 5 | **5** | 0 | 0 | **100%** |
| Reference (Hacker News, arXiv) | 5 | **5** | 0 | 0 | **100%** |
| Content (Paul Graham, Fowler) | 5 | **4** | 0 | 1 | **80%** |
| Dynamic (React, Vercel, Supabase) | 5 | **4** | 0 | 1 | **80%** |
| Docs (Rust Book, Python, MDN) | 5 | **3** | 2 | 0 | **60%** |
| E-Commerce (Newegg, eBay, IKEA) | 5 | **3** | 1 | 1 | **60%** |

## Speed: Per-Category Breakdown

| Category | Essence Avg | Firecrawl Avg | Speedup |
|---|---|---|---|
| News | **511ms** | 1,406ms | **2.8x** |
| Docs | **401ms** | 713ms | **1.8x** |
| Structured | **1,003ms** | 1,616ms | **1.6x** |
| Content | **929ms** | 1,400ms | **1.5x** |
| Dynamic | **1,059ms** | 1,530ms | **1.4x** |
| Reference | **1,231ms** | 1,251ms | 1.0x |
| E-Commerce | 5,749ms | **1,569ms** | 0.3x* |

*\*E-Commerce avg skewed by one URL triggering browser fallback (21.9s). Excluding that outlier, Essence averages 1,212ms across remaining e-commerce URLs.*

## Test Corpus

35 URLs across 7 categories, selected to represent diverse real-world web content:

| Category | URLs Tested |
|---|---|
| **Content** | Paul Graham essays, Wait But Why, Martin Fowler, Dan Luu, Stanford Encyclopedia |
| **Docs** | Rust Book, Python Tutorial, Effective Go, MDN, Docker Docs |
| **Dynamic** | React docs, Vercel, Supabase, Linear, Next.js |
| **Structured** | Wikipedia (2), Arch Wiki, Stripe API, OpenAI API |
| **News** | BBC, Reuters, Ars Technica, The Guardian, Wired |
| **E-Commerce** | Books to Scrape, Newegg, eBay, IKEA, Steam |
| **Reference** | Hacker News, Slashdot, DEV Community, arXiv, Nature |

## Methodology

### LLM Judge (Primary Quality Signal)

Each URL is scraped by both Essence and Firecrawl. The two Markdown outputs are evaluated by an LLM judge (Claude) across 5 dimensions:

1. **Content Relevance** -- main content captured, irrelevant content excluded
2. **Noise Removal** -- navigation, ads, footers, boilerplate removed
3. **Readability** -- clean Markdown, proper heading hierarchy, token-efficient
4. **Structural Coherence** -- logical structure, proper nesting
5. **Information Completeness** -- all critical information preserved

The judge selects a per-dimension winner and an overall quality winner for each URL. Results are stored in a SQLite database for historical tracking.

### Objective Metrics (Supporting)

Heuristic metrics are tracked for diagnostics but do not determine winners:

| Metric | Weight | Role |
|---|---|---|
| Markdown Cleanliness | 2.5x | HTML artifact detection |
| Code Block Preservation | 2.0x | Formatting integrity |
| Speed | 2.0x | Response time |
| Link Preservation | 1.5x | Content completeness |
| Image Preservation | 1.5x | Content completeness |
| Metadata Extraction | 1.5x | Title, description, OG tags |
| Heading Preservation | 1.0x | Document structure |
| Word Count | 0.5x | Content volume |
| Content Density | 0.5x | Signal-to-noise ratio |

### Firecrawl Configuration

Firecrawl is self-hosted via Docker (`firecrawl/docker-compose.yml`) at `localhost:3002`. No cloud API is used -- both engines run locally for a fair comparison.

## Quality Improvement History

Essence's Markdown output quality was improved through iterative benchmark-driven development:

| Cycle | LLM Judge Win Rate | Speed Win Rate | Key Changes |
|---|---|---|---|
| Baseline | 52.9% | 14.3% | Initial benchmark |
| Cycle 1 | 79.4% (+26.5pp) | 65.7% (+51.4pp) | Inline code protection, JS cleanup, UI noise removal |
| Cycle 2 | **91.2%** (+11.8pp) | **68.6%** (+2.9pp) | Copyright stripping, link normalization, list dedup |

## Remaining Losses

Only 2 URLs where Firecrawl produces better output:

| URL | Category | Root Cause |
|---|---|---|
| `platform.openai.com/docs/api-reference` | Docs | SPA -- content is entirely JavaScript-rendered |
| `ebay.com/b/Laptops-Netbooks` | E-Commerce | Dynamic product listings require browser rendering |

Both losses are inherent to the HTTP engine: these pages serve empty shells that require JavaScript execution. Essence's browser engine handles them correctly, but the auto-detection doesn't trigger for these specific pages.

## Reproducing These Results

```bash
# 1. Start Essence
cd backend && cargo run --release

# 2. Start Firecrawl (self-hosted)
cd firecrawl && docker compose up -d

# 3. Run the benchmark
cd backend && LLM_JUDGE=true SAVE_MARKDOWN=true \
  cargo test --release --test competitive_benchmark \
  -- --ignored --nocapture competitive_benchmark_run

# Results appear in:
#   docs/loop/competitive_scores.json  -- detailed per-URL results
#   docs/loop/dashboard.html           -- interactive dashboard
#   docs/loop/benchmark_outputs/       -- raw Markdown outputs for comparison
```

### Environment Variables

| Variable | Default | Description |
|---|---|---|
| `LLM_JUDGE` | `false` | Enable Claude-based LLM judge evaluation |
| `SAVE_MARKDOWN` | `false` | Save raw Markdown outputs for manual review |
| `BENCHMARK_TIMEOUT` | `30000` | Per-URL timeout in ms |
| `BENCHMARK_THROTTLE` | `1000` | Delay between requests in ms |
| `BENCHMARK_SUBSET` | all | Filter by category (e.g. `news,docs`) |
| `LLM_JUDGE_CONCURRENCY` | `15` | Max parallel LLM evaluations |
