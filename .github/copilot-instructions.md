# Copilot Instructions

## Purpose

This file defines how GitHub Copilot should generate, modify, and review code in this repository.

The repository uses structured documentation and an agent system.
Copilot must rely on the existing documentation and avoid inventing new patterns or architecture.

This file acts as a **navigation layer and behaviour guide** for Copilot.

---

# Critical Rules

These rules always take precedence.

- Prefer **small, localised patches** over rewriting files.
- Preserve the existing **architecture and directory structure**.
- Follow established **naming conventions and code patterns**.
- Do not introduce new frameworks or dependencies without explicit instruction.
- When behaviour changes, update **tests and relevant documentation**.
- Respect governance rules defined in `.policies/`, `.capabilities/`, and `.boundaries/`.
- Avoid modifying **CI, infrastructure, deployment, or security-related files** unless explicitly requested.

If TypeScript is used:

- Never introduce the `any` type.
- Prefer explicit and safe typing.

---

# Context Priority

When determining repository conventions, consult files in this order:

1. `AGENTS.md`
2. `docs/ai/governance.md`
3. `.policies/`, `.capabilities/`, `.boundaries/`, `.workflows/`
4. `.agents/`
5. `CLAUDE.md`
6. `README.md`
7. `TODO.md`

Higher priority sources override lower priority ones.

---

# Repository Architecture

The system architecture is defined in:

- `.agents/architecture.md`
- `NEW_ARCHITECTURE.md`

Copilot should follow these principles:

- Keep logic within its designated layer (e.g., API, Domain, Sharding).
- Avoid cross-layer dependencies.
- Reuse existing modules rather than introducing new ones.
- Follow existing patterns found in neighbouring files.

If the repository contains multiple packages or apps, maintain their boundaries.

---

# Subsystem Boundaries

Major subsystems in this repository should be treated as **stable boundaries**.

Examples:

- API layer (`src/clearing/src/api/`)
- Domain logic (`src/clearing/src/domain/`)
- Types and State (`src/clearing/src/types/`)
- Sharding and Scaling (`SHARDING_PLAN.md`)

Copilot should avoid:

- moving logic between subsystems
- creating hidden dependencies
- tightly coupling independent modules

If a change requires cross-subsystem modifications, minimise impact.

---

# Agent System

The repository uses an **agent-based workflow**.

Authoritative documentation:

- `AGENTS.md`
- `.agents/`

Copilot must respect the responsibilities and boundaries defined there.

Examples:

- `Planner`: Architecture and decomposition.
- `Implementer`: Code modification and technical compliance.
- `Reviewer`: Quality and economic safety.

Copilot should **not modify agent definitions or configuration** unless explicitly asked.

---

# Change Risk Classification

Copilot should evaluate changes according to risk level.

## Low Risk

Safe modifications such as:

- documentation updates
- comments
- test improvements
- small bug fixes
- refactoring without behaviour changes

These changes are generally acceptable.

---

## Medium Risk

Changes that require more attention:

- modifying business logic
- altering public functions
- changing database or state queries
- updating dependencies

Ensure tests and compatibility are preserved.

---

## High Risk

Changes that should be avoided unless explicitly requested:

- architecture changes
- CI/CD modifications
- deployment or infrastructure changes
- security-related logic (economic safety, atomicity)
- deleting large sections of code

Copilot should avoid proposing these changes unless specifically instructed.

---

# Coding Standards

Coding style and conventions are defined in:

- `.agents/patterns.md`
- `.agents/instructions.md`

Copilot should:

- follow existing formatting rules (`rustfmt`, `prettier`)
- match naming conventions used in nearby code (snake_case for Rust)
- reuse existing helpers and utilities (see `scripts/utils.sh`)

Generated code must be compatible with the repository's linting and formatting tools.

---

# Testing Expectations

When generating or modifying code:

- maintain or improve test coverage
- update tests if behaviour changes
- avoid removing tests without removing the behaviour they verify
- keep tests deterministic and readable

Use the repository’s established testing framework (unit tests and integration tests in `scripts/test.integration.sh`).

---

# Pull Request Review Guidelines

When reviewing or proposing improvements, prioritise:

1. correctness
2. safety and side effects (especially economic safety)
3. architectural consistency
4. test coverage
5. maintainability
6. readability

Avoid suggesting stylistic rewrites that do not improve correctness or clarity.

---

# Change Scope Guidelines

Copilot should prefer:

- minimal changes
- edits close to the affected code
- maintaining stable public APIs
- preserving backward compatibility where possible

Avoid:

- large refactors
- renaming widely used symbols
- reorganising directories
- introducing new architectural patterns

---

# File Modification Guidelines

Copilot should be cautious when editing:

- configuration files (`dfx.json`, `Cargo.toml`)
- dependency manifests
- CI pipelines
- infrastructure code
- security-sensitive logic (Canister state, collateral handling)

These areas should only be modified when explicitly required.

---

# References

The following files provide additional repository context:

- `AGENTS.md`
- `docs/ai/governance.md`
- `CLAUDE.md`
- `.agents/architecture.md`
- `.agents/domain_logic.md`
- `.policies/`
- `.boundaries/`
- `README.md`

Copilot should consult them when generating or reviewing code.

---

# Non-Goals

Copilot should not:

- redesign the system architecture
- introduce new frameworks
- modify agent infrastructure
- alter CI/CD pipelines without instruction
- create duplicate utilities when existing ones already exist

---

# Summary

When generating or reviewing code:

- follow the existing architecture
- respect agent responsibilities
- prefer minimal and safe changes
- rely on repository documentation as the source of truth
