# AI Guidance: General Instructions

This document provides the core instructions for AI agents working on the ICDC Core repository. Following these rules is mandatory to maintain code quality, consistency, and economic safety.

## 1. Coding Standards

- **Rust Idioms**: Write clean, idiomatic Rust. Use `clippy` and `rustfmt`.
- **No Code Smells**: Avoid large functions, deep nesting, and commented-out code.
- **DRY (Don't Repeat Yourself)**: Abstract common logic into the `shared` crate or internal utility modules.
- **Coherent Imports**:
  - Group imports logically: standard library, external crates, internal modules.
  - Use `crate::` for internal lookups.
  - Prefer explicit imports over `*`.
- **Lint & Format Compliance**: Code MUST be compliant with all project-specific linting and formatting rules.
  - Run `npm run quality` to check and fix all files (Rust, Shell, Prettier).
  - Run `npm run did` after changing API endpoints to ensure correct Candid generation and compliance.
  - Use `./scripts/format.sh` and `./scripts/lint.sh` for targeted checks.

## 2. Testing Policy

- **Always Create Tests**: Every new feature or bug fix must include tests.
- **Unit Tests**: Place in a `tests` module at the bottom of the file.
- **Integration Tests**: Focus on end-to-end flows in `scripts/` or dedicated integration test files.
- **Economic Safety**: Specifically test edge cases in margin calculations, settlement, and asset transfers. Use property-based testing principles where applicable.

## 3. Canister Placement & Structure

- **Shared**: Place types, constants, and utilities used by more than one canister in `src/shared`.
- **Domain Logic**: Move core logic out of the API layer into a dedicated domain service (e.g., `src/clearing/src/account/service.rs`).
- **Canister Separation**:
  - **Registry**: Metadata for products and series.
  - **Clearing**: Economic truth, balances, and positions.
  - **Execution (Future)**: High-frequency matching and order books.

## 4. Network Awareness

- **Multi-Network Design**: Design logic to be network-agnostic where possible.
- **Handler Pattern**: Use handlers to encapsulate network-specific properties (e.g., block times, fee structures).
- **Priority**: Internet Computer (IC) is the primary target unless otherwise specified.

## 5. Meta-Update Rule

> [!IMPORTANT]
> If a task introduces a fundamental change to the architecture, a new coding pattern, or a critical workflow, you MUST update the relevant guidance in `.agents/` as part of your submission, following the [.policies/ai-meta-updates.md](file:///Users/antonio.ventilii/projects/icdc-core/.policies/ai-meta-updates.md) and [.workflows/meta-update.md](file:///Users/antonio.ventilii/projects/icdc-core/.workflows/meta-update.md).
