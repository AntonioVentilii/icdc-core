# Policy: AI Meta-Updates

This policy governs how AI configuration and governance files must be updated to maintain system integrity and agent predictability.

## Rules

### 1. Synchronization

Whenever a code change introduces a new architectural pattern, a fundamental library update, or changes a domain boundary, the corresponding AI guidance file in `.agents/` or `.policies/` **MUST** be updated in the same PR.

### 2. No Circular Dependencies

Guidance files should not rely on external AI tools for their own definition. They must be human-readable and deterministic.

### 3. Truth Hierarchy

- Code reflects the current state.
- `.policies/` and `.boundaries/` define the non-negotiable constraints.
- `AGENTS.md` and `.agents/` define the operational guidance.

### 4. Atomic Updates

Updates to the governance layer should be atomic and self-consistent. If you change a rule in `.agents/instructions.md`, ensure it does not conflict with a policy in `.policies/`.

### 5. Review Requirement

Any change to files in `.policies/` or `.boundaries/` requires explicit human review and approval.

## Enforcement

AI agents must verify compliance with this policy before submitting work. Failure to update guidance in sync with architectural changes is considered a "governance debt" and must be addressed.
