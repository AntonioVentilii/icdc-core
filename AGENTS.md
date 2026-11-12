# AGENTS.md

The entry point for every coding agent working in `icdc-core` (Claude Code, Cursor, Copilot, Codex and the rest). Tool files (`CLAUDE.md`, `.cursor/rules/`, `.github/copilot-instructions.md`) only point here. It is short on purpose: everything deeper lives in [`docs/ai/`](docs/ai/README.md).

## What this repo is

ICDC (Internet Computer Derivatives Clearing) is a multi-canister clearing house on the Internet Computer. It acts as the Central Counterparty (CCP) for derivative series: it holds collateral, keeps positions internal (never tokenised), computes margin and settles every series in an internal USD unit (`vUSD`). Economic safety and atomicity matter more than anything else here. Background: [`README.md`](README.md), [`NEW_ARCHITECTURE.md`](NEW_ARCHITECTURE.md).

A Rust workspace ([`Cargo.toml`](Cargo.toml)) of four crates, three of them canisters ([`dfx.json`](dfx.json)):

| Crate                           | What it owns                                                                                         |
| ------------------------------- | ---------------------------------------------------------------------------------------------------- |
| [`src/clearing`](src/clearing/) | The economic core: balances, positions, margin, trading, settlement, collateral custody, leaderboard |
| [`src/registry`](src/registry/) | Series definitions, oracles, groups (closed circles), engines and their roles                        |
| [`src/minter`](src/minter/)     | Mints `vUSD` on its ledger for authorised callers (the clearing)                                     |
| [`src/shared`](src/shared/)     | Types, constants and migration rules used by more than one canister                                  |

Integration tests live in [`src/clearing/tests/it/`](src/clearing/tests/it/) and run on PocketIC. Shell automation (build, deploy, init, lint) lives in [`scripts/`](scripts/) behind the npm scripts in [`package.json`](package.json).

## Commands

```bash
npm run quality            # format + lint everything (prettier, rustfmt, clippy, shfmt, the dash rule)
npm run test:unit          # cargo test --lib --bins with -D warnings
scripts/pic-install        # once per machine, before the integration tests
npm run build              # build the canister wasms the integration tests load
npm run test:integration   # cargo test --test it (PocketIC)
npm run did                # regenerate the .did files after an API change; run it last
```

The full list of gates and the CI jobs that enforce them: [`docs/ai/pr-and-ci.md`](docs/ai/pr-and-ci.md).

## The standard

Every change follows the universal code standard: **[officina `standards/code.md`](https://github.com/AntonioVentilii/officina/blob/main/standards/code.md)**. It is the only copy of those rules; this repository never restates them. Cite a rule by number and name ("rule 5, one value, one place").

What `icdc-core` adds on top of it:

1. **Economic safety first.** No change may compromise the financial integrity of the clearing: conservation of value, solvency (`equity_usd >= reserved_margin_usd`), no value created or lost on a failed path. See [`docs/ai/domain.md`](docs/ai/domain.md).
2. **Validate, then mutate.** Every operation that touches balances, positions or margin validates and computes everything first, then mutates. Never `await` between the two if that can leave the state inconsistent. Ledger interactions use a durable plan (Plan, Execute, Finalise). See [`docs/ai/patterns.md`](docs/ai/patterns.md).
3. **Thin endpoints.** A `#[update]` or `#[query]` function reads the caller and the global state, then calls a pure `*_impl` or `*_logic` function (or a domain service) that receives its dependencies as arguments. Logic does not grow inside the endpoint.
4. **Persisted types change with a migration.** Adding a non-`opt` field, renaming or retyping a field of anything saved in `pre_upgrade` traps the next upgrade unless a migration ships with it. See [`docs/ai/migrations.md`](docs/ai/migrations.md).
5. **The `.did` files are generated.** Change the Rust API, then run `npm run did`; CI fails when the committed Candid does not match.
6. **Venue neutral.** The clearing never makes a tactical choice for one consumer (such as Vici); it accepts valid fills and manages collateral for any venue.
7. **Clippy is pedantic and denies warnings.** Fix the code, never add an `allow`; the rare justified exception is an `#[expect(lint, reason = "...")]` (the workspace denies `allow_attributes`).
8. **Plan before large or sensitive changes.** A change over about 50 lines, or one touching architecture, authorization, custody or settlement, starts with a written plan the owner approves. When proposing an architectural change, list the pros and cons.

## Where to look

| You are about to...                                    | Read first                                                                                                                         |
| ------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------- |
| Open any PR                                            | [`docs/ai/pr-and-ci.md`](docs/ai/pr-and-ci.md)                                                                                     |
| Add a file, a module or a crate                        | [`docs/ai/structure.md`](docs/ai/structure.md)                                                                                     |
| Write any Rust (endpoints, state, errors, async)       | [`docs/ai/patterns.md`](docs/ai/patterns.md)                                                                                       |
| Add a type, helper, constant, guard or test fixture    | [`docs/ai/reusability.md`](docs/ai/reusability.md)                                                                                 |
| Touch balances, margin, settlement, vUSD, withdrawals  | [`docs/ai/domain.md`](docs/ai/domain.md)                                                                                           |
| Add an endpoint (update or query)                      | [`docs/ai/workflows/new-endpoint.md`](docs/ai/workflows/new-endpoint.md)                                                           |
| Add or change a field on `Series` or any saved type    | [`docs/ai/workflows/add-series-field.md`](docs/ai/workflows/add-series-field.md), [`docs/ai/migrations.md`](docs/ai/migrations.md) |
| Fix a bug                                              | [`docs/ai/workflows/fix-a-bug.md`](docs/ai/workflows/fix-a-bug.md)                                                                 |
| Add or change tests                                    | [`docs/ai/testing.md`](docs/ai/testing.md)                                                                                         |
| Touch guards, CI, `scripts/`, `dfx.json`, `Cargo.toml` | [`docs/ai/governance.md`](docs/ai/governance.md)                                                                                   |
| Touch balance domains, closed circles or engines       | [`docs/architecture/`](docs/architecture/)                                                                                         |
| Release or deploy                                      | [`docs/ci-cd/release-deploy.md`](docs/ci-cd/release-deploy.md)                                                                     |

## Truth hierarchy

When two sources disagree, the higher one wins:

1. The code on `main` (`src/**`, `scripts/**`).
2. CI (`.github/workflows/`): the checks that must pass.
3. [`.github/CODEOWNERS`](.github/CODEOWNERS): who reviews what.
4. [`docs/ai/**`](docs/ai/README.md).
5. This file and the [universal standard](https://github.com/AntonioVentilii/officina/blob/main/standards/code.md).
6. Tool layers: `CLAUDE.md`, `.cursor/rules/`, `.github/copilot-instructions.md`.

Explicit instructions from the owner in the conversation override all of them for that task. A contradiction is a bug in the lower source: do not pick a side silently, stop and ask, or fix the lower source in its own change.

## The meta-update rule

A PR that adds a pattern, a shared piece of code, a naming convention, a policy or a workflow updates the matching `docs/ai/**` page in the same PR, and says so under `# Changes`. [`docs/ai/governance.md`](docs/ai/governance.md#meta-update-map) maps each kind of change to its page. Docs that no longer match the code are a bug.
