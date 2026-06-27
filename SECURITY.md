# Security Policy

## Reporting a Vulnerability

If you discover a security vulnerability in Maestro, please report it privately by opening a GitHub Security Advisory:

https://github.com/Vellixia/Maestro/security/advisories/new

Do **not** report security vulnerabilities via public issues, pull requests, or discussions.

## Scope

- The Rust API server (`crates/api`)
- The provider gateway (`crates/gateway`) — API key handling, credential storage
- Authentication and authorization (`crates/api` middleware)
- Docker sandbox (`crates/verifier`) — code execution isolation

## Non-Goals

- Security of provider API keys stored in environment variables is the user's responsibility
- SurrealDB instance security is the user's responsibility

## Response

You can expect acknowledgment within 48 hours, and a fix timeline will be communicated based on severity.
