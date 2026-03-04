---
name: Bug Report
about: Report a bug or unexpected behavior
title: "[Bug] "
labels: bug
---

## Describe the bug

A clear description of what the bug is.

## To reproduce

1. Send request to `POST /api/v1/scrape` with body:
```json
{
  "url": "https://example.com",
  "formats": ["markdown"]
}
```
2. Observe ...

## Expected behavior

What you expected to happen.

## Actual behavior

What actually happened. Include response body or error messages.

## Environment

- Essence version: [e.g., 0.1.0]
- OS: [e.g., Ubuntu 22.04]
- Deployment: [Docker / binary / cargo run]
- Engine: [auto / http / browser]

## Additional context

Any other relevant information (logs, screenshots, etc.).
