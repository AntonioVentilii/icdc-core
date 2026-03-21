//! Inter-canister helpers for the Series Registry.

use candid::Principal;
use ic_cdk::call::Call;

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
