# Policy: Testing Policy

## Objective

Ensure high confidence in code correctness through automated testing.

## Rules

1. **Coverage**: Every new feature or bug fix must include corresponding tests.
2. **Regressions**: Existing tests must pass before and after any changes.
3. **Test Standards**: Follow the conventions defined in [.workflows/testing-standards.md](file:///Users/antonio.ventilii/projects/icdc-core/.workflows/testing-standards.md).
4. **Integration Tests**: Significant changes to API or state logic must be verified with integration tests using `dfx`.
