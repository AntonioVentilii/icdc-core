# Boundary: Security

## Objective

Isolate and protect sensitive security and authentication modules.

## Restricted Areas

- Modules handling Principal authorization.
- Cryptographic primitives (if any).
- Ledger and account balance verification logic.

## Rules

- Changes to these areas require extreme caution and verbose documentation.
- Reviewer agents must prioritize these areas for deep analysis.
