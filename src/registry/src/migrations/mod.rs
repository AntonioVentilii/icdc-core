//! One-shot, quarantined stable-state migrations for the registry canister.
//!
//! These types decode stable state written by registry builds **before**
//! `Series::resolution` existed. Candid cannot decode a missing non-optional
//! field, so the historical stable-state schemas are mirrored here using
//! [`LegacySeries`] (the pre-`resolution` series shape) and mapped onto the
//! current types via [`shared::migrations::upgrade_series`], backfilling
//! `resolution` from the description.
//!
//! # Retirement
//!
//! Delete this module once every deployed registry has been upgraded past the
//! release that introduced `Series::resolution` (no pre-resolution blob can then
//! exist). See `docs/ai/migrations.md` for the convention.

use std::collections::BTreeMap;

use candid::Principal;
use shared::{
    migrations::{upgrade_series, LegacySeries},
    types::{Engine, EngineId, Group, GroupId, Oracle, Series, SeriesId, SocialLimits},
};

/// Pre-`resolution` mirror of `StableStateV4` (latest pre-resolution schema).
#[derive(candid::CandidType, serde::Deserialize)]
pub struct LegacyStableStateV4 {
    pub series: BTreeMap<SeriesId, LegacySeries>,
    pub oracles: BTreeMap<String, Oracle>,
    pub groups: BTreeMap<GroupId, Group>,
    pub next_group_id: u64,
    pub social_creation_log: BTreeMap<Principal, Vec<u64>>,
    pub social_limits: SocialLimits,
    pub engines: BTreeMap<EngineId, Engine>,
    pub next_engine_id: u64,
}

/// Pre-`resolution` mirror of `StableStateV3` (pre-engines).
#[derive(candid::CandidType, serde::Deserialize)]
pub struct LegacyStableStateV3 {
    pub series: BTreeMap<SeriesId, LegacySeries>,
    pub oracles: BTreeMap<String, Oracle>,
    pub creators: BTreeMap<Principal, bool>,
    pub groups: BTreeMap<GroupId, Group>,
    pub next_group_id: u64,
    pub forkers: BTreeMap<Principal, bool>,
    pub social_creation_log: BTreeMap<Principal, Vec<u64>>,
    pub social_limits: SocialLimits,
}

/// Pre-`resolution` mirror of `StableStateV2` (pre-forkers/social).
#[derive(candid::CandidType, serde::Deserialize)]
pub struct LegacyStableStateV2 {
    pub series: BTreeMap<SeriesId, LegacySeries>,
    pub oracles: BTreeMap<String, Oracle>,
    pub creators: BTreeMap<Principal, bool>,
    pub groups: BTreeMap<GroupId, Group>,
    pub next_group_id: u64,
}

/// Backfills `resolution` across an entire legacy series map.
#[must_use]
pub fn upgrade_series_map(old: BTreeMap<SeriesId, LegacySeries>) -> BTreeMap<SeriesId, Series> {
    old.into_iter()
        .map(|(id, series)| (id, upgrade_series(series)))
        .collect()
}

#[cfg(test)]
mod tests {
    use candid::{decode_one, encode_one};
    use shared::{
        migrations::NO_RESOLUTION_PLACEHOLDER,
        types::{BalanceDomain, Description, PayoffType, PayoutUnit, TradingAccess},
    };

    use super::*;
    use crate::memory::StableStateV5;

    fn legacy_series(id: &str, description: Description) -> LegacySeries {
        LegacySeries {
            series_id: SeriesId::from(id.to_owned()),
            underlying: "ICP".to_owned(),
            expiry_ns: 1_000,
            payoff_type: PayoffType::Binary,
            strike: None,
            price_precision: 8,
            payout_unit: PayoutUnit::usd(),
            outcomes: None,
            oracle_source: "oracle".to_owned(),
            creator: Principal::anonymous(),
            created_at_ns: 0,
            title: "t".to_owned(),
            description,
            icon_url: None,
            banner_url: None,
            balance_domain: BalanceDomain::Settlement,
            trading_access: vec![TradingAccess::Open],
            engine_id: None,
            forked_from: None,
            locale: None,
        }
    }

    /// Encodes a pre-resolution V4 state, decodes it through the legacy mirror,
    /// and confirms `resolution` is backfilled while other fields survive.
    #[test]
    fn v4_blob_round_trips_and_backfills_resolution() {
        let mut series = BTreeMap::new();
        series.insert(
            SeriesId::from("with_desc".to_owned()),
            legacy_series("with_desc", Description::plain("Settles at expiry")),
        );
        series.insert(
            SeriesId::from("empty_desc".to_owned()),
            legacy_series("empty_desc", Description::plain("")),
        );

        let state = LegacyStableStateV4 {
            series,
            oracles: BTreeMap::new(),
            groups: BTreeMap::new(),
            next_group_id: 7,
            social_creation_log: BTreeMap::new(),
            social_limits: SocialLimits::default(),
            engines: BTreeMap::new(),
            next_engine_id: 3,
        };

        let bytes = encode_one(state).unwrap();

        // The current (post-resolution) schema must NOT decode a pre-resolution
        // V4 blob: candid rejects the missing compulsory `Series.resolution`.
        // This is what makes the legacy fallback + backfill necessary.
        assert!(decode_one::<StableStateV5>(&bytes).is_err());

        let decoded: LegacyStableStateV4 = decode_one(&bytes).unwrap();

        assert_eq!(decoded.next_group_id, 7);
        assert_eq!(decoded.next_engine_id, 3);

        let upgraded = upgrade_series_map(decoded.series);
        assert_eq!(
            upgraded[&SeriesId::from("with_desc".to_owned())]
                .resolution
                .clause,
            "Settles at expiry"
        );
        assert_eq!(
            upgraded[&SeriesId::from("empty_desc".to_owned())]
                .resolution
                .clause,
            NO_RESOLUTION_PLACEHOLDER
        );
    }
}
