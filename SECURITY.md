# Security Policy

## Supported Versions

Security fixes are provided for the current `main` branch and the latest tagged release.

## Reporting a Vulnerability

Please do not open public issues for suspected vulnerabilities.

Use one of these channels:

1. Open a **private GitHub Security Advisory** for this repository (preferred).
2. If unavailable, open a minimal issue asking maintainers for a private contact path — do not include exploit details in the public issue.

Include:

- Affected commit or tag
- Impact summary
- Reproduction steps
- Suggested mitigation (if known)

## Attack Surface

Cartograph has a narrow attack surface:

**MCP server (`serve` subcommand)**
- Accepts entity path strings from callers. Path traversal is blocked — inputs containing `..` are rejected before any file or database access.
- `depth` is capped at 10 and `limit` at 500.
- The server reads from a local SQLite database only; it does not make network requests.

**Git history mining (`index` subcommand)**
- Invokes `git log` as a subprocess in the target repository directory.
- No credentials are read or stored. Cartograph does not access remote git hosts.

**Database**
- SQLite file stored locally at `.cartograph/db.sqlite` in the indexed repo. Cartograph does not transmit this data anywhere.

## No Credentials Required

Cartograph does not use API keys, tokens, or secrets for its core operation. If you are scripting around it, keep any surrounding credentials out of committed files.