use candid::Principal;
use ic_cdk::call::Call;
use shared::types::{PayoutUnit, Series, SeriesId};

use crate::{
    api::trade::errors::TradeError,
    memory::{REGISTRY_CANISTER, SERIES, SETTLEMENT_PLANS},
    types::errors::CommonError,
    utils::system::now_ns,
};

/// What a caller intends to do with the series, which decides how much of the
/// trading window is enforced.
///
/// Making this explicit at every call site is the point: an operation that only
/// gives exposure back must not be locked out by the same rails that stop new
/// exposure being created, and an omission here should read as a deliberate
/// choice rather than a forgotten check.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeriesAccess {
    /// The caller creates or increases exposure — orders, trades, minting a
    /// complete set. Requires the series to be inside its full trading window.
    OpenExposure,
    /// The caller only releases or relocates exposure that already exists —
    /// redeeming a complete set, accepting a transferred position. Still barred
    /// from settled and not-yet-open series, but allowed after expiry.
    ReduceExposure,
}

/// Ensures that a derivative series is registered and cached locally.
///
/// If the series is not found in local state, it attempts to fetch it from the registry canister.
///
/// Three rails treat a series as closed, the last depending on `access`:
///
/// - A series with an existing [`SettlementPlan`] (regardless of `PlanStatus`) returns
///   [`TradeError::SeriesAlreadySettled`] without re-caching the series. This keeps trade,
///   limit-order, and position-transfer paths from reopening economic exposure on a series whose
///   book and positions have already been cleared.
/// - A scheduled series whose `start_ns` has not been reached returns
///   [`TradeError::SeriesNotStarted`]. Applied regardless of `access`: no position can exist before
///   the window opens, so there is nothing for a reducing caller to give back.
/// - An expired series returns [`TradeError::SeriesExpired`], but **only** for
///   [`SeriesAccess::OpenExposure`].
///
/// The expiry exemption is not a convenience. Settlement is oracle-triggered and chunked, so the
/// gap between `expiry_ns` and a settlement plan existing is unbounded operational latency. During
/// it, `redeem_complete_set` is the only way a user can release reserved margin, and withdrawals
/// are refused while equity sits below reserved margin — gating it would strand collateral until
/// an operator got around to settling. `accept_position_transfer` is exempt for a different
/// reason: its counterpart `freeze_position_for_transfer` is ungated and destroys the source
/// position first, so rejecting the accept would strand the position mid-migration with no
/// un-freeze path.
///
/// # Arguments
/// * `series_id` - The identifier of the series to validate and cache.
/// * `access` - Whether the caller opens or only reduces exposure.
///
/// # Returns
/// * [`Series`] if successfully registered, not settled, and open for the requested `access`.
/// * [`TradeError::SeriesAlreadySettled`] if a settlement plan exists.
/// * [`TradeError::SeriesNotStarted`] if the trading window has not opened yet.
/// * [`TradeError::SeriesExpired`] if the window has closed and `access` is `OpenExposure`.
/// * [`TradeError`] if the registry is not set, the series is not found, or the payout unit is
///   unsupported.
pub async fn ensure_series_registered(
    series_id: &SeriesId,
    access: SeriesAccess,
) -> Result<Series, TradeError> {
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
    // through this function cannot acquire a series without passing the gates.
    let now = now_ns();
    assert_series_started(&series, now)?;
    if access == SeriesAccess::OpenExposure {
        assert_series_not_expired(&series, now)?;
    }

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

/// Rejects exposure-opening paths on a series whose trading window has closed.
///
/// A series expires **at** `expiry_ns`: the comparison is `now >= expiry_ns`,
/// exclusive at the close, matching the registry's `Series::status` and the
/// `[start_ns, expiry_ns)` window the Candid interface already documents.
///
/// Reading the cached copy is safe for the same reason the start gate is:
/// `expiry_ns` is hashed into the `series_id`, so it is immutable for the life
/// of a series and clearing's never-refreshed `SERIES` cache cannot go stale on
/// it.
pub fn assert_series_not_expired(series: &Series, now: u64) -> Result<(), TradeError> {
    if now >= series.expiry_ns {
        return Err(TradeError::SeriesExpired {
            series_id: series.series_id.clone(),
            expiry_ns: series.expiry_ns,
        });
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

    /// Exclusive at the close: a series expires *at* `expiry_ns`, matching the
    /// registry's `only_unexpired` filter and the `[start_ns, expiry_ns)` window
    /// the Candid interface documents.
    #[test]
    fn series_expires_exclusively_at_expiry() {
        let series = series_with_start(None);
        let expiry = series.expiry_ns;

        assert!(assert_series_not_expired(&series, expiry - 1).is_ok());

        let at_expiry = assert_series_not_expired(&series, expiry);
        assert!(
            matches!(
                &at_expiry,
                Err(TradeError::SeriesExpired { series_id, expiry_ns })
                    if series_id == &series.series_id && *expiry_ns == expiry
            ),
            "expected SeriesExpired carrying the close, got {at_expiry:?}"
        );

        assert!(assert_series_not_expired(&series, expiry + 1).is_err());
    }
}
