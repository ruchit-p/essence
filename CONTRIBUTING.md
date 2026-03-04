# Contributing to Essence

Thanks for your interest in contributing to Essence! This guide will help you get started.

## Development Setup

### Prerequisites

- **Rust** (stable toolchain): [Install via rustup](https://rustup.rs/)
- **Chromium/Chrome** (optional): Only needed for browser engine tests. The HTTP engine handles most pages without a browser.

### Getting Started

```bash
git clone https://github.com/ruchit-p/essence.git
cd essence/backend
cp .env.example .env
cargo build
```

### Running the Server

```bash
cargo run --release   # Serves on port 8080
```

### Running Tests

```bash
# Unit tests (no network required, fast)
cargo test --lib

# Integration tests (requires network)
cargo test --test integration -- --ignored

# Browser engine tests (requires Chromium installed)
cargo test --test browser_engine_tests
```

## Making Changes

### Code Style

- Follow standard Rust conventions (`cargo fmt`, `cargo clippy`)
- Use the error types from `backend/src/error.rs` — return `Result<T, ScrapeError>` from fallible operations
- Keep changes focused — one concern per PR

### Testing Your Changes

Before submitting a PR, ensure:

1. `cargo test --lib` passes (all unit tests)
2. `cargo clippy -- -D warnings` produces no warnings
3. `cargo build --release` compiles cleanly

### AI-Assisted Development

This repository includes a `CLAUDE.md` file with project context for AI coding assistants. Feel free to use it with tools like Claude Code.

## Pull Request Process

1. Fork the repository and create a feature branch from `master`
2. Make your changes with clear, focused commits
3. Ensure all tests pass and clippy is clean
4. Open a PR with a clear description of what changed and why
5. Link any related issues

## Reporting Issues

When opening an issue, please include:

- What you were trying to do
- What happened instead
- Steps to reproduce (URL being scraped, engine used, etc.)
- Relevant error messages or unexpected output

## License

By contributing, you agree that your contributions will be licensed under the [MIT License](LICENSE).
