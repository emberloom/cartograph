# Contributing to Cartograph

Thank you for your interest in contributing. This is a small, focused project — changes should stay within that spirit.

## Before Submitting

- `cargo test` — all tests must pass
- `cargo clippy -- -D warnings` — no clippy warnings
- `cargo fmt --check` — code must be formatted

## What We Welcome

- Bug fixes with a failing test that now passes
- New language parsers (TypeScript, Python, Go) — open an issue first to discuss approach
- Performance improvements with benchmarks
- Documentation improvements

## What to Avoid

- Sweeping refactors without prior discussion
- New dependencies unless clearly necessary
- Changes that break the MCP tool interface without a migration path

## Process

1. Open an issue describing the change before writing code (for non-trivial work)
2. Fork, branch off `main`, make your changes
3. Open a pull request — CI must be green

Questions → [GitHub Issues](https://github.com/emberloom/cartograph/issues)