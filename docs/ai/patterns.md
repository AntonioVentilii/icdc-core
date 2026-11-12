# Patterns

The patterns `icdc-core` really uses, each with an example to copy, and the anti-patterns found in the code, named so nobody copies them. Where the code holds two styles, the one on this page wins. The economic rules behind them are in [`domain.md`](domain.md).

## Endpoints

### Thin endpoint, pure core

An endpoint reads the caller and the global state, does the awaits it needs, then hands everything to a synchronous function that takes its dependencies as arguments. The core can be unit-tested without canister state or a replica.

- `aggregate_lean` passes the positions map into `aggregate_lean_impl`: `src/clearing/src/api/account/api.rs:192`, core at `:198`.
- `add_series` reads caller and time, then calls `add_series_impl`: `src/registry/src/api/series.rs:56`, core at `:82`.
- `internal_execute_trade` awaits the series, then calls the synchronous `execute_trade_impl(&series, params)`: `src/clearing/src/trade/service.rs:25`, core at `:39`.

Name the core `<endpoint>_impl` (or `<endpoint>_logic` where the file already does, as `mint_complete_set_logic` in `src/clearing/src/api/trade/api.rs:1016`). A core used by more than one endpoint moves to the domain module (`trade/`, `account/`, `payoffs/`); a domain service struct (`AccountService` in `src/clearing/src/account/service.rs:14`) exists only where it already does.

### Params, Result and Error types

Each endpoint has its own types in the domain folder: `<Action>Params` in `params.rs`, `<Action>Result` in `results.rs`, `<Domain>Error` in `errors.rs`. Every one derives `CandidType, Serialize, Deserialize, Clone, Debug`.

A result is a Candid-friendly `Ok`/`Err` enum built from a Rust `Result` through `From`, so the endpoint body works with `?` and ends with `result.into()`: `SubmitMatchedTradeResult` in `src/clearing/src/api/trade/results.rs:9`, used at `src/clearing/src/api/trade/api.rs:193`.

### Guards

Authorization is declared on the endpoint, never checked ad hoc inside it (public discovery queries such as `list_series`, `src/clearing/src/api/general/api.rs:9`, carry no guard on purpose): `#[update(guard = "caller_is_not_anonymous")]` (`src/clearing/src/api/trade/api.rs:160`) or `guard = "caller_is_controller"` (`:365`). The guards live in each canister's `guards.rs` (`src/clearing/src/guards.rs:5`). Role checks that need arguments (engine roles, group membership) are functions such as `has_engine_role_on` in the registry's `guards.rs`, called first thing in the endpoint.

## Errors

- Errors are typed enums with a doc comment per variant. Cross-cutting cases are `CommonError` (`src/clearing/src/types/errors.rs:6`); a domain error wraps it as a variant (`TradeError::Common`, `src/clearing/src/api/trade/errors.rs:14`) rather than repeating `Unauthorized` or `MathOverflow`.
- Variants carry the data the caller needs (`TradeError::SeriesNotStarted { series_id, start_ns }`), never a formatted string when a field would do. `Internal(String)` is for transport and decode failures only.
- No `unwrap()`, `expect()`, `panic!` or `unreachable!` outside tests: return the error. Arithmetic on amounts uses checked operations and maps overflow to `CommonError::MathOverflow`.
- Prefer an enum to a boolean flag that switches behaviour: `SeriesAccess::{OpenExposure, ReduceExposure}` (`src/clearing/src/utils/series.rs:20`) makes every call site state its intent.

## State

- Each canister keeps its state in `thread_local!` `RefCell` maps declared once in `memory.rs` (`src/clearing/src/memory.rs:49`). Access is `MAP.with(|m| m.borrow())`; keep the borrow inside the closure and never hold it across an `await`.
- Upgrades serialise the whole state with Candid: `save_state` in `pre_upgrade` (`src/clearing/src/memory.rs:140`), `restore_state` in `post_upgrade` (`:203`). A new map is added to both. A derived index is not saved: it is rebuilt from its source in `restore_state` (`rebuild_series_trade_history`, `:341`).
- A persisted type changes only with a migration: [`migrations.md`](migrations.md).
- Pass the clock in: pure cores take `now: u64`; endpoints get it from `now_ns()` (`src/clearing/src/utils/system.rs:9`), which returns `0` off-chain so host tests are deterministic.

## Async safety

### Validate, then mutate

Do every await first (registry lookups, authorization), then validate and compute, then mutate, with no await in between. `submit_market_order` (`src/clearing/src/api/trade/api.rs:161`) awaits the series and the trading access, then calls the synchronous `submit_market_order_impl` (`:196`), which validates and mutates in one message.

### Plan, Execute, Finalise

Every ledger interaction runs through a durable plan stored in state, so a retry, a lost response or an upgrade resumes instead of paying twice:

1. **Plan** (no awaits): `DepositPlan::get_or_create` keyed by the caller's idempotency ID (`src/clearing/src/types/plans.rs:108`); return early if it is `Finalised`.
2. **Execute** (async, resumable): mark `Executing`, do the transfer, store the receipt.
3. **Finalise** (no awaits, idempotent): apply the accounting once and mark `Finalised`.

Reference implementation: `deposit_collateral` in `src/clearing/src/api/collateral/api.rs:82` (Phase A), `:96` (Phase B), `:135` (Phase C). Settlement, withdrawals, fund withdrawals, domain migration and reassignment use the same `PlanStatus` (`src/clearing/src/types/plans.rs:21`). Flows: [`docs/architecture/flows.md`](../architecture/flows.md).

### Inter-canister calls

Use `Call::bounded_wait(canister, "method").with_args(...)`, map transport and decode errors to a typed error, decode with `candid_tuple`: `src/clearing/src/utils/registry.rs:13`. Reach for an existing wrapper in `utils/registry.rs` or the asset handlers before writing another call.

### Moving user state between principals

Anything that re-keys a user's state (today account reassignment, `src/clearing/src/account/reassignment.rs`) survives two hazards that durable plans alone do not cover:

- **In-flight user calls.** A call that passed its guard and is suspended at an await resumes after the re-key and would recreate state under the old principal. Every post-await mutation is guarded by a `ReassignmentGuard`: acquire it before the first await, revalidate it right before the mutation (`ReassignmentGuard::acquire`, `src/clearing/src/account/reassignment.rs:87`; `revalidate`, `:102`; in use at `src/clearing/src/api/trade/api.rs:181` and `:187`). It rejects a principal locked by a pending move, and one whose generation changed while the call was suspended.
- **On-ledger custody.** Internal balances are only half the account: collateral lives in a subaccount derived from the principal, so a re-key that does not drain it strands the funds. Re-key first, synchronously, in the same message prefix as the validation, then sweep each asset by re-reading the ledger balance and moving `balance - fee`. Deriving the amount from the ledger, never from internal accounting, is what makes a retry idempotent.

## Assets and networks

Keep network specifics behind a handler. `get_handler` picks the ICRC or EVM handler for an `Asset` (`src/clearing/src/assets/asset/handler.rs:57`) and callers only see `balance_of`, `transfer`, `transfer_from`, `get_fee`. A new network is a new handler, not a `match` in an endpoint. Design for the Internet Computer first and keep the logic network-agnostic.

## Code style

- `snake_case` for functions and variables, descriptive names, no abbreviations the domain does not use.
- Prefer standard library types, and no `unsafe` unless the owner agrees it is necessary.
- Imports in three groups (`core`/`std`, external crates, `crate::`), explicit names, no glob imports except the existing `pub use` re-exports in `mod.rs` files. Clippy's `absolute_paths` and `std_instead_of_core` are denied, so import instead of writing long paths, and take from `core` what lives there.
- Doc comments on every public item; for endpoints and pure cores, say what each argument means and what each error variant signals. Comments explain why, never what.
- Shared init configuration for scripts (token IDs, decimals, prices) lives in `scripts/init.common.sh`, nowhere else.

## Anti-patterns found in the code

Do not copy these; fix one only in its own PR.

- **Logic piled into `api.rs`.** `src/clearing/src/api/trade/api.rs` (about 1,200 lines before its tests), `src/clearing/src/api/settlement/api.rs` (about 850), `src/clearing/src/api/leaderboard/api.rs`, and `src/registry/src/api/series.rs` (650 lines) mix endpoints, validation and domain logic. New logic goes into a pure core or the domain module, not into these files.
- **God type file.** `src/shared/src/types/series.rs` (about 975 lines before its tests) holds the `Series` type, ids, params, results, errors, pagination and filters. New series-related types get their own module under `src/shared/src/types/`.
- **Guards copied per canister.** `caller_is_not_anonymous` and `caller_is_controller` are the same code in `src/clearing/src/guards.rs:5`, `src/registry/src/guards.rs:8` and `src/minter/src/guards.rs:7`, including a hand-written "Update call error. RejectionCode..." string. `has_engine_role` and `has_engine_role_on` (`src/registry/src/guards.rs:30`, `:45`) repeat the same predicate.
- **Registry call boilerplate.** `is_oracle_authorized` and `is_trading_authorized` (`src/clearing/src/utils/registry.rs:13`, `:48`) repeat the call-and-decode body, and `ensure_series_registered` inlines a third copy with a different error type (`src/clearing/src/utils/series.rs:81`).
- **Hand-written `From<Result>` for every result enum** (25 of them, such as `src/clearing/src/api/trade/results.rs:15`).
- **Panics in production paths.** `unwrap()` at `src/registry/src/api/engines.rs:107`, `src/clearing/src/api/trade/api.rs:336`, `src/clearing/src/api/admin/api.rs:207`, `src/clearing/src/api/collateral/api.rs:332`; `expect` at `src/clearing/src/types/leaderboard.rs:135`; `unreachable!()` at `src/clearing/src/payoffs/mod.rs:124`; `trap` on bad input in `canonical_id_part` (`src/registry/src/utils.rs:14`). The `expect` on `stable_save` and `stable_restore` in `pre_upgrade`/`post_upgrade` (`src/clearing/src/memory.rs:200`, `src/minter/src/lib.rs:30`) is the one accepted case: trapping there is the intended rollback.
- **Dead code kept alive** with `#[expect(dead_code)]`: `get_internal_ledger_id` (which also panics, `src/clearing/src/utils/vusd.rs:20`), `src/clearing/src/types/account.rs:39`, `src/clearing/src/assets/evm/client.rs:50` and `:128`.
- **Lower layers importing `api/`.** See [`structure.md`](structure.md#dependency-direction).
