//! One-shot, quarantined state-migration helpers shared across canisters.
//!
//! # Why this module exists
//!
//! Canister state is persisted with candid (`stable_save`/`stable_restore`).
//! Candid record decoding **fails on a missing non-optional field**, so adding a
//! compulsory field to a persisted type (here: `Series::resolution`) would brick
//! decoding of state written by older builds. The fix is to decode old state
//! through a *legacy* shadow type that matches the old shape, then map it onto
//! the current type while backfilling the new field.
//!
//! # Quarantine / retirement
//!
//! Everything here is **transient**. The `*_resolution_*` items below bridge the
//! pre-`resolution` schema to the `resolution`-bearing schema. They can be
//! deleted once every deployed canister has been upgraded past the release that
//! introduced `Series::resolution` and no stable blob predating it can exist.
//! Keep migration code in `migrations/` modules (never mixed into live `api/`
//! domains) precisely so it stays easy to find and remove.
//!
//! See `docs/ai/migrations.md` for the full convention.

use candid::{CandidType, Principal};
use serde::Deserialize;

use crate::types::{
    BalanceDomain, Description, EngineId, Outcome, PayoffType, PayoutUnit, Price, Resolution,
    Series, SeriesId, TradingAccess,
};

/// Placeholder clause written when a legacy series has no usable description.
pub const NO_RESOLUTION_PLACEHOLDER: &str = "no resolution data";

/// The pre-`resolution` shape of [`Series`], used **only** to decode stable
/// state written before `Series::resolution` existed. Field names and types
/// mirror the historical `Series` exactly (minus `resolution`) so candid can
/// decode legacy blobs into it.
#[derive(CandidType, Deserialize, Clone, Debug)]
pub struct LegacySeries {
    pub series_id: SeriesId,
    pub underlying: String,
    pub expiry_ns: u64,
    pub payoff_type: PayoffType,
    pub strike: Option<Price>,
    pub price_precision: u8,
    pub payout_unit: PayoutUnit,
    pub outcomes: Option<Vec<Outcome>>,
    pub oracle_source: String,
    pub creator: Principal,
    pub created_at_ns: u64,
    pub title: String,
    pub description: Description,
    pub icon_url: Option<String>,
    pub banner_url: Option<String>,
    pub balance_domain: BalanceDomain,
    pub trading_access: Vec<TradingAccess>,
    pub engine_id: Option<EngineId>,
    pub forked_from: Option<SeriesId>,
    pub locale: Option<String>,
}

/// Backfill rule: derive a [`Resolution`] from a series' existing description.
///
/// Copies the plain description verbatim, or falls back to
/// [`NO_RESOLUTION_PLACEHOLDER`] when the description is empty/whitespace-only.
#[must_use]
pub fn resolution_from_description(description: &Description) -> Resolution {
    let clause = if description.plain.trim().is_empty() {
        NO_RESOLUTION_PLACEHOLDER.to_owned()
    } else {
        description.plain.clone()
    };
    Resolution::new(clause)
}

/// Upgrades a decoded [`LegacySeries`] to the current [`Series`], backfilling
/// `resolution` from the description. All other fields are preserved verbatim,
/// so `series_id` (and every economic parameter) is unchanged.
#[must_use]
pub fn upgrade_series(old: LegacySeries) -> Series {
    let resolution = resolution_from_description(&old.description);
    Series {
        series_id: old.series_id,
        underlying: old.underlying,
        expiry_ns: old.expiry_ns,
        payoff_type: old.payoff_type,
        strike: old.strike,
        price_precision: old.price_precision,
        payout_unit: old.payout_unit,
        outcomes: old.outcomes,
        oracle_source: old.oracle_source,
        creator: old.creator,
        created_at_ns: old.created_at_ns,
        title: old.title,
        description: old.description,
        resolution,
        icon_url: old.icon_url,
        banner_url: old.banner_url,
        balance_domain: old.balance_domain,
        trading_access: old.trading_access,
        engine_id: old.engine_id,
        forked_from: old.forked_from,
        locale: old.locale,
    }
}

#[cfg(test)]
mod tests {
    use candid::{decode_one, encode_one};

    use super::*;

    fn legacy(description: Description) -> LegacySeries {
        LegacySeries {
            series_id: SeriesId::from("s".to_owned()),
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

    #[test]
    fn non_empty_description_is_copied_into_clause() {
        let r = resolution_from_description(&Description::plain("settles at expiry"));
        assert_eq!(r.clause, "settles at expiry");
    }

    #[test]
    fn empty_description_yields_placeholder() {
        assert_eq!(
            resolution_from_description(&Description::plain("")).clause,
            NO_RESOLUTION_PLACEHOLDER
        );
    }

    #[test]
    fn whitespace_only_description_yields_placeholder() {
        assert_eq!(
            resolution_from_description(&Description::plain("   \n\t")).clause,
            NO_RESOLUTION_PLACEHOLDER
        );
    }

    #[test]
    fn upgrade_preserves_fields_and_backfills_resolution() {
        let old = legacy(Description::plain("desc"));
        let id_before = old.series_id.clone();
        let new = upgrade_series(old);
        assert_eq!(new.series_id, id_before);
        assert_eq!(new.title, "t");
        assert_eq!(new.description.plain, "desc");
        assert_eq!(new.resolution.clause, "desc");
    }

    #[test]
    fn legacy_blob_decodes_through_shadow_type() {
        // A blob encoded from the legacy shape must decode into `LegacySeries`
        // (the basis of the migration), proving the shadow type matches.
        let bytes = encode_one(legacy(Description::plain("d"))).unwrap();

        // ...and the current `Series` schema must NOT decode it: candid rejects
        // the missing compulsory `resolution` field. This locks in the gotcha
        // that motivates the migration.
        assert!(decode_one::<Series>(&bytes).is_err());

        let decoded: LegacySeries = decode_one(&bytes).unwrap();
        assert_eq!(decoded.title, "t");
    }
}
