//! Daily sampling of oracle prices into the price history (`docs/risk/margin-model.md`, section
//! 9.2).

use core::time::Duration;

use ic_cdk_timers::set_timer_interval;

use crate::{
    memory::{ASSET_METRICS, RISK_STATE},
    utils::system::now_ns,
};

/// The sampler runs hourly and appends at most one point per underlying per UTC day, so a missed
/// hour never loses a day.
const SAMPLER_INTERVAL: Duration = Duration::from_hours(1);

/// Arms the hourly sampler. Must be armed from both `init` and `post_upgrade`, since interval
/// timers do not survive an upgrade.
pub(crate) fn start_risk_sampler() {
    set_timer_interval(SAMPLER_INTERVAL, || async {
        sample_now();
    });
}

/// Appends today's point for every underlying with a fresh oracle price. Returns the number of
/// points appended.
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
