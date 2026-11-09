//! Daily sampling of index prices into the price history (`docs/risk/margin-model.md`, section
//! 9.2).
//!
//! **Disabled in phase 1.** The only live price available today is the collateral oracle written
//! by a single authorized principal (`update_asset_price`), which the design retires because one
//! bad print would sit in the history for years and distort the volatility floor. Sampling is
//! enabled in phase 2, from the robust multi-source index, by turning [`SAMPLER_ENABLED`] on. Until
//! then the history only grows through hash-checked, timelocked backfill uploads.

use core::time::Duration;

use ic_cdk_timers::set_timer_interval;

use crate::{
    memory::{ASSET_METRICS, RISK_STATE},
    utils::system::now_ns,
};

/// Whether the daily sampler runs. Off until the robust index of phase 2 exists.
pub const SAMPLER_ENABLED: bool = false;

/// The sampler runs hourly and appends at most one point per underlying per UTC day, so a missed
/// hour never loses a day.
const SAMPLER_INTERVAL: Duration = Duration::from_hours(1);

/// Arms the hourly sampler when [`SAMPLER_ENABLED`] is on. Called from both `init` and
/// `post_upgrade`, since interval timers do not survive an upgrade.
pub(crate) fn start_risk_sampler() {
    if !SAMPLER_ENABLED {
        return;
    }
    set_timer_interval(SAMPLER_INTERVAL, || async {
        sample_now();
    });
}

/// Appends today's point for every underlying with a fresh price. Returns the number of points
/// appended.
pub(crate) fn sample_now() -> usize {
    let now = now_ns();
    let metrics = ASSET_METRICS.with(|m| m.borrow().clone());
    RISK_STATE.with(|state| {
        state.borrow_mut().sample_daily(now, &|asset_id| {
            metrics
                .get(asset_id)
                .map(|m| (m.price_usd.clone(), m.last_updated_ns))
        })
    })
}
