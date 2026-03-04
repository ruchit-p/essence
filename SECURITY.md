# Security Policy

## Reporting a Vulnerability

If you discover a security vulnerability in Essence, please report it responsibly.

**Email:** security@essence.foundation

Please include:

- Description of the vulnerability
- Steps to reproduce
- Potential impact
- Suggested fix (if any)

We will acknowledge receipt within 48 hours and aim to release a fix within 7 days for critical issues.

## Scope

Security issues in the following areas are in scope:

- **SSRF protection** — bypasses in the SSRF protection layer
- **Input validation** — injection attacks via URL parameters, headers, or request bodies
- **Resource exhaustion** — denial-of-service via crafted requests
- **Information disclosure** — unintended exposure of server internals

## Out of Scope

- Vulnerabilities in third-party dependencies (report these upstream)
- Issues requiring physical access to the server
- Social engineering attacks
