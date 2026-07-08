use candid::{CandidType, Principal};
use hex::encode;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    constants::{DEFAULT_SOCIAL_MAX_PER_HOUR, DEFAULT_SOCIAL_MAX_PER_USER, MAX_LOCALE_LEN},
    types::{
        description::Description, domain::BalanceDomain, engine::EngineId, groups::TradingAccess,
        payout::PayoutUnit, price::Price, resolution::Resolution,
    },
};

/// A unique identifier for a derivative series.
/// Encapsulates a hex-encoded string derived from series parameters.
#[derive(
    CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
pub struct SeriesId(String);
impl From<String> for SeriesId {
    fn from(value: String) -> Self {
        Self(value)
    }
}
impl SeriesId {
    /// Returns the inner string representation of the series ID.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A unique identifier for an outcome in a categorical market.
#[derive(
    CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
pub struct OutcomeId(String);
impl From<String> for OutcomeId {
    fn from(value: String) -> Self {
        Self(value)
    }
}
impl OutcomeId {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Metadata for a specific outcome in a categorical market.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    /// The unique identifier of the outcome.
    pub id: OutcomeId,
    /// A short title for the outcome (e.g., "Yes", "No", "Team A").
    pub title: String,
    /// An optional detailed description of the outcome.
    pub description: Option<Description>,
    /// An optional icon URL for the outcome.
    pub icon_url: Option<String>,
}

/// Defines the payoff structure for a derivative contract.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum PayoffType {
    /// A fixed payoff if the condition is met (all-or-nothing).
    Binary,
    /// Payoff based on the positive difference between underlying price and strike.
    Call,
    /// Payoff based on the positive difference between strike and underlying price.
    Put,
    /// A categorical market with multiple mutually exclusive outcomes.
    Categorical,
    /// A linear (delta-one) payoff `net_qty · (S_T − F)`: outright forwards,
    /// non-deliverable forwards (NDFs), and dated futures. The agreed forward
    /// rate `F` is the trade price (captured in reserved margin, like an option
    /// premium); settlement is bounded to `[0, settlement_cap]` to keep the
    /// system solvent without a running liquidation engine. See ADR 0001.
    Linear,
}
impl PayoffType {
    /// Returns the unique identifier bytes used for ID generation.
    #[must_use]
    pub fn as_id_bytes(&self) -> &'static [u8] {
        match self {
            PayoffType::Binary => b"BINARY",
            PayoffType::Call => b"CALL",
            PayoffType::Put => b"PUT",
            PayoffType::Categorical => b"CATEGORICAL",
            PayoffType::Linear => b"LINEAR",
        }
    }
}

/// Input data for settling a series.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum SettlementInput {
    /// Final price for scalar markets (Binary, Call, Put).
    Price(Price),
    /// Resolved winner for categorical markets.
    Outcome(OutcomeId),
}

/// Defines a specific derivative series (contract).
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct Series {
    /// Unique identifier computed from series parameters.
    pub series_id: SeriesId,
    /// The underlying asset ticker or identifier (e.g., "ICP/USD").
    pub underlying: String,
    /// Expiry timestamp in nanoseconds since UNIX epoch.
    pub expiry_ns: u64,
    /// The mathematical payoff model used for this series.
    pub payoff_type: PayoffType,
    /// Target price for options, if applicable.
    pub strike: Option<Price>,
    /// Upper settlement bound for `Linear` series (forwards/NDF/futures): the
    /// fixing is clamped to `[0, settlement_cap]` at settlement and short margin
    /// is reserved as `cap − entry`. Compulsory for `Linear`, `None` otherwise.
    /// Additive `opt` field — no state migration required (see ADR 0001).
    pub settlement_cap: Option<Price>,
    /// The canonical number of decimals used for prices and strikes in this series.
    pub price_precision: u8,
    /// The unit in which the contract payoff is expressed.
    pub payout_unit: PayoutUnit,
    /// The defined outcomes for categorical markets (ordered).
    pub outcomes: Option<Vec<Outcome>>,
    /// The identifier of the oracle providing the settlement data.
    pub oracle_source: String,
    /// The principal identifier of the series creator.
    pub creator: Principal,
    /// Timestamp of series creation in nanoseconds since UNIX epoch.
    pub created_at_ns: u64,
    /// A short, descriptive title for the series.
    pub title: String,
    /// A detailed description of the series.
    pub description: Description,
    /// The settlement terms describing how this market resolves.
    ///
    /// Compulsory metadata: every series carries resolution terms. Like
    /// `title`/`description`/`locale`, it does NOT participate in `series_id`
    /// hashing (see [`Series::generate_id`]), so the same economic contract
    /// keeps a single id regardless of its resolution wording.
    pub resolution: Resolution,
    /// An optional icon URL for the market.
    pub icon_url: Option<String>,
    /// An optional banner URL for the market.
    pub banner_url: Option<String>,
    /// The domain this market belongs to (e.g. Playground, Settlement).
    pub balance_domain: BalanceDomain,
    /// The set of trading access policies governing who may trade this series.
    /// **Must never be empty** — every series carries at least one policy.
    ///
    /// Evaluated as a logical OR: a caller is authorized if **any** policy
    /// in the list grants them access.
    ///
    /// - `[Open]` → explicitly unrestricted (default).
    /// - `[Restricted { groups }]` → only group members may trade.
    /// - Multiple policies can coexist (e.g. `[Open, Restricted { groups }]`).
    pub trading_access: Vec<TradingAccess>,
    /// The Engine that created this series, if any.
    /// `None` for series created directly by a canister controller.
    pub engine_id: Option<EngineId>,
    /// If this series was forked from another, the source series ID.
    pub forked_from: Option<SeriesId>,
    /// Optional [BCP 47](https://www.rfc-editor.org/info/bcp47) language tag
    /// describing the language of `title`, `description`, and any
    /// `outcomes[].title` / `outcomes[].description`.
    ///
    /// Examples: `"en"`, `"en-US"`, `"es"`, `"zh-Hant-HK"`.
    ///
    /// `locale` is metadata: it does NOT participate in `series_id` hashing,
    /// so the same economic contract written in different languages must
    /// collide on the same id (otherwise liquidity would fragment across
    /// localized clones of the same market).
    ///
    /// When `None`, consumers should assume the default locale `"en"` and are
    /// responsible for translating into the user's preferred locale — the
    /// canister never stores translations on-chain.
    pub locale: Option<String>,
}
impl Series {
    /// Generates a unique [`SeriesId`] based on the contract parameters.
    ///
    /// The ID is computed using a SHA-256 hash of all defining parameters,
    /// ensuring that identical series have the same ID while preventing collisions.
    /// When `forked_from` is present, uses V4 domain separator and additionally
    /// hashes `fork_caller` and `fork_index` to guarantee distinct IDs across
    /// multiple forks of the same source by the same or different callers.
    #[must_use]
    pub fn generate_id(params: &SeriesIdParams<'_>) -> SeriesId {
        let mut hasher = Sha256::new();

        let version = if params.forked_from.is_some() {
            b"DERIV_SERIES_V4" as &[u8]
        } else {
            b"DERIV_SERIES_V3" as &[u8]
        };
        hasher.update(version);

        hasher.update(b"|DOMAIN|");
        match params.balance_domain {
            BalanceDomain::Playground => hasher.update(b"PLAYGROUND"),
            BalanceDomain::Settlement => hasher.update(b"SETTLEMENT"),
            BalanceDomain::ViciXp => hasher.update(b"VICI_XP"),
            BalanceDomain::Social => hasher.update(b"SOCIAL"),
        }

        // Explicit field separators to avoid ambiguity
        hasher.update(b"|UNDERLYING|");
        hasher.update(params.underlying.as_bytes());

        hasher.update(b"|EXPIRY|");
        hasher.update(params.expiry_ns.to_be_bytes());

        hasher.update(b"|PAYOFF|");
        hasher.update(params.payoff_type.as_id_bytes());

        hasher.update(b"|STRIKE|");
        match params.strike {
            Some(p) => {
                hasher.update(p.value().to_be_bytes());
                hasher.update([p.decimals()]);
            }
            None => hasher.update(b"NONE"),
        }

        hasher.update(b"|PRECISION|");
        hasher.update([params.price_precision]);

        hasher.update(b"|PAYOUT_UNIT|");
        hasher.update(params.payout_unit.as_id_bytes());

        hasher.update(b"|OUTCOMES|");
        match params.outcomes {
            Some(list) => {
                for outcome in list {
                    hasher.update(outcome.id.as_str().as_bytes());
                    hasher.update(b",");
                }
            }
            None => hasher.update(b"NONE"),
        }

        hasher.update(b"|ORACLE|");
        hasher.update(params.oracle_source.as_bytes());

        // Appended only when present so existing (cap-less) series ids are
        // unchanged; distinguishes `Linear` series that differ only by their
        // settlement cap.
        if let Some(cap) = params.settlement_cap {
            hasher.update(b"|SETTLEMENT_CAP|");
            hasher.update(cap.value().to_be_bytes());
            hasher.update([cap.decimals()]);
        }

        if let Some(source) = params.forked_from {
            hasher.update(b"|FORKED_FROM|");
            hasher.update(source.as_str().as_bytes());

            if let Some(caller) = params.fork_caller {
                hasher.update(b"|FORK_CALLER|");
                hasher.update(caller.as_slice());
            }

            if let Some(index) = params.fork_index {
                hasher.update(b"|FORK_INDEX|");
                hasher.update(index.to_be_bytes());
            }
        }

        let series_id = encode(hasher.finalize());

        series_id.into()
    }
}

/// Validates a [BCP 47](https://www.rfc-editor.org/info/bcp47)-shaped locale tag.
///
/// This is intentionally a *shape* check, not a registry-backed lookup: the
/// canister has no business validating against the IANA language subtag
/// registry. The check ensures:
///
/// - non-empty, ASCII-only,
/// - at most [`MAX_LOCALE_LEN`] characters,
/// - matches the regex `^[A-Za-z]{2,3}(-[A-Za-z0-9]{2,8})*$` (a primary language subtag of 2 or 3
///   ASCII letters, optionally followed by `-`- separated alphanumeric subtags of 2 to 8
///   characters).
///
/// Examples accepted: `"en"`, `"en-US"`, `"zh-Hant-HK"`, `"sr-Latn-RS"`.
/// Examples rejected: `""`, `"e"`, `"en_US"`, `"english"`, `"en-"`.
#[must_use]
pub fn is_valid_locale(locale: &str) -> bool {
    if locale.is_empty() || locale.chars().count() > MAX_LOCALE_LEN {
        return false;
    }
    if !locale.is_ascii() {
        return false;
    }

    let mut subtags = locale.split('-');

    let Some(primary) = subtags.next() else {
        return false;
    };
    let primary_len = primary.len();
    if !(2..=3).contains(&primary_len) || !primary.bytes().all(|b| b.is_ascii_alphabetic()) {
        return false;
    }

    for subtag in subtags {
        let len = subtag.len();
        if !(2..=8).contains(&len) || !subtag.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return false;
        }
    }

    true
}

/// Parameters used to generate a unique [`SeriesId`].
pub struct SeriesIdParams<'a> {
    pub underlying: &'a str,
    pub expiry_ns: u64,
    pub payoff_type: &'a PayoffType,
    pub strike: Option<&'a Price>,
    /// Upper settlement bound for `Linear` series; hashed only when present.
    pub settlement_cap: Option<&'a Price>,
    pub price_precision: u8,
    pub payout_unit: &'a PayoutUnit,
    pub outcomes: Option<&'a [Outcome]>,
    pub oracle_source: &'a str,
    pub balance_domain: BalanceDomain,
    pub forked_from: Option<&'a SeriesId>,
    /// The principal that created the fork. Required when `forked_from` is `Some`.
    pub fork_caller: Option<&'a Principal>,
    /// A per-caller monotonic index to distinguish multiple forks of the same source.
    /// Required when `forked_from` is `Some`.
    pub fork_index: Option<u64>,
}

/// Errors that can occur during series-related operations.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum SeriesError {
    /// Returned when attempting to add a series that already exists.
    SeriesAlreadyExists,
    /// Returned when the provided title exceeds the maximum allowed length.
    TitleTooLong,
    /// Returned when the provided description exceeds the maximum allowed length.
    DescriptionTooLong,
    /// Returned when the resolution clause is empty (a non-empty clause is compulsory).
    ResolutionClauseEmpty,
    /// Returned when the resolution clause exceeds the maximum allowed length.
    ResolutionClauseTooLong,
    /// Returned when the caller is not authorized to add a series.
    Unauthorized,
    /// Returned when the provided payout unit is not supported by the protocol.
    UnsupportedPayoutUnit,
    /// Social reward title exceeds limit.
    RewardTitleTooLong,
    /// Social reward description exceeds limit.
    RewardDescriptionTooLong,
    /// Social reward icon URL exceeds limit.
    RewardIconUrlTooLong,
    /// Social markets must have `Restricted` trading access.
    SocialMarketMustBeRestricted,
    /// Social markets require a `NonMonetary` payout unit.
    SocialMarketRequiresNonMonetaryPayout,
    /// The caller has exceeded the hourly social market creation limit.
    SocialRateLimitExceeded,
    /// The caller has reached the maximum number of social markets allowed per user.
    SocialMaxPerUserReached,
    /// The source series specified in a fork does not exist.
    SourceSeriesNotFound,
    /// Forked series must have `Restricted` trading access.
    ForkMustBeRestricted,
    /// The caller has reached the maximum number of forks for this source series.
    ForkLimitReached,
    /// The specified Engine does not exist or the caller does not hold the required role on it.
    EngineRoleNotHeld,
    /// Non-controller callers must specify an `engine_id` for non-social markets.
    EngineIdRequired,
    /// The provided locale tag is not a valid BCP 47 shape or exceeds the length limit.
    InvalidLocale,
    /// Returned when the targeted series does not exist.
    SeriesNotFound,
    /// A `Linear` series requires a `settlement_cap`.
    LinearRequiresSettlementCap,
    /// A `Linear` series must not define categorical `outcomes`.
    LinearRejectsOutcomes,
    /// A `Linear` series must not define a `strike`: the forward rate is the
    /// trade price, not a series-fixed strike.
    LinearRejectsStrike,
    /// `settlement_cap` was provided on a non-`Linear` series.
    SettlementCapOnlyForLinear,
    /// The provided `settlement_cap` is not strictly positive.
    InvalidSettlementCap,
}

/// Input parameters for registering a new derivative series.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct AddSeriesParams {
    /// The underlying asset ticker (case-insensitive, e.g., "ICP").
    pub underlying: String,
    /// The balance domain this series belongs to.
    pub balance_domain: BalanceDomain,
    /// Expiry timestamp in nanoseconds since UNIX epoch.
    pub expiry_ns: u64,
    /// The payoff model for the series.
    pub payoff_type: PayoffType,
    /// The option strike price, if applicable.
    pub strike: Option<Price>,
    /// Upper settlement bound for `Linear` series (forwards/NDF/futures).
    /// Compulsory for `Linear`, `None` otherwise. See `Series::settlement_cap`.
    pub settlement_cap: Option<Price>,
    /// The number of decimals used for prices and strikes in this series.
    pub price_precision: u8,
    /// The unit in which the contract payoff is expressed.
    pub payout_unit: PayoutUnit,
    /// The price oracle identifier (case-insensitive, e.g., "Coingecko").
    pub oracle_source: String,
    /// A short, descriptive title for the series.
    pub title: String,
    /// A detailed description of the series.
    pub description: Description,
    /// The settlement terms describing how this market resolves.
    /// Compulsory — every new series must state how it settles.
    pub resolution: Resolution,
    /// The defined outcomes for categorical markets (ordered).
    pub outcomes: Option<Vec<Outcome>>,
    /// An optional icon URL for the market.
    pub icon_url: Option<String>,
    /// An optional banner URL for the market.
    pub banner_url: Option<String>,
    /// Initial trading access policies for the new series.
    /// **Must not be empty** — pass at least `[Open]`.
    ///
    /// Controls who may submit orders on this market once it is registered.
    /// Pass `[Restricted { groups: [g1, g2, ...] }]` to limit trading to
    /// members of those groups.
    ///
    /// If the caller passes an empty list, `add_series` will fill it with `[Open]`.
    /// Policies can be updated after creation via `update_trading_access`.
    pub trading_access: Vec<TradingAccess>,
    /// The Engine on whose behalf this series is created.
    /// Controllers may omit this (`None`). Non-controller callers must provide
    /// a valid `EngineId` on which they hold the `Creator` role.
    pub engine_id: Option<EngineId>,
    /// Optional [BCP 47](https://www.rfc-editor.org/info/bcp47) locale tag for
    /// `title`, `description`, and any `outcomes[].title`/`description`.
    ///
    /// When `None`, consumers should assume the default locale `"en"`.
    /// Translations are never stored on-chain and are the responsibility of
    /// the consumer.
    pub locale: Option<String>,
}

/// Input parameters for forking (cloning) an existing series into a restricted circle.
///
/// The forked series inherits all defining parameters from the source series but
/// gets a distinct ID (via the `forked_from` hash component) and carries a reference
/// back to the original.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct ForkSeriesParams {
    /// The source series to fork.
    pub source_series_id: SeriesId,
    /// Optional title override. Falls back to the source series title.
    pub title: Option<String>,
    /// Optional description override. Falls back to the source series description.
    pub description: Option<Description>,
    /// Optional resolution override. Falls back to the source series resolution.
    pub resolution: Option<Resolution>,
    /// Trading access policies for the forked series. Must be `Restricted`.
    pub trading_access: Vec<TradingAccess>,
    /// The Engine on whose behalf this fork is created.
    /// Controllers may omit this (`None`). Non-controller callers must provide
    /// a valid `EngineId` on which they hold the `Creator` role.
    pub engine_id: Option<EngineId>,
    /// Optional locale override. Falls back to the source series locale when
    /// `None`. See `Series.locale` for the full semantics.
    pub locale: Option<String>,
}

/// Parameters for paginating results.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, Default)]
pub struct PaginationParams {
    /// Maximum number of items to return.
    pub limit: Option<u64>,
    /// Return items strictly *after* this series id (exclusive).
    pub cursor: Option<SeriesId>,
}

impl PaginationParams {
    /// Applies cursor-based pagination to the provided iterator.
    ///
    /// The iterator MUST yield items in ascending order of their [`SeriesId`].
    /// This method skips all items up to and including the provided `cursor`,
    /// then takes `limit` items. It returns those items (cloned) and the
    /// **exclusive** cursor for the next page (if any).
    ///
    /// The returned cursor is the [`SeriesId`] of the **last item returned** in
    /// this page, not the first un-returned one. Every resume path treats the
    /// cursor as exclusive — the registry ranges with `Bound::Excluded(cursor)`
    /// and the fallback below skips `id <= cursor` — so handing back the
    /// last-returned id resumes exactly after it, dropping and repeating
    /// nothing. (Returning the first un-returned id here would skip it on the
    /// next call, losing one item per page boundary.) This matches the cursor
    /// convention used by clearing's `backfill_settlement_events` and
    /// `list_settled_series`.
    pub fn apply<'a, I>(params: Option<&Self>, iter: I) -> (Vec<Series>, Option<SeriesId>)
    where
        I: Iterator<Item = (&'a SeriesId, &'a Series)>,
    {
        use core::cmp::min;

        let cursor = params.and_then(|p| p.cursor.as_ref());
        // Default to u64::MAX if no limit is provided ("give them all").
        // Clamp to at least 1: `list_series` forwards user-supplied limits
        // unvalidated, and a limit of 0 would otherwise return an empty page
        // with no cursor, leaving a paging caller stuck with no forward
        // progress. Treat 0 as "smallest meaningful page".
        let limit = usize::try_from(params.and_then(|p| p.limit).unwrap_or(u64::MAX))
            .unwrap_or(usize::MAX)
            .max(1);

        // Pre-allocate the vector with a sensible cap (100) to prevent
        // massive allocations if the requested limit is extremely high.
        let mut items = Vec::with_capacity(min(limit, 100));
        // Id of the last item pushed this page — becomes the exclusive cursor.
        let mut last_id: Option<&SeriesId> = None;

        // Fallback: Skip items up to the cursor if the caller didn't use a range optimization.
        let mut iter = iter.skip_while(move |(id, _)| cursor.is_some_and(|c| *id <= c));

        // Collect up to 'limit' items for the current page.
        for _ in 0..limit {
            if let Some((id, s)) = iter.next() {
                items.push(s.clone());
                last_id = Some(id);
            } else {
                // Iterator exhausted before reaching the limit: no further page.
                return (items, None);
            }
        }

        // Emit the last-returned id as the next cursor only if at least one more
        // item remains; otherwise this was the final page.
        let next_cursor = iter.next().is_some().then(|| last_id.cloned()).flatten();

        (items, next_cursor)
    }
}

/// Parameters for filtering the list of registered derivative series.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, Default)]
pub struct ListSeriesParams {
    /// Filter by the underlying asset ticker (case-insensitive).
    pub underlying: Option<String>,
    /// Filter by balance domain.
    pub balance_domain: Option<BalanceDomain>,
    /// Filter by the payoff model.
    pub payoff_type: Option<PayoffType>,
    /// Filter by the strike price.
    pub strike: Option<Price>,
    /// Filter by the payout unit.
    pub payout_unit: Option<PayoutUnit>,
    /// Filter by the price oracle identifier (case-insensitive, partial match).
    pub oracle_source: Option<String>,
    /// Filter by the principal identifier of the creator.
    pub creator: Option<Principal>,
    /// Filter by a search term in the title or description (case-insensitive, partial match).
    pub search_term: Option<String>,
    /// When `Some(true)`, exclude series whose `expiry_ns` is at or before the
    /// registry's current time — i.e. return only series that are still open
    /// for trading. `Some(false)` and `None` are equivalent and apply no expiry
    /// filtering (the historical behavior).
    ///
    /// The cutoff is the canister's own `time()` evaluated server-side, so the
    /// caller cannot widen the window with a stale or forged clock. This is the
    /// expiry half of the "currently-tradeable" predicate; resolution/settlement
    /// state is owned by the clearing canister and is filtered separately by the
    /// caller (see `clearing.list_settled_series`).
    pub only_unexpired: Option<bool>,
    /// Optional pagination parameters.
    pub pagination: Option<PaginationParams>,
}

impl ListSeriesParams {
    /// Returns true if the provided series matches all defined filter criteria.
    #[must_use]
    pub fn matches(&self, series: &Series) -> bool {
        if let Some(underlying) = &self.underlying {
            if series.underlying.to_lowercase() != underlying.to_lowercase() {
                return false;
            }
        }

        if let Some(domain) = self.balance_domain {
            if series.balance_domain != domain {
                return false;
            }
        }

        if let Some(payoff_type) = &self.payoff_type {
            if &series.payoff_type != payoff_type {
                return false;
            }
        }

        if let Some(strike) = &self.strike {
            if series.strike.as_ref() != Some(strike) {
                return false;
            }
        }

        if let Some(payout_unit) = &self.payout_unit {
            if &series.payout_unit != payout_unit {
                return false;
            }
        }

        if let Some(oracle_source) = &self.oracle_source {
            if !series
                .oracle_source
                .to_lowercase()
                .contains(&oracle_source.to_lowercase())
            {
                return false;
            }
        }

        if let Some(creator) = self.creator {
            if series.creator != creator {
                return false;
            }
        }

        if let Some(search_term) = &self.search_term {
            let search_term = search_term.to_lowercase();
            let matches_title = series.title.to_lowercase().contains(&search_term);
            let matches_description = series
                .description
                .plain
                .to_lowercase()
                .contains(&search_term);
            let matches_id = series
                .series_id
                .as_str()
                .to_lowercase()
                .contains(&search_term);

            if !matches_title && !matches_description && !matches_id {
                return false;
            }
        }

        true
    }

    /// Returns true if `series` satisfies the `only_unexpired` filter relative to
    /// `now` (nanoseconds since the UNIX epoch).
    ///
    /// Kept separate from [`Self::matches`] — which is a pure function of the
    /// filter and the series — because the expiry cutoff depends on the caller's
    /// notion of "now". The registry passes its own `time()` so the cutoff is
    /// server-authoritative.
    ///
    /// A series is unexpired when its `expiry_ns` is strictly in the future.
    /// When the filter is unset or `Some(false)`, every series passes.
    #[must_use]
    pub fn matches_expiry(&self, series: &Series, now: u64) -> bool {
        if self.only_unexpired == Some(true) {
            series.expiry_ns > now
        } else {
            true
        }
    }
}

/// The result of an [`add_series`] operation.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum AddSeriesResult {
    /// Successfully registered the series with the returned [`SeriesId`].
    Ok(SeriesId),
    /// Failed to register the series.
    Err(SeriesError),
}

impl From<Result<SeriesId, SeriesError>> for AddSeriesResult {
    fn from(value: Result<SeriesId, SeriesError>) -> Self {
        match value {
            Ok(v) => AddSeriesResult::Ok(v),
            Err(e) => AddSeriesResult::Err(e),
        }
    }
}

/// Input parameters for [`update_series_metadata`](../../registry/index.html).
///
/// Edits only **non-critical, non-identity** fields of an existing series.
/// `title`, `resolution`, and every field that feeds the `series_id` hash
/// (underlying, expiry, payoff, strike, precision, payout unit, outcomes,
/// oracle source, balance domain) are intentionally **not** editable: changing
/// them would either break the series' identity or rewrite the terms a market
/// already trades on.
///
/// Each field follows a tri-state convention so a single call can leave some
/// fields untouched, set others, and clear nullable ones:
///
/// - `None` → leave the current value unchanged.
/// - `Some(value)` → replace with `value`.
/// - For the `Option<Option<_>>` fields, `Some(None)` → clear the field to `null` (e.g. remove a
///   banner).
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct UpdateSeriesMetadataParams {
    /// The series to update.
    pub series_id: SeriesId,
    /// New plain/markdown/html description. `None` leaves it unchanged.
    pub description: Option<Description>,
    /// New icon URL. `None` leaves it unchanged; `Some(None)` clears it.
    pub icon_url: Option<Option<String>>,
    /// New banner URL. `None` leaves it unchanged; `Some(None)` clears it.
    pub banner_url: Option<Option<String>>,
    /// New [BCP 47](https://www.rfc-editor.org/info/bcp47) locale tag. `None`
    /// leaves it unchanged; `Some(None)` clears it; `Some(Some(tag))` validates
    /// and sets it.
    pub locale: Option<Option<String>>,
}

/// The result of an `update_series_metadata` operation.
///
/// The success payload is boxed because [`Series`] is far larger than
/// [`SeriesError`]; candid serializes `Box<Series>` identically to `Series`.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum UpdateSeriesResult {
    /// Successfully updated; returns the series in its new state.
    Ok(Box<Series>),
    /// Failed to update the series.
    Err(SeriesError),
}

impl From<Result<Series, SeriesError>> for UpdateSeriesResult {
    fn from(value: Result<Series, SeriesError>) -> Self {
        match value {
            Ok(v) => UpdateSeriesResult::Ok(Box::new(v)),
            Err(e) => UpdateSeriesResult::Err(e),
        }
    }
}

/// A paginated page of registered series.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct SeriesPage {
    /// The list of series in this page.
    pub items: Vec<Series>,
    /// The cursor to be used for the next request, if any.
    pub next_cursor: Option<SeriesId>,
}

/// Configurable rate limits for social (non-monetary) market creation.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SocialLimits {
    /// Maximum social markets any single user may create within a rolling hour.
    pub max_per_hour: u64,
    /// Maximum total social markets any single user may create (lifetime).
    pub max_per_user: u64,
}

impl Default for SocialLimits {
    fn default() -> Self {
        Self {
            max_per_hour: DEFAULT_SOCIAL_MAX_PER_HOUR,
            max_per_user: DEFAULT_SOCIAL_MAX_PER_USER,
        }
    }
}

#[cfg(test)]
mod tests {
    use candid::Principal;

    use crate::types::{
        series::{is_valid_locale, PaginationParams},
        BalanceDomain, Description, PayoffType, PayoutUnit, Price, Resolution, Series, SeriesId,
        SeriesIdParams, TradingAccess,
    };

    /// Builds a minimal [`Series`] carrying the given id. Only the id matters
    /// for pagination tests.
    fn series_with_id(id: &str) -> Series {
        Series {
            series_id: SeriesId::from(id.to_owned()),
            underlying: "ICP".to_owned(),
            expiry_ns: 1_000,
            payoff_type: PayoffType::Binary,
            strike: None,
            settlement_cap: None,
            price_precision: 8,
            payout_unit: PayoutUnit::usd(),
            outcomes: None,
            oracle_source: "oracle".to_owned(),
            creator: Principal::anonymous(),
            created_at_ns: 0,
            title: "t".to_owned(),
            description: Description::plain("d"),
            resolution: Resolution::new("r"),
            icon_url: None,
            banner_url: None,
            balance_domain: BalanceDomain::Settlement,
            trading_access: vec![TradingAccess::Open],
            engine_id: None,
            forked_from: None,
            locale: None,
        }
    }

    /// Walking every page with a fixed `limit` must reproduce the full ordered
    /// set exactly — no item dropped at a page boundary, none repeated. This is
    /// a regression test for an off-by-one where `next_cursor` pointed at the
    /// first *un-returned* item while the resume path treats the cursor as
    /// exclusive, silently skipping that item.
    #[test]
    fn pagination_covers_every_item_without_drops_or_dupes() {
        use std::collections::BTreeMap;

        let mut store: BTreeMap<SeriesId, Series> = BTreeMap::new();
        for n in 0..10 {
            // Zero-padded so lexical order matches numeric order.
            let id = format!("s{n:02}");
            store.insert(SeriesId::from(id.clone()), series_with_id(&id));
        }
        let full: Vec<SeriesId> = store.keys().cloned().collect();

        // Try several page sizes, including ones that don't evenly divide the
        // set and one larger than the set.
        for limit in [1_u64, 2, 3, 4, 7, 10, 25] {
            let mut collected: Vec<SeriesId> = Vec::new();
            let mut cursor: Option<SeriesId> = None;

            // Hard bound on iterations to fail fast rather than hang if the
            // cursor ever fails to advance.
            for _ in 0..(full.len() + 5) {
                let params = PaginationParams {
                    limit: Some(limit),
                    cursor: cursor.clone(),
                };
                // No range pre-exclusion here, so this exercises `apply`'s own
                // `skip_while` fallback against the cursor.
                let (items, next) = PaginationParams::apply(Some(&params), store.iter());
                collected.extend(items.into_iter().map(|s| s.series_id));
                match next {
                    Some(c) => cursor = Some(c),
                    None => break,
                }
            }

            assert_eq!(
                collected, full,
                "limit={limit}: paged traversal must equal the full ordered set"
            );
        }
    }

    /// Mirrors how the registry actually resumes: it pre-ranges the store with
    /// `Bound::Excluded(cursor)` before handing the iterator to `apply`. The
    /// returned cursor must be exclusive-resume-safe through that path too —
    /// every item returned exactly once, in order.
    #[test]
    fn pagination_through_excluded_range_resume_has_no_drops() {
        use core::ops::Bound;
        use std::collections::BTreeMap;

        let mut store: BTreeMap<SeriesId, Series> = BTreeMap::new();
        for n in 0..6 {
            let id = format!("s{n}");
            store.insert(SeriesId::from(id.clone()), series_with_id(&id));
        }
        let full: Vec<SeriesId> = store.keys().cloned().collect();

        let mut collected: Vec<SeriesId> = Vec::new();
        let mut cursor: Option<SeriesId> = None;
        for _ in 0..(full.len() + 5) {
            let params = PaginationParams {
                limit: Some(2),
                cursor: cursor.clone(),
            };
            let (items, next) = match cursor.as_ref() {
                Some(c) => PaginationParams::apply(
                    Some(&params),
                    store.range((Bound::Excluded(c.clone()), Bound::Unbounded)),
                ),
                None => PaginationParams::apply(Some(&params), store.range(..)),
            };
            collected.extend(items.into_iter().map(|s| s.series_id));
            match next {
                Some(c) => cursor = Some(c),
                None => break,
            }
        }

        assert_eq!(collected, full);
    }

    /// A final page exactly filled to `limit` must not advertise a further
    /// cursor (otherwise the caller makes a spurious empty request, or worse,
    /// a buggy resume repeats/loses items).
    #[test]
    fn pagination_full_final_page_has_no_next_cursor() {
        use std::collections::BTreeMap;

        let mut store: BTreeMap<SeriesId, Series> = BTreeMap::new();
        for id in ["s0", "s1", "s2", "s3"] {
            store.insert(SeriesId::from(id.to_owned()), series_with_id(id));
        }

        // First page of 2 → cursor at the last returned id (s1), more remain.
        let (page1, cur1) = PaginationParams::apply(
            Some(&PaginationParams {
                limit: Some(2),
                cursor: None,
            }),
            store.iter(),
        );
        assert_eq!(
            page1
                .iter()
                .map(|s| s.series_id.as_str())
                .collect::<Vec<_>>(),
            vec!["s0", "s1"]
        );
        assert_eq!(cur1.as_ref().map(SeriesId::as_str), Some("s1"));

        // Second page of 2 exactly empties the set → no further cursor.
        let (page2, cur2) = PaginationParams::apply(
            Some(&PaginationParams {
                limit: Some(2),
                cursor: cur1,
            }),
            store.iter(),
        );
        assert_eq!(
            page2
                .iter()
                .map(|s| s.series_id.as_str())
                .collect::<Vec<_>>(),
            vec!["s2", "s3"]
        );
        assert!(cur2.is_none(), "exactly-filled final page must not page on");
    }

    /// A `limit` of 0 must still make forward progress (it is clamped to 1)
    /// rather than returning an empty page with no cursor, which would strand
    /// a paging caller. Walking with `limit = 0` therefore drains the whole
    /// ordered set, one item per page.
    #[test]
    fn pagination_limit_zero_is_clamped_and_makes_progress() {
        use std::collections::BTreeMap;

        let mut store: BTreeMap<SeriesId, Series> = BTreeMap::new();
        for id in ["s0", "s1", "s2"] {
            store.insert(SeriesId::from(id.to_owned()), series_with_id(id));
        }
        let full: Vec<SeriesId> = store.keys().cloned().collect();

        let mut collected: Vec<SeriesId> = Vec::new();
        let mut cursor: Option<SeriesId> = None;
        for _ in 0..(full.len() + 5) {
            let params = PaginationParams {
                limit: Some(0),
                cursor: cursor.clone(),
            };
            let (items, next) = PaginationParams::apply(Some(&params), store.iter());
            assert_eq!(items.len(), 1, "limit 0 is clamped to a single-item page");
            collected.extend(items.into_iter().map(|s| s.series_id));
            match next {
                Some(c) => cursor = Some(c),
                None => break,
            }
        }

        assert_eq!(collected, full);
    }

    #[test]
    fn generate_series_id_consistency() {
        let underlying = "ICP";
        let expiry = 1_735_689_600;
        let payoff_type = PayoffType::Call;
        let strike = Some(Price::new(100, 8));
        let precision = 8;
        let payout_unit = PayoutUnit::usd();
        let oracle_source = "coingecko";

        let id1 = Series::generate_id(&SeriesIdParams {
            underlying,
            expiry_ns: expiry,
            payoff_type: &payoff_type,
            strike: strike.as_ref(),
            settlement_cap: None,
            price_precision: precision,
            payout_unit: &payout_unit,
            outcomes: None,
            oracle_source,
            balance_domain: BalanceDomain::Settlement,
            forked_from: None,
            fork_caller: None,
            fork_index: None,
        });

        let id2 = Series::generate_id(&SeriesIdParams {
            underlying,
            expiry_ns: expiry,
            payoff_type: &payoff_type,
            strike: strike.as_ref(),
            settlement_cap: None,
            price_precision: precision,
            payout_unit: &payout_unit,
            outcomes: None,
            oracle_source,
            balance_domain: BalanceDomain::Settlement,
            forked_from: None,
            fork_caller: None,
            fork_index: None,
        });

        assert_eq!(id1, id2);
    }

    #[test]
    fn generate_series_id_different_expiry() {
        let underlying = "ICP";
        let payoff_type = PayoffType::Call;
        let strike = Some(Price::new(100, 8));
        let precision = 8;
        let payout_unit = PayoutUnit::usd();
        let oracle_source = "coingecko";

        let id1 = Series::generate_id(&SeriesIdParams {
            underlying,
            expiry_ns: 100,
            payoff_type: &payoff_type,
            strike: strike.as_ref(),
            settlement_cap: None,
            price_precision: precision,
            payout_unit: &payout_unit,
            outcomes: None,
            oracle_source,
            balance_domain: BalanceDomain::Settlement,
            forked_from: None,
            fork_caller: None,
            fork_index: None,
        });

        let id2 = Series::generate_id(&SeriesIdParams {
            underlying,
            expiry_ns: 200,
            payoff_type: &payoff_type,
            strike: strike.as_ref(),
            settlement_cap: None,
            price_precision: precision,
            payout_unit: &payout_unit,
            outcomes: None,
            oracle_source,
            balance_domain: BalanceDomain::Settlement,
            forked_from: None,
            fork_caller: None,
            fork_index: None,
        });

        assert_ne!(id1, id2);
    }

    #[test]
    fn generate_series_id_different_precision() {
        let underlying = "ICP";
        let expiry = 100;
        let payoff_type = PayoffType::Call;
        let strike = Some(Price::new(100, 8));
        let payout_unit = PayoutUnit::usd();
        let oracle_source = "coingecko";

        let id1 = Series::generate_id(&SeriesIdParams {
            underlying,
            expiry_ns: expiry,
            payoff_type: &payoff_type,
            strike: strike.as_ref(),
            settlement_cap: None,
            price_precision: 8,
            payout_unit: &payout_unit,
            outcomes: None,
            oracle_source,
            balance_domain: BalanceDomain::Settlement,
            forked_from: None,
            fork_caller: None,
            fork_index: None,
        });

        let id2 = Series::generate_id(&SeriesIdParams {
            underlying,
            expiry_ns: expiry,
            payoff_type: &payoff_type,
            strike: strike.as_ref(),
            settlement_cap: None,
            price_precision: 10,
            payout_unit: &payout_unit,
            outcomes: None,
            oracle_source,
            balance_domain: BalanceDomain::Settlement,
            forked_from: None,
            fork_caller: None,
            fork_index: None,
        });

        assert_ne!(id1, id2);
    }

    #[test]
    fn forked_series_has_different_id() {
        let underlying = "ICP";
        let expiry = 1_735_689_600;
        let payoff_type = PayoffType::Call;
        let strike = Some(Price::new(100, 8));
        let precision = 8;
        let payout_unit = PayoutUnit::usd();
        let oracle_source = "coingecko";
        let caller = Principal::from_slice(&[1]);

        let original = Series::generate_id(&SeriesIdParams {
            underlying,
            expiry_ns: expiry,
            payoff_type: &payoff_type,
            strike: strike.as_ref(),
            settlement_cap: None,
            price_precision: precision,
            payout_unit: &payout_unit,
            outcomes: None,
            oracle_source,
            balance_domain: BalanceDomain::Settlement,
            forked_from: None,
            fork_caller: None,
            fork_index: None,
        });

        let forked = Series::generate_id(&SeriesIdParams {
            underlying,
            expiry_ns: expiry,
            payoff_type: &payoff_type,
            strike: strike.as_ref(),
            settlement_cap: None,
            price_precision: precision,
            payout_unit: &payout_unit,
            outcomes: None,
            oracle_source,
            balance_domain: BalanceDomain::Settlement,
            forked_from: Some(&original),
            fork_caller: Some(&caller),
            fork_index: Some(0),
        });

        assert_ne!(original, forked);
    }

    #[test]
    fn multiple_forks_produce_distinct_ids() {
        let underlying = "ICP";
        let expiry = 1_735_689_600;
        let payoff_type = PayoffType::Call;
        let strike = Some(Price::new(100, 8));
        let precision = 8;
        let payout_unit = PayoutUnit::usd();
        let oracle_source = "coingecko";
        let caller = Principal::from_slice(&[1]);

        let original = Series::generate_id(&SeriesIdParams {
            underlying,
            expiry_ns: expiry,
            payoff_type: &payoff_type,
            strike: strike.as_ref(),
            settlement_cap: None,
            price_precision: precision,
            payout_unit: &payout_unit,
            outcomes: None,
            oracle_source,
            balance_domain: BalanceDomain::Settlement,
            forked_from: None,
            fork_caller: None,
            fork_index: None,
        });

        let fork_0 = Series::generate_id(&SeriesIdParams {
            underlying,
            expiry_ns: expiry,
            payoff_type: &payoff_type,
            strike: strike.as_ref(),
            settlement_cap: None,
            price_precision: precision,
            payout_unit: &payout_unit,
            outcomes: None,
            oracle_source,
            balance_domain: BalanceDomain::Settlement,
            forked_from: Some(&original),
            fork_caller: Some(&caller),
            fork_index: Some(0),
        });

        let fork_1 = Series::generate_id(&SeriesIdParams {
            underlying,
            expiry_ns: expiry,
            payoff_type: &payoff_type,
            strike: strike.as_ref(),
            settlement_cap: None,
            price_precision: precision,
            payout_unit: &payout_unit,
            outcomes: None,
            oracle_source,
            balance_domain: BalanceDomain::Settlement,
            forked_from: Some(&original),
            fork_caller: Some(&caller),
            fork_index: Some(1),
        });

        assert_ne!(
            fork_0, fork_1,
            "Different fork indices must produce different IDs"
        );
    }

    #[test]
    fn different_callers_produce_distinct_fork_ids() {
        let underlying = "ICP";
        let expiry = 1_735_689_600;
        let payoff_type = PayoffType::Call;
        let strike = Some(Price::new(100, 8));
        let precision = 8;
        let payout_unit = PayoutUnit::usd();
        let oracle_source = "coingecko";
        let alice = Principal::from_slice(&[1]);
        let bob = Principal::from_slice(&[2]);

        let original = Series::generate_id(&SeriesIdParams {
            underlying,
            expiry_ns: expiry,
            payoff_type: &payoff_type,
            strike: strike.as_ref(),
            settlement_cap: None,
            price_precision: precision,
            payout_unit: &payout_unit,
            outcomes: None,
            oracle_source,
            balance_domain: BalanceDomain::Settlement,
            forked_from: None,
            fork_caller: None,
            fork_index: None,
        });

        let fork_alice = Series::generate_id(&SeriesIdParams {
            underlying,
            expiry_ns: expiry,
            payoff_type: &payoff_type,
            strike: strike.as_ref(),
            settlement_cap: None,
            price_precision: precision,
            payout_unit: &payout_unit,
            outcomes: None,
            oracle_source,
            balance_domain: BalanceDomain::Settlement,
            forked_from: Some(&original),
            fork_caller: Some(&alice),
            fork_index: Some(0),
        });

        let fork_bob = Series::generate_id(&SeriesIdParams {
            underlying,
            expiry_ns: expiry,
            payoff_type: &payoff_type,
            strike: strike.as_ref(),
            settlement_cap: None,
            price_precision: precision,
            payout_unit: &payout_unit,
            outcomes: None,
            oracle_source,
            balance_domain: BalanceDomain::Settlement,
            forked_from: Some(&original),
            fork_caller: Some(&bob),
            fork_index: Some(0),
        });

        assert_ne!(
            fork_alice, fork_bob,
            "Different callers must produce different fork IDs"
        );
    }

    #[test]
    fn series_with_metadata() {
        let series = Series {
            series_id: SeriesId::from("test".to_owned()),
            underlying: "ICP".to_owned(),
            expiry_ns: 1_735_689_600,
            payoff_type: PayoffType::Call,
            strike: Some(Price::new(100, 8)),
            settlement_cap: None,
            price_precision: 8,
            payout_unit: PayoutUnit::usd(),
            outcomes: None,
            oracle_source: "coingecko".to_owned(),
            creator: Principal::anonymous(),
            created_at_ns: 1_700_000_000,
            title: "Long ICP Call".to_owned(),
            description: Description::plain("A vanilla call option on ICP"),
            resolution: Resolution::new("Settles to Coingecko ICP/USD at expiry"),
            icon_url: None,
            banner_url: None,
            balance_domain: BalanceDomain::Settlement,
            trading_access: vec![TradingAccess::Open],
            engine_id: None,
            forked_from: None,
            locale: None,
        };

        assert_eq!(series.title, "Long ICP Call");
        assert_eq!(series.description.plain, "A vanilla call option on ICP");
        assert_eq!(series.creator, Principal::anonymous());
        assert_eq!(series.price_precision, 8);
    }

    #[test]
    fn locale_does_not_affect_series_id() {
        // Derives `SeriesIdParams` from a full `Series` value. The intent is
        // to model the registry call site, which constructs hashing inputs
        // from the user-supplied series fields. Because `SeriesIdParams` has
        // no `locale` field, `Series.locale` is structurally excluded from
        // hashing. If a future change adds locale to the hashing inputs, this
        // helper will fail to compile (and the test below will catch the
        // resulting id divergence in case the field is wired through).
        fn id_from(series: &Series) -> SeriesId {
            Series::generate_id(&SeriesIdParams {
                underlying: &series.underlying,
                expiry_ns: series.expiry_ns,
                payoff_type: &series.payoff_type,
                strike: series.strike.as_ref(),
                settlement_cap: None,
                price_precision: series.price_precision,
                payout_unit: &series.payout_unit,
                outcomes: series.outcomes.as_deref(),
                oracle_source: &series.oracle_source,
                balance_domain: series.balance_domain,
                forked_from: series.forked_from.as_ref(),
                fork_caller: None,
                fork_index: None,
            })
        }

        let base = Series {
            series_id: SeriesId::from(String::new()),
            underlying: "ICP".to_owned(),
            expiry_ns: 1_735_689_600,
            payoff_type: PayoffType::Call,
            strike: Some(Price::new(100, 8)),
            settlement_cap: None,
            price_precision: 8,
            payout_unit: PayoutUnit::usd(),
            outcomes: None,
            oracle_source: "coingecko".to_owned(),
            creator: Principal::anonymous(),
            created_at_ns: 0,
            title: "Long ICP Call".to_owned(),
            description: Description::plain("EN"),
            resolution: Resolution::new("EN"),
            icon_url: None,
            banner_url: None,
            balance_domain: BalanceDomain::Settlement,
            trading_access: vec![TradingAccess::Open],
            engine_id: None,
            forked_from: None,
            locale: Some("en".to_owned()),
        };

        let mut localized = base.clone();
        localized.locale = Some("it-IT".to_owned());
        // Title/description are part of `Series` (not hashed) but localizing
        // them along with `locale` mirrors realistic usage.
        localized.title = "Long ICP Call (IT)".to_owned();
        localized.description = Description::plain("IT");

        assert_eq!(id_from(&base), id_from(&localized));
    }

    #[test]
    fn is_valid_locale_accepts_common_tags() {
        for tag in [
            "en",
            "es",
            "it",
            "fr",
            "de",
            "en-US",
            "en-GB",
            "pt-BR",
            "zh-Hant",
            "zh-Hant-HK",
            "sr-Latn-RS",
        ] {
            assert!(is_valid_locale(tag), "expected `{tag}` to be valid");
        }
    }

    #[test]
    fn is_valid_locale_rejects_malformed_tags() {
        for tag in [
            "",
            "e",                  // primary too short
            "english",            // primary too long (>3)
            "en_US",              // wrong separator
            "en-",                // trailing separator
            "en--US",             // empty subtag
            "en-U",               // subtag too short
            "en-VERYLONGREGION",  // subtag too long (>8)
            "en US",              // space disallowed
            "en-US-",             // trailing separator
            "12",                 // primary not letters
            "abcdefghijklmnopqr", // exceeds MAX_LOCALE_LEN
        ] {
            assert!(!is_valid_locale(tag), "expected `{tag}` to be rejected");
        }
    }
}
