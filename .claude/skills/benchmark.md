# /benchmark -- View or run benchmark results

<skill>
description: View the latest benchmark results comparing Essence vs Firecrawl, or run a new benchmark. Shows quality scores, speed comparisons, and per-category breakdowns.
user_invocable: true
arg_description: "Optional: 'run' to execute a new benchmark, 'dashboard' to regenerate the dashboard, or omit to view latest results"
</skill>

## Behavior

### View Results (default)

Read the latest benchmark data and present a summary:

1. Read `docs/loop/progress.md` for the current status
2. Read `docs/loop/competitive_scores.json` for detailed per-URL results
3. Present:
   - Overall LLM judge win rate and speed win rate
   - Per-category breakdown table
   - Any remaining losses and why
   - Improvement history across cycles

### `run` -- Execute a New Benchmark

**Prerequisites:**
- Essence server must be running (`cd backend && cargo run --release`)
- Firecrawl must be running at localhost:3002 (`cd firecrawl && docker compose up -d`)

Run the benchmark:
```bash
cd backend && LLM_JUDGE=true SAVE_MARKDOWN=true cargo test --release --test competitive_benchmark -- --ignored --nocapture competitive_benchmark_run
```

After completion, read and summarize the new results from `docs/loop/competitive_scores.json`.

### `dashboard` -- Regenerate Dashboard

Regenerate the HTML dashboard from existing database data without re-scraping:
```bash
cd backend && cargo test --test competitive_benchmark -- --ignored --nocapture regenerate_dashboard
```

The dashboard will be at `docs/loop/dashboard.html` (open in browser).

## Key Files

- `docs/loop/benchmark.db` -- SQLite database with all historical runs
- `docs/loop/dashboard.html` -- Interactive Chart.js dashboard (dark theme)
- `docs/loop/competitive_scores.json` -- Latest head-to-head comparison results
- `docs/loop/benchmark_outputs/` -- Raw markdown outputs (essence/ and firecrawl/)
- `backend/tests/competitive_benchmark.rs` -- Benchmark test implementation
- `backend/tests/benchmark/llm_judge.rs` -- LLM-as-judge evaluation
