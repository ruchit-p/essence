# /init -- Set up Essence for development

<skill>
description: Initialize the Essence project for local development. Installs dependencies, sets up environment, builds the project, and verifies everything works.
user_invocable: true
</skill>

## Steps

1. **Check prerequisites** -- verify Rust toolchain is installed (`rustc --version`, `cargo --version`). If missing, tell the user to install via https://rustup.rs/
2. **Check for Chromium** (optional) -- check if `chromium` or `google-chrome` is available. If not, note that the HTTP engine will work fine without it but browser fallback won't be available
3. **Set up environment** -- if `backend/.env` doesn't exist, copy `backend/.env.example` to `backend/.env`
4. **Build the project** -- run `cd backend && cargo build --release`
5. **Run unit tests** -- run `cd backend && cargo test --lib` to verify everything compiles and passes
6. **Report status** -- summarize what was set up, what's ready, and any optional components that are missing

## Notes

- The server runs on port 8080 by default (configurable in `.env`)
- Start the server with `cd backend && cargo run --release`
- The MCP endpoint will be available at `http://localhost:8080/mcp`
- No external databases or services are required
- Docker alternative: `docker-compose up -d` from the project root
