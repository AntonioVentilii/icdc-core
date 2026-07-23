use core::future::Future;

use candid::{Nat, Principal};
use ic_cdk::{
    api::canister_self,
    call::{Call, CallFailed},
};
use icrc_ledger_types::{
    icrc1::{
        account::Account,
        transfer::{TransferArg, TransferError},
    },
    icrc2::transfer_from::{TransferFromArgs, TransferFromError},
};
use num_traits::ToPrimitive as _;
use shared::types::{asset::errors::AssetError, Asset, AssetId};

use crate::{
    assets::{
        asset::params::{AssetBalanceOfParams, AssetTransferFromParams, AssetTransferParams},
        types::AssetAmount,
    },
    memory::{cached_transfer_fee, update_cached_transfer_fee},
    traits::ClearingAccountExt as _,
    types::account::{AssetAccount, ExternalAssetAccount},
};

/// `AssetError::CallError.code` when the failure is not an IC reject (e.g. insufficient cycles).
const LEDGER_CALL_CODE_NON_REJECT: i32 = -1;
/// `AssetError::CallError.code` when the response bytes could not be decoded as Candid.
const LEDGER_CALL_CODE_CANDID_DECODE: i32 = -2;
/// Used when `raw_reject_code` does not fit in `i32`.
const LEDGER_CALL_REJECT_CODE_OVERFLOW: i32 = i32::MAX;

fn map_ledger_call_failed(method: &str, err: CallFailed) -> AssetError {
    match err {
        CallFailed::CallRejected(r) => AssetError::CallError {
            method: method.to_owned(),
            code: i32::try_from(r.raw_reject_code()).unwrap_or(LEDGER_CALL_REJECT_CODE_OVERFLOW),
            message: r.reject_message().to_owned(),
        },
        e => AssetError::CallError {
            method: method.to_owned(),
            code: LEDGER_CALL_CODE_NON_REJECT,
            message: e.to_string(),
        },
    }
}

/// Outcome of a single `icrc1_transfer` call once the ledger response has been
/// decoded, distinguishing a settled block from a recoverable [`BadFee`] drift.
///
/// [`BadFee`]: TransferError::BadFee
enum SettledTransfer {
    /// The transfer settled (or was a `Duplicate`); carries the block index.
    Block(u128),
    /// The ledger rejected the sent fee and reported the one it expects.
    BadFee { expected_fee: u128 },
}

/// Sends one `icrc1_transfer` call and decodes the ledger response, surfacing IC
/// call and Candid decode failures as [`AssetError`].
async fn send_icrc1_transfer(
    ledger_id: Principal,
    args: TransferArg,
) -> Result<Result<Nat, TransferError>, AssetError> {
    let response = Call::bounded_wait(ledger_id, "icrc1_transfer")
        .with_args(&(args,))
        .await
        .map_err(|e| map_ledger_call_failed("icrc1_transfer", e))?;

    let (res,) = response
        .candid_tuple::<(Result<Nat, TransferError>,)>()
        .map_err(|e| AssetError::CallError {
            method: "icrc1_transfer".to_owned(),
            code: LEDGER_CALL_CODE_CANDID_DECODE,
            message: e.to_string(),
        })?;

    Ok(res)
}

/// Classifies a decoded `icrc1_transfer` result for the cached-fee protocol.
///
/// A `Duplicate` maps to its `duplicate_of` block (the idempotent retry
/// succeeded), a `BadFee` is surfaced with the ledger's expected fee so the
/// caller can refresh the cache and retry, and every other [`TransferError`] is
/// a definitive failure.
fn settle_transfer(res: Result<Nat, TransferError>) -> Result<SettledTransfer, AssetError> {
    match res {
        Ok(block) => Ok(SettledTransfer::Block(
            block.0.to_u128().ok_or(AssetError::MathOverflow)?,
        )),
        Err(TransferError::Duplicate { duplicate_of }) => Ok(SettledTransfer::Block(
            duplicate_of.0.to_u128().ok_or(AssetError::MathOverflow)?,
        )),
        Err(TransferError::BadFee { expected_fee }) => Ok(SettledTransfer::BadFee {
            expected_fee: expected_fee.0.to_u128().ok_or(AssetError::MathOverflow)?,
        }),
        Err(e) => Err(AssetError::TransferError(format!("{e:?}"))),
    }
}

/// Drives the cached-fee protocol over an injected `send` function.
///
/// `build_args` stamps the fee (and the shared `created_at_time`) onto a
/// [`TransferArg`], and `send` performs one settled `icrc1_transfer`. The
/// protocol is: send with `cached_fee`; on [`SettledTransfer::BadFee`] refresh
/// the cache and retry **once** with the ledger's expected fee; a second
/// `BadFee` fails, leaving the refreshed cache for the next call.
///
/// The ledger call is injected so the retry, cache-refresh, and
/// `created_at_time`-reuse behaviour can be unit-tested without a live ledger.
async fn run_cached_fee_transfer<B, S, Fut>(
    asset_id: &AssetId,
    cached_fee: Option<u128>,
    build_args: B,
    mut send: S,
) -> Result<u128, AssetError>
where
    B: Fn(Option<u128>) -> TransferArg,
    S: FnMut(TransferArg) -> Fut,
    Fut: Future<Output = Result<SettledTransfer, AssetError>>,
{
    // Attempt 1: send the cached fee explicitly.
    let expected_fee = match send(build_args(cached_fee)).await? {
        SettledTransfer::Block(block) => return Ok(block),
        SettledTransfer::BadFee { expected_fee } => expected_fee,
    };

    // Refresh the cache and retry once with the ledger's expected fee.
    update_cached_transfer_fee(asset_id, expected_fee);
    match send(build_args(Some(expected_fee))).await? {
        SettledTransfer::Block(block) => Ok(block),
        SettledTransfer::BadFee { expected_fee } => {
            // Definitive rejection: keep the refreshed cache for the next try.
            update_cached_transfer_fee(asset_id, expected_fee);
            Err(AssetError::TransferError(format!(
                "BadFee: ledger expected fee {expected_fee}"
            )))
        }
    }
}

/// Implementation of [`AssetHandler`] for ICRC-1 and ICRC-2 compatible ledgers.
pub struct IcrcHandler;

impl IcrcHandler {
    /// Retrieves the balance of an ICRC account.
    pub async fn balance_of(&self, params: AssetBalanceOfParams<'_>) -> Result<u128, AssetError> {
        let ledger_id = params.asset.as_icrc()?;

        let account = resolve_account(&params.account)?;

        let response = Call::bounded_wait(*ledger_id, "icrc1_balance_of")
            .with_args(&(account,))
            .await
            .map_err(|e| map_ledger_call_failed("icrc1_balance_of", e))?;

        let (ledger_balance,) =
            response
                .candid_tuple::<(Nat,)>()
                .map_err(|e| AssetError::CallError {
                    method: "icrc1_balance_of".to_owned(),
                    code: LEDGER_CALL_CODE_CANDID_DECODE,
                    message: e.to_string(),
                })?;

        ledger_balance.0.to_u128().ok_or(AssetError::MathOverflow)
    }

    /// Retrieves the transfer fee of an ICRC ledger.
    pub async fn get_fee(&self, asset: &Asset) -> Result<u128, AssetError> {
        let ledger_id = asset.as_icrc()?;

        let response = Call::bounded_wait(*ledger_id, "icrc1_fee")
            .await
            .map_err(|e| map_ledger_call_failed("icrc1_fee", e))?;

        let (fee_nat,) = response
            .candid_tuple::<(Nat,)>()
            .map_err(|e| AssetError::CallError {
                method: "icrc1_fee".to_owned(),
                code: LEDGER_CALL_CODE_CANDID_DECODE,
                message: e.to_string(),
            })?;

        fee_nat.0.to_u128().ok_or(AssetError::MathOverflow)
    }

    /// Retrieves the symbol of an ICRC ledger.
    pub async fn get_symbol(&self, asset: &Asset) -> Result<String, AssetError> {
        let ledger_id = asset.as_icrc()?;

        let response = Call::bounded_wait(*ledger_id, "icrc1_symbol")
            .await
            .map_err(|e| map_ledger_call_failed("icrc1_symbol", e))?;

        let (symbol,) =
            response
                .candid_tuple::<(String,)>()
                .map_err(|e| AssetError::CallError {
                    method: "icrc1_symbol".to_owned(),
                    code: LEDGER_CALL_CODE_CANDID_DECODE,
                    message: e.to_string(),
                })?;

        Ok(symbol)
    }

    /// Retrieves the decimals of an ICRC ledger.
    pub async fn get_decimals(&self, asset: &Asset) -> Result<u8, AssetError> {
        let ledger_id = asset.as_icrc()?;

        let response = Call::bounded_wait(*ledger_id, "icrc1_decimals")
            .await
            .map_err(|e| map_ledger_call_failed("icrc1_decimals", e))?;

        let (decimals,) = response
            .candid_tuple::<(u8,)>()
            .map_err(|e| AssetError::CallError {
                method: "icrc1_decimals".to_owned(),
                code: LEDGER_CALL_CODE_CANDID_DECODE,
                message: e.to_string(),
            })?;

        Ok(decimals)
    }

    /// Executes an ICRC-1 transfer using the cached-fee protocol.
    ///
    /// Internal accounting deducts balances against a cached expectation of the
    /// ledger fee (`AssetMetrics::latest_transfer_fee`). Sending `fee: None`
    /// lets the ledger silently apply a drifted fee, desynchronising internal
    /// balances from real ledger movements without ever raising an error. To
    /// close that gap we send the cached fee explicitly and self-heal on drift:
    ///
    /// 1. The first attempt sends the cached fee (when known) in [`TransferArg::fee`], so any drift
    ///    surfaces as an explicit `BadFee` instead of a silent mismatch.
    /// 2. On [`TransferError::BadFee`] the cache is refreshed with the ledger's `expected_fee` and
    ///    the transfer is retried **once** with that value, reusing the same `created_at_time` so
    ///    the retry stays idempotent.
    /// 3. A second `BadFee` fails the attempt (definitive rejection), leaving the refreshed cache
    ///    in place for the next call to reuse.
    pub async fn transfer(&self, params: AssetTransferParams<'_>) -> Result<u128, AssetError> {
        let ledger_id = *params.asset.as_icrc()?;

        let from_account = resolve_account(&params.from)?;

        let to_account = resolve_account(&params.to)?;

        let AssetAmount::Fixed(amount_u128) = params.amount;

        // Both attempts reuse the same `created_at_time`, so the retry is an
        // idempotent duplicate of the first attempt at the ledger.
        let build_args = |fee: Option<u128>| TransferArg {
            from_subaccount: from_account.subaccount,
            to: to_account,
            amount: Nat::from(amount_u128),
            fee: fee.map(Nat::from),
            memo: None,
            created_at_time: params.created_at_time_ns,
        };

        let cached_fee = cached_transfer_fee(params.asset_id);

        run_cached_fee_transfer(params.asset_id, cached_fee, build_args, |args| async move {
            settle_transfer(send_icrc1_transfer(ledger_id, args).await?)
        })
        .await
    }

    /// Executes an ICRC-2 `transfer_from` call.
    pub async fn transfer_from(
        &self,
        params: AssetTransferFromParams<'_>,
    ) -> Result<u128, AssetError> {
        let ledger_id = params.asset.as_icrc()?;

        let spender_account = resolve_account(&params.spender)?;

        let from_account = resolve_account(&params.from)?;

        let to_account = resolve_account(&params.to)?;

        let AssetAmount::Fixed(amount_u128) = params.amount;

        let icrc_args = TransferFromArgs {
            spender_subaccount: spender_account.subaccount,
            from: from_account,
            to: to_account,
            amount: Nat::from(amount_u128),
            fee: None,
            memo: None,
            created_at_time: params.created_at_time_ns,
        };

        let response = Call::bounded_wait(*ledger_id, "icrc2_transfer_from")
            .with_args(&(icrc_args,))
            .await
            .map_err(|e| map_ledger_call_failed("icrc2_transfer_from", e))?;

        let (res,) = response
            .candid_tuple::<(Result<Nat, TransferFromError>,)>()
            .map_err(|e| AssetError::CallError {
                method: "icrc2_transfer_from".to_owned(),
                code: LEDGER_CALL_CODE_CANDID_DECODE,
                message: e.to_string(),
            })?;

        match res {
            Ok(block) => block.0.to_u128().ok_or(AssetError::MathOverflow),
            Err(TransferFromError::Duplicate { duplicate_of }) => {
                duplicate_of.0.to_u128().ok_or(AssetError::MathOverflow)
            }
            Err(e) => Err(AssetError::TransferError(format!("{e:?}"))),
        }
    }

    /// Fetches all relevant metadata for an ICRC token.
    pub async fn get_metadata(&self, asset: &Asset) -> Result<IcrcMetadata, AssetError> {
        let symbol = self.get_symbol(asset).await?;
        let decimals = self.get_decimals(asset).await?;
        let fee = self.get_fee(asset).await?;

        Ok(IcrcMetadata {
            symbol,
            decimals,
            fee,
        })
    }
}

pub struct IcrcMetadata {
    pub symbol: String,
    pub decimals: u8,
    pub fee: u128,
}

/// Resolves a [`AssetAccount`] into a concrete [`Account`].
fn resolve_account(account: &AssetAccount) -> Result<Account, AssetError> {
    match account {
        AssetAccount::UserClearing(u) => Ok(u.clearing_account()),
        AssetAccount::CanisterMain => Ok(Account {
            owner: canister_self(),
            subaccount: None,
        }),
        AssetAccount::External(ExternalAssetAccount::Principal(principal)) => Ok(Account {
            owner: *principal,
            subaccount: None,
        }),
        AssetAccount::External(ExternalAssetAccount::Icrc { owner, subaccount }) => Ok(Account {
            owner: *owner,
            subaccount: *subaccount,
        }),
        AssetAccount::External(ExternalAssetAccount::Evm(_)) => {
            Err(AssetError::InvalidAssetForHandler)
        }
    }
}

#[cfg(test)]
mod tests {
    use core::{
        cell::RefCell,
        future::{ready, Future},
        pin::pin,
        task::{Context, Poll, Waker},
    };

    use candid::{Nat, Principal};
    use icrc_ledger_types::icrc1::{
        account::Account,
        transfer::{TransferArg, TransferError},
    };
    use shared::types::{asset::errors::AssetError, AssetMetrics, DecimalValue};

    use super::{run_cached_fee_transfer, settle_transfer, SettledTransfer};
    use crate::memory::ASSET_METRICS;

    /// Minimal executor for the always-ready futures these tests use; avoids
    /// pulling in an async runtime as a dev-dependency.
    fn block_on<F: Future>(future: F) -> F::Output {
        let mut future = pin!(future);
        let mut cx = Context::from_waker(Waker::noop());
        loop {
            if let Poll::Ready(value) = future.as_mut().poll(&mut cx) {
                return value;
            }
        }
    }

    /// Seeds an [`ASSET_METRICS`] entry so the fee-cache helpers have something
    /// to read and self-heal.
    fn seed_metrics(asset_id: &str, fee: Option<u128>) {
        ASSET_METRICS.with(|m| {
            m.borrow_mut().insert(
                asset_id.to_owned(),
                AssetMetrics {
                    price_usd: DecimalValue {
                        value: 1_000_000,
                        decimals: 6,
                    },
                    haircut_bps: 0,
                    latest_transfer_fee: fee,
                    insurance_fee_ratio: None,
                    protocol_fee_ratio: None,
                    last_updated_ns: None,
                },
            );
        });
    }

    fn cached_fee(asset_id: &str) -> Option<u128> {
        ASSET_METRICS.with(|m| {
            m.borrow()
                .get(asset_id)
                .and_then(|metrics| metrics.latest_transfer_fee)
        })
    }

    /// `build_args` for the orchestration tests: stamps the fee and a fixed
    /// `created_at_time` so retries can be checked for idempotency.
    fn build_args(fee: Option<u128>) -> TransferArg {
        TransferArg {
            from_subaccount: None,
            to: Account {
                owner: Principal::anonymous(),
                subaccount: None,
            },
            amount: Nat::from(1_000_u64),
            fee: fee.map(Nat::from),
            memo: None,
            created_at_time: Some(42),
        }
    }

    /// Runs the orchestrator against a scripted sequence of ledger outcomes,
    /// recording the `(fee, created_at_time)` of every attempt.
    fn run_with(
        asset_id: &str,
        outcomes: Vec<SettledTransfer>,
    ) -> (Result<u128, AssetError>, Vec<(Option<Nat>, Option<u64>)>) {
        let scripted = RefCell::new(outcomes.into_iter());
        let calls: RefCell<Vec<(Option<Nat>, Option<u64>)>> = RefCell::new(Vec::new());

        let result = block_on(run_cached_fee_transfer(
            &asset_id.to_owned(),
            cached_fee(asset_id),
            build_args,
            |args: TransferArg| {
                calls
                    .borrow_mut()
                    .push((args.fee.clone(), args.created_at_time));
                let next = scripted
                    .borrow_mut()
                    .next()
                    .expect("sent more transfers than scripted outcomes");
                ready(Ok(next))
            },
        ));

        (result, calls.into_inner())
    }

    #[test]
    fn transfer_happy_path_sends_cached_fee_once() {
        seed_metrics("fee-happy", Some(10));

        let (result, calls) = run_with("fee-happy", vec![SettledTransfer::Block(7)]);

        assert!(matches!(result, Ok(7)));
        assert_eq!(calls.len(), 1, "no retry on success");
        assert_eq!(calls[0].0, Some(Nat::from(10_u64)), "sends the cached fee");
        assert_eq!(cached_fee("fee-happy"), Some(10), "cache is untouched");
    }

    #[test]
    fn transfer_bad_fee_refreshes_cache_and_retries_once() {
        seed_metrics("fee-retry", Some(10));

        let (result, calls) = run_with(
            "fee-retry",
            vec![
                SettledTransfer::BadFee { expected_fee: 25 },
                SettledTransfer::Block(9),
            ],
        );

        assert!(matches!(result, Ok(9)));
        assert_eq!(calls.len(), 2, "retries exactly once");
        assert_eq!(
            calls[0].0,
            Some(Nat::from(10_u64)),
            "first sends cached fee"
        );
        assert_eq!(
            calls[1].0,
            Some(Nat::from(25_u64)),
            "retry sends the ledger's expected fee"
        );
        assert_eq!(calls[0].1, calls[1].1, "retry reuses the created_at_time");
        assert_eq!(cached_fee("fee-retry"), Some(25), "cache is refreshed");
    }

    #[test]
    fn transfer_second_bad_fee_fails_and_leaves_refreshed_cache() {
        seed_metrics("fee-fail", Some(10));

        let (result, calls) = run_with(
            "fee-fail",
            vec![
                SettledTransfer::BadFee { expected_fee: 25 },
                SettledTransfer::BadFee { expected_fee: 30 },
            ],
        );

        assert!(matches!(result, Err(AssetError::TransferError(_))));
        assert_eq!(calls.len(), 2, "never retries more than once");
        assert_eq!(
            cached_fee("fee-fail"),
            Some(30),
            "cache keeps the latest expected fee for the next try"
        );
    }

    #[test]
    fn settle_ok_returns_block() {
        let settled = settle_transfer(Ok(Nat::from(7_u64))).unwrap();
        assert!(matches!(settled, SettledTransfer::Block(7)));
    }

    #[test]
    fn settle_duplicate_returns_original_block() {
        let settled = settle_transfer(Err(TransferError::Duplicate {
            duplicate_of: Nat::from(42_u64),
        }))
        .unwrap();
        assert!(matches!(settled, SettledTransfer::Block(42)));
    }

    #[test]
    fn settle_bad_fee_surfaces_expected_fee() {
        let settled = settle_transfer(Err(TransferError::BadFee {
            expected_fee: Nat::from(10_u64),
        }))
        .unwrap();
        assert!(matches!(
            settled,
            SettledTransfer::BadFee { expected_fee: 10 }
        ));
    }

    #[test]
    fn settle_other_error_is_definitive_failure() {
        assert!(matches!(
            settle_transfer(Err(TransferError::TooOld)),
            Err(AssetError::TransferError(_))
        ));
    }
}
