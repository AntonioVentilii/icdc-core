# Structure

Where code lives in `icdc-core`, where each kind of new code goes, and which way dependencies point. The system design behind it is in [`NEW_ARCHITECTURE.md`](../../NEW_ARCHITECTURE.md) and [`docs/architecture/`](../architecture/); this page only maps it onto folders.

## Crates and canisters

```text
src/
├── shared/      library: types, constants, guards, migration rules used by more than one canister
├── registry/    canister: series, oracles, groups, engines
├── clearing/    canister: the economic core (and the only integration test suite)
└── minter/      canister: mints vUSD for authorised callers
```

Each canister crate depends on `shared` and on nothing else in the workspace. Canisters talk to each other only through bounded inter-canister calls (`ic_cdk::call::Call::bounded_wait`): the clearing calls the registry (`get_series`, `is_trading_authorized`, `is_oracle_authorized`), the ICRC ledgers and the EVM RPC canister. Never add a crate dependency from one canister to another; a type both need moves to `shared`.

Responsibilities, as the design fixes them:

- **Registry**: canonical definitions of products and series (payoff, oracle source, expiry, trading access). No balances, no positions.
- **Clearing**: the single source of truth for value: balances per balance domain, positions, margin, equity, settlement, deposits and withdrawals. Balance domains (`Settlement`, `Playground`, branded ones) keep funds and positions apart: [`docs/architecture/balance-domains.md`](../architecture/balance-domains.md).
- **Execution and venues**: order matching beyond the clearing's own limit book belongs to a future execution layer or to the venue, which must check margin with the clearing before accepting an order.
- **Sharding** by `(underlying_id, expiry_month)` with a router canister is a plan, not code yet: [`SHARDING_PLAN.md`](../../SHARDING_PLAN.md). Do not change that strategy without the owner.

## Inside the clearing (`src/clearing/src/`)

```text
lib.rs          init / pre_upgrade / post_upgrade, the imports export_candid! needs; no logic
api/            one folder per domain: account, admin, collateral, general, leaderboard,
                metrics, migration, settlement, trade
  <domain>/
    api.rs      the #[update] / #[query] endpoints and, today, most of the domain logic
    params.rs   <Action>Params
    results.rs  <Action>Result (Ok/Err enums), response records
    errors.rs   <Domain>Error
    tests/      (trade only) in-crate unit tests split by scenario
account/        AccountService (account-state response) and reassignment (re-keying a user)
trade/          execute_trade_impl and its params: the core trade mutation
payoffs/        payoff, settlement value and required margin per PayoffType; fees
assets/         AssetHandler and the ICRC / EVM handlers that talk to ledgers
types/          clearing-only domain types: margin, plans, events, leaderboard, user, state
utils/          small helpers: subaccount derivation, registry calls, series gates, time, vUSD
memory.rs       every thread_local state map, save_state / restore_state, derived indexes
migrations/     quarantined upgrade migration for the clearing's saved state
guards.rs       endpoint guards
traits.rs       ClearingAccountExt (subaccount and ICRC account of a User)
```

The registry is flatter: `api/{access,engines,groups,oracles,series}.rs` hold endpoints and logic, `guards.rs`, `memory.rs`, `migrations/`, `utils.rs`. Its request and response types live in `src/shared/src/types/` because the clearing decodes them too. The minter is `lib.rs`, `guards.rs`, `state/`, `utils.rs`.

## Where new code goes

| New code                                               | Goes in                                                                                                                            |
| ------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------- |
| A type, constant or rule used by two or more canisters | `src/shared/src/types/<concept>.rs` (re-exported from `types/mod.rs`), `src/shared/src/constants.rs`                               |
| A clearing endpoint                                    | `src/clearing/src/api/<domain>/api.rs`, with its types in that folder's `params.rs`, `results.rs`, `errors.rs`                     |
| A registry endpoint                                    | `src/registry/src/api/<concept>.rs`, its types in `src/shared/src/types/`                                                          |
| Clearing logic bigger than a few lines                 | a pure function next to the endpoint, or the domain module (`trade/`, `account/`, `payoffs/`) when more than one endpoint needs it |
| A new payoff type                                      | `PayoffType` in `src/shared/src/types/series.rs`, the maths in `src/clearing/src/payoffs/`                                         |
| A new collateral network                               | a handler under `src/clearing/src/assets/<network>/`, wired into `AssetHandler`                                                    |
| Persisted state                                        | the canister's `memory.rs`: the map, its line in `save_state` and `restore_state`                                                  |
| An upgrade migration                                   | `src/<crate>/src/migrations/`, the canister-agnostic rule in `src/shared/src/migrations/`                                          |
| An endpoint guard                                      | the canister's `guards.rs`; one that every canister needs goes in `src/shared/src/guards.rs` and is re-exported                    |
| An integration test                                    | `src/clearing/tests/it/<area>.rs`, registered in `main.rs`                                                                         |
| A shell helper                                         | `scripts/utils.sh` or `scripts/init.market.common.sh`; config in `scripts/init.common.sh`                                          |
| A design decision                                      | `docs/adr/NNNN-<title>.md`; feature notes in `docs/ai/` or `docs/architecture/`                                                    |

No new top-level folder, crate or `api/` domain without asking.

## Dependency direction

Inside a canister crate: `lib.rs` → `api/` → domain modules (`trade/`, `account/`, `payoffs/`, `assets/`) → `types/`, `utils/`, `memory.rs` → `shared`. Lower layers must not import from `api/`. Today some do, and new code must not add to it: `utils/series.rs` and `trade/service.rs` return `api::trade::errors::TradeError`, and `types/plans.rs` imports `api::admin::params::FundType`. An error or type a lower layer needs belongs in `types/`.

Migration code is quarantined: only `restore_state` calls it, and nothing in `api/` imports it. The runtime domain migration (moving a user between balance domains) is a product feature in `api/migration/`, not upgrade plumbing.
