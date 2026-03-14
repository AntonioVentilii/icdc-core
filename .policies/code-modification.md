# Policy: Code Modification

## Objective

Ensure all changes to the codebase are predictable, safe, and maintainable.

## Rules

1. **Plan First**: Any change affecting more than 50 lines or involving architecture requires an approved `implementation_plan.md`.
2. **Small & Localized**: Break large tasks into smaller, verifiable patches.
3. **Atomic Changes**: Each commit or pull request should represent a single logical change.
4. **Economic Safety**: Code changes must never compromise the financial integrity of the clearing system.
5. **No Redundancy**: Avoid duplicating logic; use existing utilities and patterns.
