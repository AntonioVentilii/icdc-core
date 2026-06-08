# AI Guidance: Implementation Patterns

This document captures the preferred coding patterns and refactoring strategies used in the ICDC repository.

## 1. The Domain Service Pattern

Move business logic out of the `api/` (canister interface) layer into a dedicated `Service` module within the domain folder.

- **Structure**:
  ```text
  src/clearing/src/
  ├── account/
  │   ├── mod.rs
  │   └── service.rs  <-- All complex logic here
  └── api/
      └── account/
          ├── params.rs
          ├── results.rs
          └── mod.rs  <-- Minimal bridge calling Service
  ```
- **Benefits**: Easier testing, cleaner canister entry points, and better separation of concerns.

## 2. Parameter, Result, and Error Objects

Every API endpoint should have its own `Params`, `Result`/`Response`, and potentially `Error` objects defined in the `api/` subfolders.

- **Naming**: `[Action]Params`, `[Action]Result`/`Response`, and `[Domain]Error`.
- **Derives**: Must derive `CandidType`, `Serialize`, `Deserialize`, `Clone`, and `Debug`.

## 3. Atomic State Mutation (Internal Logic)

When writing internal execution logic (like `execute_trade_impl`), pass all dependencies (configs, metrics) explicitly rather than fetching them from global state inside the function. This makes the logic testable in isolation.

## 4. Centralized Configuration

- **Initialization Scripts**: Maintain shared configuration (token IDs, decimals, prices) in `scripts/init.common.sh`.
- **Canister Guards**: Use guard functions (in `src/clearing/src/guards.rs`) to centralize authorization logic.

## 5. Stable-State Migrations

Canister state is persisted with candid, and **candid decoding fails on a
missing non-optional field**. Adding a compulsory field (or renaming/retyping
one) to a persisted type therefore requires a migration — otherwise the next
upgrade traps in `post_upgrade`.

- **Quarantine migration code** in a dedicated `migrations/` module, never mixed
  into live `api/` domains: canister-agnostic rules in `src/shared/src/migrations/`,
  per-canister wiring in `src/<crate>/src/migrations/` (called from `restore_state`).
- **Decode-old → transform → store-new**: mirror the historical type with a
  `Legacy*` shadow struct, try the current schema first, then fall back to the
  legacy shadow and backfill.
- **Document retirement**: each module header states what it bridges and when it
  can be deleted (once no pre-migration blob can exist).
- Adding an `opt` field needs no migration. See
  [docs/ai/migrations.md](file:///Users/antonio.ventilii/projects/icdc-core/docs/ai/migrations.md)
  for the full convention and the `Series::resolution` reference implementation.

> Note: this is distinct from the runtime **domain migration** feature in
> `src/clearing/src/api/migration/` (moving users between balance domains), which
> is a permanent product API, not upgrade plumbing.

## 6. Tooling & Quality Shortcuts

Use the following `npm` scripts to maintain repository standards:

- `npm run quality`: All-in-one check for formatting and linting across the entire project.
- `npm run quality:rust`: Focused check for Rust files (format + clippy).
- `npm run did`: Handles candid generation and ensures the whole project is cleaned up afterwards.
- `npm run test`: Runs unit tests.

> [!TIP]
> Always run `npm run quality` before requesting a review to avoid basic linting failures.
