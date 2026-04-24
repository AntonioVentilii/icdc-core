use candid::Principal;
use ic_cdk::call::Call;
use shared::types::{PayoutUnit, Series, SeriesId};

use crate::{
    api::trade::errors::TradeError,
    memory::{REGISTRY_CANISTER, SERIES, SETTLEMENT_PLANS},
    types::errors::CommonError,
};

/// Ensures that a derivative series is registered and cached locally.
///
/// If the series is not found in local state, it attempts to fetch it from the registry canister.
///
/// A series with an existing [`SettlementPlan`] (regardless of `PlanStatus`) is
/// treated as closed for trading: the call returns [`TradeError::SeriesAlreadySettled`]
/// without re-caching the series. This is the single safety rail that keeps
/// trade, limit-order, and position-transfer paths from reopening economic
/// exposure on a series whose book and positions have already been cleared.
///
/// # Arguments
/// * `series_id` - The identifier of the series to validate and cache.
///
/// # Returns
/// * [`Series`] if successfully registered or already present and not settled.
/// * [`TradeError::SeriesAlreadySettled`] if a settlement plan exists.
/// * [`TradeError`] if the registry is not set, the series is not found, or the payout unit is
///   unsupported.
pub async fn ensure_series_registered(series_id: &SeriesId) -> Result<Series, TradeError> {
    assert_series_not_settled(series_id)?;

    if let Some(series) = SERIES.with(|s| s.borrow().get(series_id).cloned()) {
        return Ok(series);
    }

    let registry = REGISTRY_CANISTER.with(|r| *r.borrow());

    if registry == Principal::anonymous() {
        return Err(TradeError::Common(CommonError::RegistryNotSet));
    }

    let response = Call::bounded_wait(registry, "get_series")
        .with_args(&(series_id.clone(),))
        .await
        .map_err(|e| TradeError::RegistryError(format!("Registry call failed: {e}")))?;

    let (series_opt,) = response
        .candid_tuple::<(Option<Series>,)>()
        .map_err(|e| TradeError::RegistryError(format!("Registry response decode failed: {e}")))?;

    let series = series_opt.ok_or_else(|| TradeError::SeriesNotFound(series_id.clone()))?;

    // Only USD-payout contracts are supported by this clearing canister version.
    if series.payout_unit != PayoutUnit::usd() {
        return Err(TradeError::Common(CommonError::Internal(format!(
            "Unsupported payout unit in series: {:?}. Only USD is supported.",
            series.payout_unit
        ))));
    }

    SERIES.with(|s| {
        s.borrow_mut()
            .insert(series.series_id.clone(), series.clone());
    });

    Ok(series)
}

/// Rejects trading paths on a series with an active or finalised settlement
/// plan. Checked before hitting the local cache or the registry so that a
/// series which has been removed from clearing's `SERIES` map on `Finalised`
/// cannot silently be re-cached and re-traded.
///
/// Returns `Err(TradeError::SeriesAlreadySettled)` if any entry exists in
/// `SETTLEMENT_PLANS` for `series_id`, regardless of `PlanStatus`.
pub fn assert_series_not_settled(series_id: &SeriesId) -> Result<(), TradeError> {
    if SETTLEMENT_PLANS.with(|m| m.borrow().contains_key(series_id)) {
        return Err(TradeError::SeriesAlreadySettled(series_id.clone()));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use shared::types::{BalanceDomain, Price, SettlementInput};

    use super::*;
    use crate::types::plans::{SettlementPlan, SettlementPlanParams};

    #[test]
    fn assert_series_not_settled_rejects_series_with_plan() {
        let series_id = SeriesId::from("settled_series".to_owned());

        // No plan yet → ok.
        SETTLEMENT_PLANS.with(|m| m.borrow_mut().remove(&series_id));
        assert!(assert_series_not_settled(&series_id).is_ok());

        // Insert a plan and verify the guard fires.
        let _plan = SettlementPlan::get_or_create(SettlementPlanParams {
            series_id: series_id.clone(),
            settlement: SettlementInput::Price(Price::new(100, 0)),
            oracle_source: "oracle".to_owned(),
            fee: 0,
            insurance_fee: 0,
            positions: vec![],
            balance_domain: BalanceDomain::Settlement,
        });

        let result = assert_series_not_settled(&series_id);
        assert!(
            matches!(&result, Err(TradeError::SeriesAlreadySettled(id)) if id == &series_id),
            "expected SeriesAlreadySettled, got {result:?}"
        );

        // Unrelated series is still open.
        let other = SeriesId::from("untouched_series".to_owned());
        assert!(assert_series_not_settled(&other).is_ok());

        // Cleanup
        SETTLEMENT_PLANS.with(|m| m.borrow_mut().remove(&series_id));
    }
}
