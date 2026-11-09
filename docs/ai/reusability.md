# Reusability catalog

What already exists in `icdc-core`. Search here and in the code before writing a type, a helper, a constant, a guard or a fixture (rule 4 of the standard); when you add something others could use, add its line here in the same PR.

## Shared crate (`src/shared/src/`)

### Constants (`constants.rs`)

- `KIB`, `MIB`, `GIB`: byte sizes.
- `SECOND_NS`, `MINUTE_NS`, `HOUR_NS`, `DAY_NS`, `MONTH_NS`: durations in nanoseconds. Never write `1_000_000_000` by hand.
- `USD_DECIMALS`: decimals of internal USD accounting (4).
- `BPS_BASE`, `DEFAULT_INSURANCE_FEE_RATIO`, `DEFAULT_PROTOCOL_FEE_RATIO`: basis points and default fees.
- `MAX_SERIES_TITLE_LEN`, `MAX_SERIES_DESCRIPTION_LEN`, `MAX_SERIES_RESOLUTION_CLAUSE_LEN`, `MAX_LOCALE_LEN`, `MAX_REWARD_TITLE_LEN`, `MAX_REWARD_DESCRIPTION_LEN`, `MAX_ICON_URL_LEN`: input length limits.
- `DEFAULT_LOCALE`, `DEFAULT_SOCIAL_MAX_PER_HOUR`, `DEFAULT_SOCIAL_MAX_PER_USER`, `MAX_FORKS_PER_SOURCE_PER_USER`: defaults and rate limits.

### Types (`types/`, all re-exported from `shared::types`)

- `series.rs`: `Series` (with `generate_id`, `status`, `is_tradeable_at`, `has_resolvable_payoff`), `SeriesId`, `OutcomeId`, `Outcome`, `PayoffType`, `SettlementInput`, `SeriesStatus`, `SeriesError`, `AddSeriesParams`, `ForkSeriesParams`, `UpdateSeriesMetadataParams`, `AddSeriesResult`, `UpdateSeriesResult`, `SeriesPage`, `SocialLimits`, `is_valid_locale`.
- `series.rs`: `PaginationParams::apply` (cursor pagination over any series iterator) and `ListSeriesParams` with `matches`, `matches_expiry`, `matches_tradeable_now`, `matches_due`, `matches_time`. Reuse them for every list endpoint.
- `price.rs`: `Price` (value plus decimals). `decimal.rs`: `DecimalValue`.
- `payout.rs`: `PayoutUnit` (`usd()`, `to_asset`), `FiatUnit`, `NonMonetaryUnit`, `SocialReward` (`validate`).
- `domain.rs`: `BalanceDomain`, `AllowedBalanceDomains` (validated, non-empty), `DomainPolicy`.
- `collateral.rs`: `AssetId`, `CollateralAssetConfig`, `CollateralAssetInfo`, `AssetMetrics`.
- `asset/`: `Asset` (`as_icrc`, `as_evm`), `AssetError`, EVM types `Chain`, `ChainId`, `NativeEvmAsset`, `ErcToken`, `EvmAssetRef`.
- `description.rs`: `Description` (`plain`, `markdown`, `all`). `resolution.rs`: `Resolution::new`.
- `engine.rs`: `Engine`, `EngineId`, `EngineRole`, `RoleGrant`, their params, `EngineError`, `EngineResult`, `RegisterEngineResult`.
- `groups.rs`: `Group`, `GroupId`, `TradingAccess`, their params, `GroupError`, `GroupResult`, `CreateGroupResult`.
- `oracle.rs`: `Oracle`, `OracleMetadata`, their params, `OracleError`, `OracleResult`.
- `minter.rs`: minter `Config`, `MinterArg`, `UpgradeArg`, `MintParams`, `MintResult`, `ConfigResult`.
- `status.rs`: `CanisterStatus`.

### Migrations (`migrations/mod.rs`)

- `LegacySeries`, `upgrade_series`, `resolution_from_description`, `NO_RESOLUTION_PLACEHOLDER`: the shared series backfill both canisters apply in `restore_state`.

## Clearing (`src/clearing/src/`)

### Helpers (`utils/`)

- `account.rs`: `derive_user_subaccount_for_canister` (the one subaccount derivation; `derive_user_subaccount` for the current canister).
- `traits.rs`: `ClearingAccountExt` on `User` (`clearing_subaccount`, `clearing_account`).
- `registry.rs`: `is_oracle_authorized`, `is_trading_authorized` (bounded calls to the registry).
- `series.rs`: `ensure_series_registered` (cache or fetch a series and apply the trading-window gates), `SeriesAccess`, `assert_series_started`, `assert_series_not_expired`, `assert_series_not_settled`.
- `system.rs`: `now_ns`, `canister_id` (both safe off-chain).
- `vusd.rs`: `is_internal_ledger`, `is_internal_asset`, `get_internal_asset_id`.

### Domain logic

- `trade/service.rs`: `internal_execute_trade`, `execute_trade_impl` (the trade mutation); `trade/types.rs`: `ExecuteTradeParams`.
- `account/service.rs`: `AccountService::build_account_state_response`.
- `account/reassignment.rs`: `ReassignmentGuard` (`acquire`, `revalidate`), `ReassignmentMark`, `ReassignmentConflict`, `reassign_account`.
- `payoffs/mod.rs`: `get_unit_payoff`, `get_settlement_value`, `get_required_margin`, `PayoffError`; `payoffs/fees.rs`: `calculate_settlement_fee`; `payoffs/utils.rs`: `scale_price`; `payoffs/types.rs`: `RoundingMode`.
- `assets/asset/handler.rs`: `get_handler`, `AssetHandler` (`balance_of`, `transfer`, `transfer_from`, `get_fee`); `assets/icrc/handler.rs`: `IcrcHandler` (also `get_symbol`, `get_decimals`, `get_metadata`), `IcrcMetadata`; `assets/evm/`: `EvmHandler`, the EVM RPC client.
- `api/trade/api.rs`: `validate_no_arbitrage`, `mint_complete_set_logic`, `redeem_complete_set_logic`, `submit_market_order_impl`.
- `api/settlement/api.rs`: `prepare_settlement_impl`, `apply_settlement_accounting_logic`, `settle_series_inner`, `resume_pending_settlements`, `start_settlement_heartbeat`.

### Types (`types/`)

- `margin.rs`: `Position`, `PositionKey`, `PositionsMap`, `AccountState` (balance getters and setters per domain, `calculate_equity_usd`, `calculate_raw_equity_i128`, `get_available_margin_usd`).
- `plans.rs`: `PlanStatus` and the durable plans `DepositPlan`, `WithdrawalPlan`, `SettlementPlan`, `FundWithdrawalPlan`, `MigrationPlan`, `ReassignmentPlan` (each with `get_or_create`), `SettlementStatusView`.
- `user.rs`: `User`, `DepositId`, `WithdrawalId`, `DepositKey`, `WithdrawalKey`. `payment.rs`: `PaymentIdempotency`, `PaymentReceipt`.
- `event.rs`: `Event`, `EventType`, `SeriesTradePoint` (`notional_usd_base`), `SeriesVolumeAggregate` (`record`).
- `leaderboard.rs`: `LeaderboardWindow` (`period_id`, `prior_period_id`), `PnlAggregate`. `price_history.rs`: `PriceHistoryInterval` (`bucket_ns`, `bucket_start`).
- `errors.rs`: `CommonError`. `state.rs`: `Config`, `PositionProof`. `stats.rs`, `http.rs`, `trade.rs`, `account.rs`.

### State (`memory.rs`)

- Every state map (`CONFIG`, `POSITIONS`, `ACCOUNT_STATES`, `SERIES`, `EVENTS`, the plan maps, `INSURANCE_FUND`, `TREASURY`, the indexes) and `save_state`, `restore_state`.
- `cached_transfer_fee`, `update_cached_transfer_fee`: the ledger fee cache with `BadFee` retry.
- `index_executed_trade`, `index_settled_events` and the `rebuild_*` functions for the derived indexes.

### Guards (`guards.rs`)

- `caller_is_not_anonymous`, `caller_is_controller`.

## Registry (`src/registry/src/`)

- `guards.rs`: `caller_is_not_anonymous`, `caller_is_controller`, `has_engine_role`, `has_engine_role_on`, `is_engine_creator`, `is_engine_oracle_admin`.
- `api/groups.rs`: `is_trading_authorized` (the query the clearing calls before a trade); `api/oracles.rs`: `is_oracle_authorized`; `api/access.rs`: `set_social_limits`, `get_social_limits`.
- `utils.rs`: `canonical_id_part` (normalise an identifier part).
- `memory.rs`: `SERIES_STORE`, `ORACLE_STORE`, `GROUPS_STORE`, `ENGINE_STORE`, `SOCIAL_CREATION_LOG`, `SOCIAL_LIMITS`, the id counters, `save_state`, `restore_state`.

## Minter (`src/minter/src/`)

- `state/mod.rs`: `read_config`, `set_config`, `apply_upgrade`. `utils.rs`: `to_account`. `guards.rs`: `caller_is_authorized` plus the two common guards.

## Test fixtures

- Unit (clearing trade): `src/clearing/src/api/trade/tests/utils.rs`: `setup_test_state`, `execute_trade_checked`, `settle_series_checked`, `verify_cash_balance`, `verify_reserved_margin`, `verify_position_qty`, `create_test_user`.
- Integration (`src/clearing/tests/it/utils/`): `TestSetup` (`default`, `with_icp`, `with_all_ledgers`) and `test_user` in `test_environment.rs`; `PicCanister`, `PicCanisterBuilder`, `PicCanisterTrait` (`update`, `query`) in `pic_canister.rs`; `TradeHelperTrait` (`setup_vusd`, `setup_icp`, `setup_ckusdc`, `deposit_collateral`, `add_binary_series`, `add_linear_series`, `add_scheduled_binary_series`, `add_expired_binary_series`, `setup_icrc_asset`, `setup_evm_asset`, `submit_limit_order`, `register_engine`, `grant_engine_role`, `fork_binary_series`) and `SetupIcrcAsset` in `trade_helper.rs`; `assert_ok_value`, `assert_unauthorized`, `assert_err_msg`, `assert_decimal_eq` in `assertions.rs`; ledger and principal constants in `constants.rs` and `mock.rs`; a mock ICRC ledger in `mock_ledger.rs`.

## Scripts (`scripts/`)

- `utils.sh`: `check_network` (accept `local`, `staging` or `ic`, with or without leading dashes).
- `init.common.sh`: shared init configuration (token IDs, decimals, prices).
- `init.market.common.sh`: `resolve_maker_identity`, `place_outcome_orders`, `fetch_and_parse_markets`, `deposit_all_collateral`, `compute_required_base_units`, `read_ledger_balance`, `place_scalar_orders_random`, `place_categorical_orders_random` and their helpers. Market seeders (`init.market.*.sh`) build on these.
- `build.*.sh`, `download.*.sh`, `did.sh`, `format.*.sh`, `lint.*.sh`, `reinstall.sh`, `record.module-hashes.sh`, `send.tokens.sh`: one job each, called from `package.json` and CI.
