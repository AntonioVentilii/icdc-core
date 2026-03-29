//! Inter-canister helpers for the Series Registry.

use candid::Principal;
use ic_cdk::call::Call;
use shared::types::SeriesId;

use crate::types::errors::CommonError;

/// Calls the registry `is_oracle_authorized` method (bounded wait).
///
/// Returns `Ok(true)` when the registry authorizes `principal` for `oracle_id`.
/// Transport and Candid decode failures are [`CommonError::Internal`].
pub(crate) async fn is_oracle_authorized(
    registry: Principal,
    oracle_id: String,
    principal: Principal,
) -> Result<bool, CommonError> {
    let response = Call::bounded_wait(registry, "is_oracle_authorized")
        .with_args(&(oracle_id, principal))
        .await
        .map_err(|e| CommonError::Internal(format!("Registry call failed: {e}")))?;

    let (authorized,) = response
        .candid_tuple::<(bool,)>()
        .map_err(|e| CommonError::Internal(format!("Registry response decode failed: {e}")))?;

    Ok(authorized)
}

/// Calls the registry's `is_trading_authorized` query via bounded inter-canister call.
///
/// Resolves the [`TradingAccess`](shared::types::TradingAccess) policies on the
/// given series and checks whether `principal` is authorized to trade:
/// controllers always pass, `Open` policies always pass, `Restricted`
/// policies require group membership.
///
/// # Arguments
///
/// * `registry` — the principal of the registry canister.
/// * `principal` — the trader whose authorization is being checked.
/// * `series_id` — the series the trader wants to trade on.
///
/// # Returns
///
/// * `Ok(true)` — the principal may trade this series.
/// * `Ok(false)` — the principal is not authorized.
/// * `Err(CommonError::Internal)` — transport or Candid decode failure.
pub(crate) async fn is_trading_authorized(
    registry: Principal,
    principal: Principal,
    series_id: SeriesId,
) -> Result<bool, CommonError> {
    let response = Call::bounded_wait(registry, "is_trading_authorized")
        .with_args(&(principal, series_id))
        .await
        .map_err(|e| CommonError::Internal(format!("Registry call failed: {e}")))?;

    let (authorized,) = response
        .candid_tuple::<(bool,)>()
        .map_err(|e| CommonError::Internal(format!("Registry response decode failed: {e}")))?;

    Ok(authorized)
}
