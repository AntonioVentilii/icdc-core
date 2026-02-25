use candid::Principal;
use shared::types::{Series, SeriesId};

use crate::{
    memory::{REGISTRY_CANISTER, SERIES},
    types::errors::{CommonError, TradeError},
    utils::asset::is_supported_asset,
};

/// Ensures that a derivative series is registered and cached locally.
///
/// If the series is not found in local state, it attempts to fetch it from the registry canister.
///
/// # Arguments
/// * `series_id` - The identifier of the series to validate and cache.
///
/// # Returns
/// * [`Series`] if successfully registered or already present.
/// * [`TradeError`] if the registry is not set, the series is not found, or the asset is
///   unsupported.
pub async fn ensure_series_registered(series_id: &SeriesId) -> Result<Series, TradeError> {
    if let Some(series) = SERIES.with(|s| s.borrow().get(series_id).cloned()) {
        return Ok(series);
    }

    let registry = REGISTRY_CANISTER.with(|r| *r.borrow());

    if registry == Principal::anonymous() {
        return Err(TradeError::Common(CommonError::RegistryNotSet));
    }

    let (series_opt,): (Option<Series>,) = ic_cdk::call(registry, "get_series", (series_id,))
        .await
        .map_err(|(code, msg)| {
            TradeError::RegistryError(format!("Registry call failed: {:?}: {}", code, msg))
        })?;

    let series = series_opt.ok_or_else(|| TradeError::SeriesNotFound(series_id.clone()))?;

    let asset = series.settlement_asset.to_asset();

    if !is_supported_asset(&asset) {
        // Technically this could be a separate error, but let's reuse a logical one or add if
        // needed. For now, let's say it's an internal-ish error that the registry has a
        // series with unsupported asset.
        return Err(TradeError::Common(CommonError::Internal(
            "Unsupported settlement asset in registry".to_string(),
        )));
    }

    SERIES.with(|s| {
        s.borrow_mut()
            .insert(series.series_id.clone(), series.clone());
    });

    Ok(series)
}
