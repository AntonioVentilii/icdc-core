use candid::Principal;
use shared::types::{Series, SeriesId};

use crate::{
    account::is_supported_asset,
    error::ClearingError,
    memory::{REGISTRY_CANISTER, SERIES},
};

pub async fn ensure_series_registered(series_id: &SeriesId) -> Result<Series, ClearingError> {
    if let Some(series) = SERIES.with(|s| s.borrow().get(series_id).cloned()) {
        return Ok(series);
    }

    let registry = REGISTRY_CANISTER.with(|r| *r.borrow());

    if registry == Principal::anonymous() {
        return Err(ClearingError::RegistryNotSet);
    }

    let (series_opt,): (Option<Series>,) = ic_cdk::call(registry, "get_series", (series_id,))
        .await
        .map_err(|(code, msg)| {
            ClearingError::GettingRegistrySeriesFailed(format!(
                "Registry call failed: {:?}: {}",
                code, msg
            ))
        })?;

    let series = series_opt.ok_or(ClearingError::SeriesNotFound)?;

    let asset = series.settlement_asset.to_asset();

    if !is_supported_asset(&asset) {
        return Err(ClearingError::UnsupportedSettlementAsset);
    }

    SERIES.with(|s| {
        s.borrow_mut()
            .insert(series.series_id.clone(), series.clone());
    });

    Ok(series)
}
