# Contributing to Maestro

Thank you for your interest! This is an early-stage project and contributions are welcome.

## Getting Started

1. Fork the repo
2. Install prerequisites: Rust 1.85+, Docker, Node.js 20+
3. Run `cargo build` to verify the workspace compiles
4. Run `cargo test` to verify tests pass

## Development

### Rust style

- Run `cargo clippy -- -D warnings` before committing
- Use `cargo fmt` for formatting
- New public types get doc comments; internal code is self-documenting

### Frontend style

- Run `npm run build` before committing
- Follow existing patterns in components, API client, and page structure

### Commit messages

Conventional Commits format — see [the log](https://github.com/Vellixia/Maestro/commits/main) for examples.

## Pull Request Process

1. Create a feature branch off `main`
2. Make your changes
3. Run all checks (`cargo test`, `npm run build`)
4. Open a PR with a clear title and description
5. A maintainer will review

## Code of Conduct

See [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).
