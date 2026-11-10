# Testing

Where tests live in `icdc-core`, how they are written and how to run them. Every new feature and every bug fix comes with tests, and existing tests pass before and after the change. The invariants they protect are in [`domain.md`](domain.md#6-invariants).

## Two layers

| Layer       | Where                                                                                                                                           | Runs with                                           |
| ----------- | ----------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------- |
| Unit        | a `#[cfg(test)] mod tests` at the bottom of the file under test; the trade domain splits its scenarios into `src/clearing/src/api/trade/tests/` | `npm run test:unit` (`cargo test --lib --bins`)     |
| Integration | `src/clearing/tests/it/<area>.rs`, one module per area, registered in `src/clearing/tests/it/main.rs`; the canisters run on PocketIC            | `npm run test:integration` (`cargo test --test it`) |

There is no dfx-based test suite: integration tests deploy the real wasms (clearing, registry, minter, ledgers) into PocketIC.

## Unit tests

- Test the pure core (`*_impl`, `*_logic`, payoff and margin functions) with state built in the test. Reset the `thread_local` maps you touch and clean up after the test, as `assert_series_not_settled_rejects_series_with_plan` does in `src/clearing/src/utils/series.rs:192`.
- Use the trade helpers in `src/clearing/src/api/trade/tests/utils.rs` (`setup_test_state`, `execute_trade_checked`, `settle_series_checked`, `verify_cash_balance`, `verify_reserved_margin`, `verify_position_qty`, `create_test_user`) instead of building accounts by hand.
- Mandatory coverage for any change to validation or accounting:
  - every validation failure path (for example `InsufficientMargin`);
  - the state after a mutation (equity, cash balance, reserved margin, positions);
  - decimal scaling, especially with assets of mixed decimals;
  - the edge cases of margin, settlement and transfers. Use property-style loops over inputs where they fit, as `settlement_exhaustive.rs` does.
- `now_ns()` returns `0` off-chain, so pass explicit `now` values to time-dependent cores.

## Integration tests

- Start from a fixture in `src/clearing/tests/it/utils/test_environment.rs`: `TestSetup::default()` (no collateral ledgers), `TestSetup::with_icp()`, `TestSetup::with_all_ledgers()`. Users come from `test_user(id)`.
- Call canisters through `PicCanisterTrait` (`update`, `query`, `src/clearing/tests/it/utils/pic_canister.rs:54`) and set up markets and collateral through `TradeHelperTrait` (`src/clearing/tests/it/utils/trade_helper.rs:55`).
- Assert with `src/clearing/tests/it/utils/assertions.rs` (`assert_ok_value`, `assert_unauthorized`, `assert_err_msg`, `assert_decimal_eq`); take principals and asset IDs from `utils/constants.rs` and `utils/mock.rs`.
- Cover the end-to-end flow a change touches: deposit, trade, settlement, withdrawal. Check final balances against expected values, and that a failed step changed nothing (`atomicity.rs`).
- Significant changes to an API or to state logic need an integration test, not only unit tests. So do cross-canister changes.

## Naming

Name a test after the behaviour it proves, in `snake_case` and without a `test_` prefix (clippy's `redundant_test_prefix` is denied): `scheduled_series_opens_inclusively_at_start`, `get_settlement_status_nonexistent`.

## Running

```bash
npm run test:unit                              # all unit tests, -D warnings
scripts/pic-install                            # once per machine
npm run build                                  # the integration tests load the built wasms
npm run test:integration                       # all integration tests
cargo test --test it -p clearing settlement    # one area or one test by name
```

A failing test is a signal: fix the code, or fix the test in its own change with the reason. Never commit `#[ignore]` to get to green.
