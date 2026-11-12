# Workflow: add an endpoint

The most common change in the history (`feat(clearing): add list_series_traded_volumes batch query`, `feat(clearing): add per-window leaderboard standings query`, `feat(registry): add guarded update_series_metadata for editable fields`). Patterns behind each step: [`../patterns.md`](../patterns.md).

## Clearing endpoint

1. **Pick the domain folder** under `src/clearing/src/api/` (`trade`, `settlement`, `account`, `collateral`, `admin`, `leaderboard`, ...). A new domain folder needs the owner's approval.
2. **Define the types** in that folder:
   - `params.rs`: `<Action>Params`.
   - `results.rs`: `<Action>Result` as an `Ok`/`Err` enum with `impl From<Result<T, <Domain>Error>>`, or a plain record for a query that cannot fail.
   - `errors.rs`: new variants on the existing `<Domain>Error`, wrapping `CommonError` for the generic cases.
   - Every type derives `CandidType, Serialize, Deserialize, Clone, Debug` and has doc comments. Before adding a type, check [`../reusability.md`](../reusability.md) (pagination, ids, prices, domains already exist).
3. **Write the pure core** as `<action>_impl` next to the endpoint, or in the domain module (`trade/`, `account/`, `payoffs/`) if another endpoint needs it. It takes the state, the caller and `now` as arguments, validates everything first, then mutates. Unit tests go in the file's `#[cfg(test)] mod tests` (for trade, in `api/trade/tests/`), covering every error variant.
4. **Write the endpoint** in `api.rs`:
   - `#[update(guard = "...")]` or `#[query(guard = "...")]`: every update, and every query that returns caller-specific or restricted data, takes `caller_is_not_anonymous` at least and `caller_is_controller` for administration. Only a query whose whole result is public (as `list_series`, `get_series`, `is_trading_authorized`) goes without a guard. A read is a `query`; a batch read takes a list and a cap, as `list_series_traded_volumes` does.
   - Do every await first (`ensure_series_registered`, `check_trading_access`); if the endpoint mutates a user's state after an await, acquire a `ReassignmentGuard` before the first await and `revalidate()` right before the core.
   - A ledger interaction goes through a durable plan (Plan, Execute, Finalise), never a direct transfer.
   - End with `result.into()`.
5. **Expose it**: import the new params and results in `src/clearing/src/lib.rs`, which `export_candid!()` needs in scope.
6. **Integration test** in `src/clearing/tests/it/<area>.rs`: the happy path through the canister, the unauthorized caller (`assert_unauthorized`), and any cross-canister step.
7. **Regenerate Candid** with `npm run did` as the last step, and commit the `.did` diff.
8. **Docs**: a new shared helper or type goes into [`../reusability.md`](../reusability.md); an endpoint meant for a front end gets a note like [`../open-markets-query.md`](../open-markets-query.md) when its contract needs explaining.
9. **PR**: `feat(clearing): add <endpoint>`, following [`../pr-and-ci.md`](../pr-and-ci.md).

## Registry endpoint

The same steps, with two differences: the request and response types live in `src/shared/src/types/<concept>.rs` (the clearing decodes them too), and the endpoint goes in `src/registry/src/api/<concept>.rs`. List endpoints reuse `PaginationParams::apply` and the `ListSeriesParams` filters. Title: `feat(registry): ...`, or `feat(shared,registry): ...` when the shared types change.
