use candid::Principal;
use ic_cdk::call::Call;
use shared::types::{PayoutUnit, Series, SeriesId};

use crate::{
    api::trade::errors::TradeError,
    memory::{REGISTRY_CANISTER, SERIES, SETTLEMENT_PLANS},
    types::errors::CommonError,
    utils::system::now_ns,
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
/// A scheduled series whose `start_ns` has not been reached is likewise treated
/// as closed for trading, returning [`TradeError::SeriesNotStarted`]. Together
/// with the settlement rail above this bounds economic exposure on both ends of
/// the trading window.
///
/// # Returns
/// * [`Series`] if successfully registered, not settled, and open for trading.
/// * [`TradeError::SeriesAlreadySettled`] if a settlement plan exists.
/// * [`TradeError::SeriesNotStarted`] if the trading window has not opened yet.
/// * [`TradeError`] if the registry is not set, the series is not found, or the payout unit is
///   unsupported.
pub async fn ensure_series_registered(series_id: &SeriesId) -> Result<Series, TradeError> {
    assert_series_not_settled(series_id)?;

    let series = if let Some(series) = SERIES.with(|s| s.borrow().get(series_id).cloned()) {
        series
    } else {
        let registry = REGISTRY_CANISTER.with(|r| *r.borrow());

        if registry == Principal::anonymous() {
            return Err(TradeError::Common(CommonError::RegistryNotSet));
        }

        let response = Call::bounded_wait(registry, "get_series")
            .with_args(&(series_id.clone(),))
            .await
            .map_err(|e| TradeError::RegistryError(format!("Registry call failed: {e}")))?;

        let (series_opt,) = response.candid_tuple::<(Option<Series>,)>().map_err(|e| {
            TradeError::RegistryError(format!("Registry response decode failed: {e}"))
        })?;

        let series = series_opt.ok_or_else(|| TradeError::SeriesNotFound(series_id.clone()))?;

        // Only USD-payout contracts are supported by this clearing canister version.
        if series.payout_unit != PayoutUnit::usd() {
            return Err(TradeError::Common(CommonError::Internal(format!(
                "Unsupported payout unit in series: {:?}. Only USD is supported.",
                series.payout_unit
            ))));
        }

        // Cached even when the window has not opened yet: the series is a valid,
        // registered contract either way, and caching it here means a scheduled
        // market does not re-hit the registry on every early attempt.
        SERIES.with(|s| {
            s.borrow_mut()
                .insert(series.series_id.clone(), series.clone());
        });

        series
    };

    // Checked on the single exit rather than in each branch, so a future path
    // through this function cannot acquire a series without passing the gate.
    assert_series_started(&series, now_ns())?;

    Ok(series)
}

/// Rejects trading paths on a scheduled series whose trading window has not
/// opened yet.
///
/// The comparison is inclusive at the open — a series is tradeable at exactly
/// `start_ns` — matching [`Series::status`] in the registry, so a client
/// counting down to the announced instant is never told "not yet" at zero.
///
/// Reading the cached copy is safe despite clearing never refreshing its
/// `SERIES` cache: `start_ns` is hashed into the `series_id`, so it is immutable
/// for the life of a series. A cached series carries the same window it was
/// registered with, forever.
pub fn assert_series_started(series: &Series, now: u64) -> Result<(), TradeError> {
    if let Some(start_ns) = series.start_ns {
        if now < start_ns {
            return Err(TradeError::SeriesNotStarted {
                series_id: series.series_id.clone(),
                start_ns,
            });
        }
    }

    Ok(())
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
    use shared::types::{
        BalanceDomain, Description, PayoffType, Price, Resolution, SettlementInput, TradingAccess,
    };

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

    fn series_with_start(start_ns: Option<u64>) -> Series {
        Series {
            series_id: SeriesId::from("scheduled_series".to_owned()),
            underlying: "ICP".to_owned(),
            expiry_ns: 2_000_000_000,
            start_ns,
            payoff_type: PayoffType::Binary,
            strike: None,
            price_precision: 8,
            payout_unit: PayoutUnit::usd(),
            outcomes: None,
            oracle_source: "oracle".to_owned(),
            creator: Principal::anonymous(),
            created_at_ns: 0,
            title: "Scheduled".to_owned(),
            description: Description::plain("Scheduled market"),
            resolution: Resolution::new("Resolved per oracle at expiry"),
            icon_url: None,
            banner_url: None,
            balance_domain: BalanceDomain::Settlement,
            trading_access: vec![TradingAccess::Open],
            engine_id: None,
            forked_from: None,
            locale: None,
        }
    }

    #[test]
    fn unscheduled_series_is_always_open() {
        let series = series_with_start(None);

        assert!(assert_series_started(&series, 0).is_ok());
        assert!(assert_series_started(&series, 1_000).is_ok());
    }

    /// Inclusive at the open: a series is tradeable at exactly `start_ns`, so a
    /// client counting down to the announced instant is not told "not yet" at
    /// zero.
    #[test]
    fn scheduled_series_opens_inclusively_at_start() {
        let series = series_with_start(Some(1_000));

        let early = assert_series_started(&series, 999);
        assert!(
            matches!(
                &early,
                Err(TradeError::SeriesNotStarted { series_id, start_ns })
                    if series_id == &series.series_id && *start_ns == 1_000
            ),
            "expected SeriesNotStarted carrying the open, got {early:?}"
        );

        assert!(
            assert_series_started(&series, 1_000).is_ok(),
            "must be tradeable at exactly the open"
        );
        assert!(assert_series_started(&series, 1_001).is_ok());
    }
}
