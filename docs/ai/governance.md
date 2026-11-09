# Governance

What an agent may touch in `icdc-core`, what it must ask about first, and which page to update for which kind of change. The universal rules are in the [standard](https://github.com/AntonioVentilii/officina/blob/main/standards/code.md); the truth hierarchy is in [`AGENTS.md`](../../AGENTS.md#truth-hierarchy).

## Generated paths: never edit by hand

| Path                                                                              | Produced by                                                                   |
| --------------------------------------------------------------------------------- | ----------------------------------------------------------------------------- |
| `src/clearing/clearing.did`, `src/registry/registry.did`, `src/minter/minter.did` | `npm run did` (`scripts/did.sh`); `scripts/lint.did.sh` fails CI on any drift |
| `CHANGELOG.md`, `.release-please-manifest.json`                                   | release-please, on the `chore(release): vX.Y.Z` PR                            |
| The `version` in `Cargo.toml`, `Cargo.lock`, `package.json`, `package-lock.json`  | the release workflow, on the same PR                                          |
| `Cargo.lock` (otherwise), `package-lock.json` (otherwise)                         | `cargo` and `npm` when a dependency changes                                   |
| `**/declarations/`, `target/`, `.dfx/`                                            | the build (git-ignored)                                                       |

## Protected paths: ask before changing

Change these only when the task explicitly asks for it, and name the change in the plan and in the PR body.

- `.github/workflows/`, `.github/actions/`, `.github/dependabot.yml`: CI, release and deploy.
- `dfx.json`, `canister_ids.json`: canister configuration and the deployed IDs.
- Root `Cargo.toml` (workspace members, dependencies, lints), `clippy.toml`, `rustfmt.toml`, `rust-toolchain.toml`.
- `package.json`, `dev-tools.json`.
- `scripts/`: build, deploy, init and lint automation. `scripts/init.common.sh` holds the shared init configuration (token IDs, decimals, prices).
- `AGENTS.md`, `CLAUDE.md` and this page: any change to the governance rules needs the owner's explicit review.

## Sensitive code: the deepest review

These modules move or guard value. Change them only on explicit instruction, explain the why in the PR body, and expect a second review.

- Authorization: `src/*/src/guards.rs`, engine roles in `src/registry/src/api/engines.rs`, group membership and `is_trading_authorized` in `src/registry/src/api/groups.rs`, `is_oracle_authorized` in `src/registry/src/api/oracles.rs`.
- Custody and ledgers: `src/clearing/src/assets/`, `src/clearing/src/api/collateral/`, fund withdrawals in `src/clearing/src/api/admin/`, `src/clearing/src/account/reassignment.rs`, `src/minter/`.
- Margin and settlement: `src/clearing/src/types/margin.rs`, `src/clearing/src/payoffs/`, `src/clearing/src/api/settlement/`, `src/clearing/src/trade/service.rs`.
- Persisted state: `src/*/src/memory.rs`, `src/*/src/migrations/`, `src/shared/src/migrations/`.

## What an agent may do

Without asking:

- Change functions and types in `src/`, add modules and files that follow [`patterns.md`](patterns.md) and [`structure.md`](structure.md).
- Refactor small or medium modules for clarity, with no behaviour change.
- Add and fix tests.
- Update `docs/ai/**`, `README.md` and the crate READMEs to match the code.

Only after the owner approves:

- Anything in [Protected paths](#protected-paths-ask-before-changing) or [Sensitive code](#sensitive-code-the-deepest-review).
- A change over about 50 lines, or one touching architecture: write the plan first (what changes, how it is verified), get it approved, then code.
- A new external crate or npm package. It is reviewed for security, maintenance and size, and declared once in the root `Cargo.toml` `[workspace.dependencies]`: member crates take it with `workspace = true` (`scripts/lint.cargo-workspace-dependencies.sh` enforces it).
- Changing the atomicity model, the balance-domain model or the sharding strategy in [`SHARDING_PLAN.md`](../../SHARDING_PLAN.md).
- A new framework, a new crate, or a new top-level folder.

Never:

- Commit secrets, private keys, PEM files or identities.
- Relax a guard or change who may call an endpoint as a side effect of another change.
- Delete a large section of documentation without moving its content somewhere else.
- Merge, force-push or rewrite pushed history (see [`pr-and-ci.md`](pr-and-ci.md#8-updating-an-existing-pr)).

## Risk levels

| Risk   | Examples                                                                                                  | What it needs                                       |
| ------ | --------------------------------------------------------------------------------------------------------- | --------------------------------------------------- |
| Low    | Docs, comments that explain why, tests, a small bug fix, a refactor with no behaviour change              | The normal gates                                    |
| Medium | Business logic, a public endpoint's behaviour, persisted state, a dependency bump                         | Tests for the new behaviour, the Candid regenerated |
| High   | Architecture, CI and deploy, authorization, custody, settlement, margin, deleting large parts of the code | An approved plan first; only on explicit request    |

## Security rules

- Every endpoint has a guard: `caller_is_not_anonymous` at least, `caller_is_controller` or a role check for anything administrative (see [`patterns.md`](patterns.md#guards)).
- Validate every principal and every amount at the boundary, and follow the Internet Computer canister security practices (reentrancy across `await`, bounded inter-canister calls, trap-safe callbacks).
- Value moves only through the Plan, Execute, Finalise flow, never by a direct transfer from an endpoint.

## Plan, implement, review

Non-trivial work goes through three steps, whether one agent or several do it:

1. **Plan.** Break the request into atomic tasks, check them against [`structure.md`](structure.md), [`domain.md`](domain.md) and [`SHARDING_PLAN.md`](../../SHARDING_PLAN.md), and write the plan with its verification steps. The owner approves it.
2. **Implement.** Follow the plan and [`patterns.md`](patterns.md) with targeted edits rather than rewrites; every new behaviour gets its tests; `npm run quality` passes.
3. **Review.** Check the change against [`domain.md`](domain.md) (atomicity, conservation of value, solvency) and the sensitive list above, confirm the tests cover the failure paths, and confirm the docs moved with the code. Reject a change that breaks atomicity. The PR body records what was done and how it was verified.

## Meta-update map

A PR that changes one of these updates the page in the same PR (rule 13 of the standard).

| When the change...                                                | Update                                                                                      |
| ----------------------------------------------------------------- | ------------------------------------------------------------------------------------------- |
| Adds or moves a module, a crate or a folder                       | [`structure.md`](structure.md)                                                              |
| Introduces, replaces or retires a coding pattern                  | [`patterns.md`](patterns.md)                                                                |
| Adds a shared type, helper, constant, guard or test fixture       | [`reusability.md`](reusability.md)                                                          |
| Changes margin, settlement, vUSD, custody or withdrawal semantics | [`domain.md`](domain.md), and the matching page in [`docs/architecture/`](../architecture/) |
| Changes how persisted state migrates                              | [`migrations.md`](migrations.md)                                                            |
| Changes how tests are written or run                              | [`testing.md`](testing.md)                                                                  |
| Changes a gate, a CI job, the PR template or the title rule       | [`pr-and-ci.md`](pr-and-ci.md)                                                              |
| Changes a step of a recurring change                              | the recipe in [`workflows/`](workflows/)                                                    |
| Changes a protected path, a permission or this map                | this page, with the owner's review                                                          |
| Changes the commands, the crates or the entry table               | [`AGENTS.md`](../../AGENTS.md)                                                              |
| Changes release or deploy                                         | [`docs/ci-cd/release-deploy.md`](../ci-cd/release-deploy.md)                                |

Every link stays relative and every file it names exists: check them before opening the PR.
