# Capability: Architecture Changes

## Allowed

- Refactor internal module structures to improve sharding and isolation.
- Add new internal APIs or abstractions that follow established patterns.
- Propose migrations or improvements via an implementation plan.

## Forbidden

- Change the core domain atomicity model without unanimous approval.
- Alter the fundamental sharding strategy described in `SHARDING_PLAN.md`.
- Introduce new external dependencies at the architectural level without review.
