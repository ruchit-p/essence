# /dev -- Development workflow commands

<skill>
description: Run common development tasks -- build, test, run the server, lint, or run benchmarks. Pass a subcommand to specify what to do.
user_invocable: true
arg_description: "Subcommand: build, test, run, lint, bench, or browser-test"
</skill>

## Subcommands

Parse the user's argument to determine which subcommand to run. All commands run from the `backend/` directory.

### `build` (default if no argument)
```bash
cd backend && cargo build --release
```

### `test`
Run the unit test suite (377 tests, no network required):
```bash
cd backend && cargo test --lib
```

### `run`
Start the Essence server in release mode:
```bash
cd backend && cargo run --release
```
The server will be available at `http://localhost:8080` with the MCP endpoint at `http://localhost:8080/mcp`.

### `lint`
Run clippy and format check:
```bash
cd backend && cargo clippy -- -D warnings && cargo fmt --check
```

### `bench`
Run the competitive benchmark against Firecrawl (requires Firecrawl running at localhost:3002):
```bash
cd backend && LLM_JUDGE=true SAVE_MARKDOWN=true cargo test --release --test competitive_benchmark -- --ignored --nocapture competitive_benchmark_run
```

### `browser-test`
Run browser engine tests (requires Chromium installed):
```bash
cd backend && cargo test --test browser_engine_tests
```

### `integration`
Run integration tests (requires network):
```bash
cd backend && cargo test --test integration -- --ignored
```

## Notes

- If the user doesn't specify a subcommand, ask them what they'd like to do
- Unit tests (`test`) are the safest starting point -- they need no network or browser
- The server (`run`) must be running for MCP clients to connect
- Benchmarks (`bench`) require Firecrawl self-hosted: `cd firecrawl && docker compose up -d`
