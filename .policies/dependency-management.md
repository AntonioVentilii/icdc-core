# Policy: Dependency Management

## Objective

Maintain a secure and minimal dependency graph for the project.

## Rules

1. **Security First**: Never introduce dependencies with known security vulnerabilities.
2. **Preference for Std/Common**: Use standard libraries or established repository crates (e.g., `ic-cdk`, `serde`) before adding new external dependencies.
3. **Audit Required**: New external crates must be reviewed for performance, security, and maintenance status.
4. **Pinned Versions**: All dependencies must have exact versions specified in `Cargo.toml`.
