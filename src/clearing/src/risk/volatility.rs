//! Volatility: EWMA with a floor, volatility time and multiplier composition
//! (`docs/risk/margin-model.md`, sections 3 and 6). All figures are daily, per unit of volatility
//! time.

use crate::risk::{
    calendar::{day_of, day_weight, CalendarEvent, NS_PER_DAY},
    fixed::{Fixed, FixedError},
    history::PricePoint,
};

/// Number of normalized returns averaged to seed the EWMA recursion.
pub const EWMA_SEED_RETURNS: usize = 20;

/// Minimum number of normalized returns needed for an estimate.
pub const MIN_RETURNS: usize = EWMA_SEED_RETURNS + 1;

/// A volatility estimate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VolatilityEstimate {
    /// Square root of the EWMA variance.
    pub sigma_ewma: Fixed,
    /// Square root of the mean normalized squared return over the whole history.
    pub sigma_long_run: Fixed,
    /// `max(sigma_ewma, floor_ratio x sigma_long_run)`.
    pub sigma_used: Fixed,
    /// Number of normalized returns used.
    pub returns: usize,
}

/// Squared log returns between consecutive points, each divided by the volatility time elapsed
/// between them.
///
/// Elapsed volatility time counts the days after the earlier point up to and including the later
/// one, with weekends weighted by `weekend_weight`. Event weights are not applied to history, so
/// event variance stays in the data the model learns from. A pair whose elapsed volatility time is
/// zero is skipped.
pub fn normalized_squared_returns(
    points: &[PricePoint],
    weekend_weight: Fixed,
) -> Result<Vec<Fixed>, FixedError> {
    let mut returns = Vec::with_capacity(points.len().saturating_sub(1));
    for pair in points.windows(2) {
        let (earlier, later) = (pair[0], pair[1]);
        let mut elapsed = Fixed::ZERO;
        for day in earlier.date.saturating_add(1)..=later.date {
            elapsed = elapsed.checked_add(day_weight(day, weekend_weight))?;
        }
        if elapsed == Fixed::ZERO {
            continue;
        }
        let log_return = later.price.checked_div(earlier.price)?.ln()?;
        let squared = log_return.checked_mul(log_return)?;
        returns.push(squared.checked_div(elapsed)?);
    }
    Ok(returns)
}

/// EWMA and long-run volatility with the floor of section 6.2. `None` when fewer than
/// [`MIN_RETURNS`] returns exist.
///
/// The recursion is seeded with the mean of the first [`EWMA_SEED_RETURNS`] returns, then
/// `variance = lambda x variance + (1 - lambda) x x_t` for each following return.
pub fn estimate(
    returns: &[Fixed],
    lambda: Fixed,
    floor_ratio: Fixed,
) -> Result<Option<VolatilityEstimate>, FixedError> {
    if returns.len() < MIN_RETURNS {
        return Ok(None);
    }

    let (seed, rest) = returns.split_at(EWMA_SEED_RETURNS);
    let mut variance = mean(seed)?;
    let decay_complement = Fixed::ONE.checked_sub(lambda)?;
    for x in rest {
        variance = lambda
            .checked_mul(variance)?
            .checked_add(decay_complement.checked_mul(*x)?)?;
    }

    let sigma_ewma = variance.sqrt()?;
    let sigma_long_run = mean(returns)?.sqrt()?;
    let floor = floor_ratio.checked_mul(sigma_long_run)?;
    Ok(Some(VolatilityEstimate {
        sigma_ewma,
        sigma_long_run,
        sigma_used: sigma_ewma.max(floor),
        returns: returns.len(),
    }))
}

/// Volatility time over the next `horizon_days` days starting with today's date (section 6.3).
///
/// Each day contributes its day weight. Each calendar event inside `[now, now + horizon)` adds
/// `(weight - 1) x day_weight x ramp`, with `ramp = 1 - (event_time - now) / horizon`: an event at
/// the edge of the horizon contributes nothing and reaches its full weight as it arrives, so margin
/// never jumps when an event enters the horizon. The result is never negative.
pub fn volatility_time(
    now_ns: u64,
    horizon_days: u32,
    weekend_weight: Fixed,
    events: &[CalendarEvent],
) -> Result<Fixed, FixedError> {
    let today = day_of(now_ns);
    let mut total = Fixed::ZERO;
    for offset in 0..horizon_days {
        total = total.checked_add(day_weight(today.saturating_add(offset), weekend_weight))?;
    }

    let horizon_ns = u64::from(horizon_days).saturating_mul(NS_PER_DAY);
    let horizon_end = now_ns.saturating_add(horizon_ns);
    for event in events {
        if event.time_ns < now_ns || event.time_ns >= horizon_end {
            continue;
        }
        let remaining =
            Fixed::from_ratio(i128::from(event.time_ns - now_ns), i128::from(horizon_ns))?;
        let ramp = Fixed::ONE.checked_sub(remaining)?;
        let extra = event
            .weight
            .checked_sub(Fixed::ONE)?
            .checked_mul(day_weight(day_of(event.time_ns), weekend_weight))?
            .checked_mul(ramp)?;
        total = total.checked_add(extra)?;
    }

    Ok(total.max(Fixed::ZERO))
}

/// `k_eff = k x global_k`. The staleness factor of section 3 joins in phase 2.
pub fn effective_multiplier(k: Fixed, global_k: Fixed) -> Result<Fixed, FixedError> {
    k.checked_mul(global_k)
}

fn mean(values: &[Fixed]) -> Result<Fixed, FixedError> {
    let mut sum = Fixed::ZERO;
    for value in values {
        sum = sum.checked_add(*value)?;
    }
    let count = i128::try_from(values.len()).map_err(|_| FixedError::Overflow)?;
    sum.checked_div_int(count)
}

#[cfg(test)]
mod tests {
    use core::slice;

    use super::{
        effective_multiplier, estimate, normalized_squared_returns, volatility_time,
        EWMA_SEED_RETURNS,
    };
    use crate::risk::{
        calendar::{CalendarEvent, NS_PER_DAY},
        fixed::{Fixed, SCALE},
        history::PricePoint,
    };

    /// Monday 2026-11-09, as days since the epoch.
    const MONDAY: u32 = 20_766;

    fn fx(raw_fraction_of_one: i128) -> Fixed {
        Fixed::from_raw(raw_fraction_of_one)
    }

    #[test]
    fn returns_are_normalized_by_weekend_volatility_time() {
        // Friday -> Monday: Saturday and Sunday at 0.3, Monday at 1 => elapsed 1.6.
        let friday = MONDAY - 3;
        let points = [
            PricePoint {
                date: friday,
                price: Fixed::from_int(100),
            },
            PricePoint {
                date: MONDAY,
                price: Fixed::from_int(102),
            },
        ];
        let fx_weekend = fx(300_000_000_000);
        let fx_returns = normalized_squared_returns(&points, fx_weekend).unwrap();
        let crypto_returns = normalized_squared_returns(&points, Fixed::ONE).unwrap();

        // ln(1.02) = 0.019802627296..., squared = 0.000392143...
        let squared = Fixed::from_raw(1_020_000_000_000).ln().unwrap();
        let squared = squared.checked_mul(squared).unwrap();
        assert_eq!(
            fx_returns,
            vec![squared.checked_div(fx(1_600_000_000_000)).unwrap()]
        );
        assert_eq!(
            crypto_returns,
            vec![squared.checked_div(Fixed::from_int(3)).unwrap()]
        );
    }

    #[test]
    fn ewma_matches_hand_computation() {
        // 20 seed returns of 0.0001 and one of 0.0004, lambda 0.94:
        // variance = 0.94 x 0.0001 + 0.06 x 0.0004 = 0.000118
        let mut returns = vec![fx(SCALE / 10_000); EWMA_SEED_RETURNS];
        returns.push(fx(4 * SCALE / 10_000));
        let lambda = fx(940_000_000_000);
        let estimate = estimate(&returns, lambda, fx(750_000_000_000))
            .unwrap()
            .unwrap();
        assert_eq!(estimate.sigma_ewma, fx(118_000_000).sqrt().unwrap());
        // Long-run mean = (20 x 0.0001 + 0.0004) / 21 = 0.000114285714...
        let long_run_variance = fx(24 * SCALE / 10_000).checked_div_int(21).unwrap();
        assert_eq!(estimate.sigma_long_run, long_run_variance.sqrt().unwrap());
        // EWMA above 0.75 x long run: the floor does not bind.
        assert_eq!(estimate.sigma_used, estimate.sigma_ewma);
        assert_eq!(estimate.returns, 21);
    }

    #[test]
    fn floor_binds_after_a_calm_period() {
        // A volatile seed followed by a long calm stretch pulls EWMA well below the long-run level.
        let mut returns = vec![fx(SCALE / 100); EWMA_SEED_RETURNS];
        returns.extend(vec![fx(SCALE / 1_000_000); 200]);
        let floor_ratio = fx(750_000_000_000);
        let estimate = estimate(&returns, fx(940_000_000_000), floor_ratio)
            .unwrap()
            .unwrap();
        let floor = floor_ratio.checked_mul(estimate.sigma_long_run).unwrap();
        assert!(estimate.sigma_ewma < floor);
        assert_eq!(estimate.sigma_used, floor);
    }

    #[test]
    fn too_few_returns_give_no_estimate() {
        let returns = vec![Fixed::ONE; EWMA_SEED_RETURNS];
        assert_eq!(estimate(&returns, Fixed::ONE, Fixed::ONE).unwrap(), None);
    }

    fn event(time_ns: u64, weight: i64) -> CalendarEvent {
        CalendarEvent {
            id: 0,
            time_ns,
            weight: Fixed::from_int(weight),
            label: "ECB".to_owned(),
        }
    }

    #[test]
    fn volatility_time_counts_weekends_and_ramps_events() {
        let monday_ns = u64::from(MONDAY) * NS_PER_DAY;
        let fx_weekend = fx(300_000_000_000);

        // Monday to Sunday: five weekdays at 1, two weekend days at 0.3.
        assert_eq!(
            volatility_time(monday_ns, 7, fx_weekend, &[]).unwrap(),
            fx(5_600_000_000_000)
        );

        let horizon_days = 2;
        let horizon_ns = 2 * NS_PER_DAY;
        let base = volatility_time(monday_ns, horizon_days, fx_weekend, &[]).unwrap();
        assert_eq!(base, Fixed::from_int(2));

        // Weight 3 on a weekday: extra = 2 x ramp.
        let at_edge = event(monday_ns + horizon_ns, 3);
        let halfway = event(monday_ns + horizon_ns / 2, 3);
        let now = event(monday_ns, 3);
        let past = event(monday_ns - 1, 3);
        let vt = |e: &CalendarEvent| {
            volatility_time(monday_ns, horizon_days, fx_weekend, slice::from_ref(e)).unwrap()
        };
        assert_eq!(
            vt(&at_edge),
            base,
            "an event at the horizon edge adds nothing"
        );
        assert_eq!(
            vt(&halfway),
            Fixed::from_int(3),
            "halfway adds half of the extra weight"
        );
        assert_eq!(
            vt(&now),
            Fixed::from_int(4),
            "an event now adds its full extra weight"
        );
        assert_eq!(vt(&past), base, "a past event adds nothing");
    }

    #[test]
    fn multipliers_compose_multiplicatively() {
        assert_eq!(
            effective_multiplier(fx(1_500_000_000_000), Fixed::from_int(2)).unwrap(),
            Fixed::from_int(3)
        );
    }
}
