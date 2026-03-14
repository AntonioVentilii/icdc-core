# Boundary: Infrastructure

## Objective

Protect the core repository infrastructure and CI/CD pipelines.

## Restricted Areas

- `.github/workflows/*`: Critical for repository integrity and security checks.
- `dfx.json`: Main project configuration for the Internet Computer.
- `Cargo.toml` (root): Project-wide dependency and workspace configuration.
- `scripts/`: Critical automation and initialization scripts.

## Rules

- Agents should not modify these files unless explicitly asked to fix a specific infrastructure issue.
- Any change to these areas must be highlighted in the implementation plan.
