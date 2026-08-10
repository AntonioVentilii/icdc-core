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
    /// Optional timestamp in nanoseconds since UNIX epoch at which trading opens.
    ///
    /// `None` means the series is tradeable from the moment it is registered —
    /// the historical behavior, and the canonical way to express "live now".
    /// `Some(t)` schedules the series: it is listed and discoverable immediately
    /// but trading is rejected until `t`. The window is inclusive at the open
    /// (`now >= start_ns` is live) and exclusive at the close (`now < expiry_ns`
    /// is unexpired), so a series is tradeable over `[start_ns, expiry_ns)`.
    ///
    /// Participates in `series_id` hashing (see [`Series::generate_id`]): two
    /// series that differ only in their trading window are distinct contracts,
    /// so a scheduled series can never silently collide with an already-live one
    /// and inherit a start date its creator did not ask for. To keep that
    /// guarantee meaningful `None` is the only encoding of "already live" —
    /// `add_series` rejects a `start_ns` at or before the current time, so the
    /// same market cannot exist under two ids.
    pub start_ns: Option<u64>,
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

        // Emitted only when a start is set, so unscheduled series (`None`) hash
        // byte-identically to how they did before `start_ns` existed. That keeps
        // every already-registered id stable without a domain-separator bump.
        if let Some(start_ns) = params.start_ns {
            hasher.update(b"|START|");
            hasher.update(start_ns.to_be_bytes());
        }

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

    /// Returns where `now` falls in this series' trading window.
    ///
    /// The window is `[start_ns, expiry_ns)` — inclusive at the open so the
    /// series is live at exactly the announced instant, exclusive at the close
    /// to match the pre-existing expiry semantics (a series expires *at*
    /// `expiry_ns`, see [`ListSeriesParams::matches_expiry`]).
    ///
    /// Expiry takes precedence over the start: because `start_ns < expiry_ns` is
    /// enforced at registration, any `now` at or past expiry is also past the
    /// start, so [`SeriesStatus::Expired`] is never ambiguous.
    #[must_use]
    pub fn status(&self, now: u64) -> SeriesStatus {
        if now >= self.expiry_ns {
            SeriesStatus::Expired
        } else if self.start_ns.is_some_and(|start| now < start) {
            SeriesStatus::Upcoming
        } else {
            SeriesStatus::Live
        }
    }

    /// Returns true when `now` falls inside this series' trading window.
    ///
    /// This is the registry's half of the "currently tradeable" predicate. It is
    /// purely a function of the clock and the series' own window — settlement
    /// state and trading access are owned elsewhere and checked separately.
    #[must_use]
    pub fn is_tradeable_at(&self, now: u64) -> bool {
        matches!(self.status(now), SeriesStatus::Live)
    }

    /// Returns true when this series carries the minimal structural data its
    /// payoff model needs to be resolved.
    ///
    /// A market can be past expiry yet not meaningfully resolvable: a `Call`/`Put`
    /// with no `strike` has no reference to settle against — the clearing
    /// canister's `get_unit_payoff` returns `MissingStrike` for it — and a
    /// `Categorical` with no declared `outcomes` has nothing to resolve to. A
    /// resolution solver should skip such a market rather than treat it as due
    /// work. `Binary` needs only the settlement price the oracle supplies at
    /// resolution, so it is always structurally resolvable.
    ///
    /// This is a pure function of the series — the settlement price itself is
    /// provided by the oracle at resolution time, not stored here — so it reports
    /// only whether the *contract* is resolvable, not whether an oracle value is
    /// available yet.
    #[must_use]
    pub fn has_resolvable_payoff(&self) -> bool {
        match self.payoff_type {
            PayoffType::Binary => true,
            PayoffType::Call | PayoffType::Put => self.strike.is_some(),
            PayoffType::Categorical => self.outcomes.as_ref().is_some_and(|o| !o.is_empty()),
        }
    }
}

/// Where a series sits in its trading window at a given instant.
///
/// Derived from `start_ns`/`expiry_ns` rather than stored, so it can never drift
/// out of sync with the timestamps it describes. Settlement state is deliberately
/// absent: it is owned by the clearing canister, and folding it in here would
/// make the registry's answer depend on state it does not hold.
#[derive(CandidType, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeriesStatus {
    /// `start_ns` is set and still in the future — listed, but not yet tradeable.
    Upcoming,
    /// Inside the trading window: at or past `start_ns`, strictly before `expiry_ns`.
    Live,
    /// At or past `expiry_ns`.
    Expired,
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
    /// The trading-window open, if the series is scheduled. See [`Series::start_ns`].
    pub start_ns: Option<u64>,
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
    /// The provided `start_ns` is at or before the registry's current time.
    ///
    /// A series that is already live must be registered with `start_ns: None`,
    /// which is the single canonical encoding of "tradeable immediately".
    /// Allowing a past start would let the same market exist under two ids.
    StartNotInFuture,
    /// The provided `start_ns` is at or after `expiry_ns`, leaving no window in
    /// which the series could ever be traded.
    StartNotBeforeExpiry,
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
    /// Optional timestamp at which trading opens. See [`Series::start_ns`].
    ///
    /// Omit (`None`) for a series that is tradeable immediately. When set, it
    /// must be strictly in the future and strictly before `expiry_ns`.
    pub start_ns: Option<u64>,
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
    /// When `Some(true)`, return only series that are inside their trading
    /// window right now — at or past `start_ns` (when set) *and* strictly before
    /// `expiry_ns`. `Some(false)` and `None` apply no filtering.
    ///
    /// This is strictly narrower than [`Self::only_unexpired`], which admits
    /// scheduled series whose start has not arrived yet. `only_unexpired` is
    /// deliberately left alone rather than redefined: callers that already rely
    /// on it keep their exact current results, and a frontend that wants to show
    /// an "upcoming" tab needs both answers, not one merged into the other.
    ///
    /// Like the expiry cutoff, the clock is the canister's own server-side
    /// `time()`.
    pub tradeable_now: Option<bool>,
    /// When `Some(true)`, return only series that are **due for resolution**: at
    /// or past `expiry_ns`, carrying a non-empty resolution clause, and with a
    /// payoff model that has the data required to settle (see
    /// [`Series::has_resolvable_payoff`]). `Some(false)` and `None` apply no
    /// filtering.
    ///
    /// This is the exact candidate set a resolution solver iterates: markets that
    /// have expired but not yet settled and can actually be resolved. It is the
    /// mirror image of [`Self::tradeable_now`] on the expiry axis (expired rather
    /// than live) and lets the caller fetch only the due subset instead of paging
    /// the whole registry and discarding most of it. Settlement state itself is
    /// owned by the clearing canister, so a caller still subtracts the
    /// already-settled ids (see `clearing.list_settled_series`) from this set.
    ///
    /// Like the other clock-dependent filters, the expiry cutoff uses the
    /// canister's own server-side `time()`.
    pub due: Option<bool>,
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

    /// Returns true if `series` satisfies the `tradeable_now` filter relative to
    /// `now` (nanoseconds since the UNIX epoch).
    ///
    /// Separate from [`Self::matches_expiry`] for the same reason that one is
    /// separate from [`Self::matches`]: the cutoff depends on the caller's notion
    /// of "now", and the registry supplies its own `time()` so it stays
    /// server-authoritative.
    ///
    /// When the filter is unset or `Some(false)`, every series passes.
    #[must_use]
    pub fn matches_tradeable_now(&self, series: &Series, now: u64) -> bool {
        if self.tradeable_now == Some(true) {
            series.is_tradeable_at(now)
        } else {
            true
        }
    }

    /// Returns true if `series` satisfies the `due` filter relative to `now`
    /// (nanoseconds since the UNIX epoch).
    ///
    /// A series is due when it is at or past `expiry_ns` (the same
    /// [`SeriesStatus::Expired`] boundary the rest of the system uses — expiry is
    /// inclusive), carries a non-empty resolution clause, and has a resolvable
    /// payoff (see [`Series::has_resolvable_payoff`]). Kept separate from
    /// [`Self::matches`] because the expiry half depends on the caller's notion of
    /// "now"; the registry supplies its own `time()` so the cutoff stays
    /// server-authoritative.
    ///
    /// When the filter is unset or `Some(false)`, every series passes.
    #[must_use]
    pub fn matches_due(&self, series: &Series, now: u64) -> bool {
        if self.due == Some(true) {
            now >= series.expiry_ns
                && !series.resolution.clause.trim().is_empty()
                && series.has_resolvable_payoff()
        } else {
            true
        }
    }

    /// Applies every clock-dependent filter at once.
    ///
    /// Convenience for call sites that must not forget one of them: adding a
    /// further time-based filter later extends this and every caller picks it up.
    #[must_use]
    pub fn matches_time(&self, series: &Series, now: u64) -> bool {
        self.matches_expiry(series, now)
            && self.matches_tradeable_now(series, now)
            && self.matches_due(series, now)
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
        series::{is_valid_locale, ListSeriesParams, Outcome, PaginationParams, SeriesStatus},
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
            start_ns: None,
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
            start_ns: None,
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
            start_ns: None,
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
            start_ns: None,
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
            start_ns: None,
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
            start_ns: None,
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
            start_ns: None,
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
            start_ns: None,
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
            start_ns: None,
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
            start_ns: None,
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
            start_ns: None,
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
            start_ns: None,
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
            start_ns: None,
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
            start_ns: None,
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
            start_ns: None,
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
            start_ns: None,
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
                start_ns: None,
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
            start_ns: None,
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

    // --- start_ns / trading window ---------------------------------------

    fn windowed_series(start_ns: Option<u64>, expiry_ns: u64) -> Series {
        let mut s = series_with_id("s");
        s.start_ns = start_ns;
        s.expiry_ns = expiry_ns;
        s
    }

    /// Pins the id of an unscheduled series to a literal.
    ///
    /// `start_ns: None` must hash byte-identically to how it did before the field
    /// existed, otherwise every already-registered series would change id on
    /// upgrade. This golden value predates `start_ns` — if a change to
    /// `generate_id` breaks it, that change is not backward compatible.
    #[test]
    fn unscheduled_series_id_is_unchanged_by_start_ns() {
        let payoff_type = PayoffType::Call;
        let strike = Some(Price::new(100, 8));
        let payout_unit = PayoutUnit::usd();

        let id = Series::generate_id(&SeriesIdParams {
            underlying: "ICP",
            expiry_ns: 1_735_689_600,
            start_ns: None,
            payoff_type: &payoff_type,
            strike: strike.as_ref(),
            settlement_cap: None,
            price_precision: 8,
            payout_unit: &payout_unit,
            outcomes: None,
            oracle_source: "coingecko",
            balance_domain: BalanceDomain::Settlement,
            forked_from: None,
            fork_caller: None,
            fork_index: None,
        });

        assert_eq!(
            id.as_str(),
            "908081d593ebc096c1392345c40997ac22d0f5f958854bd37c9c9eaeb198d7ec"
        );
    }

    #[test]
    fn start_ns_changes_the_series_id() {
        let payoff_type = PayoffType::Binary;
        let payout_unit = PayoutUnit::usd();
        let params = |start_ns| SeriesIdParams {
            underlying: "ICP",
            expiry_ns: 5_000,
            start_ns,
            payoff_type: &payoff_type,
            strike: None,
            settlement_cap: None,
            price_precision: 8,
            payout_unit: &payout_unit,
            outcomes: None,
            oracle_source: "coingecko",
            balance_domain: BalanceDomain::Settlement,
            forked_from: None,
            fork_caller: None,
            fork_index: None,
        };

        let unscheduled = Series::generate_id(&params(None));
        let early = Series::generate_id(&params(Some(1_000)));
        let late = Series::generate_id(&params(Some(2_000)));

        assert_ne!(unscheduled, early, "None must not collide with a start");
        assert_ne!(early, late, "different starts must be different contracts");
    }

    #[test]
    fn status_is_live_when_no_start_is_set() {
        let series = windowed_series(None, 1_000);

        assert_eq!(series.status(0), SeriesStatus::Live);
        assert_eq!(series.status(999), SeriesStatus::Live);
    }

    /// The window is inclusive at the open and exclusive at the close: a series
    /// is live at exactly `start_ns` and expired at exactly `expiry_ns`.
    #[test]
    fn status_boundaries_are_inclusive_open_exclusive_close() {
        let series = windowed_series(Some(100), 200);

        assert_eq!(series.status(99), SeriesStatus::Upcoming);
        assert_eq!(series.status(100), SeriesStatus::Live, "live at the open");
        assert_eq!(series.status(199), SeriesStatus::Live);
        assert_eq!(
            series.status(200),
            SeriesStatus::Expired,
            "expired at the close"
        );
        assert_eq!(series.status(201), SeriesStatus::Expired);
    }

    #[test]
    fn tradeable_now_filter_excludes_upcoming_series() {
        let upcoming = windowed_series(Some(100), 200);
        let live = windowed_series(None, 200);

        let params = ListSeriesParams {
            tradeable_now: Some(true),
            ..Default::default()
        };

        assert!(!params.matches_tradeable_now(&upcoming, 50));
        assert!(params.matches_tradeable_now(&upcoming, 100));
        assert!(params.matches_tradeable_now(&live, 50));
    }

    /// `only_unexpired` deliberately keeps its old, wider meaning: a scheduled
    /// series that has not opened yet is still unexpired.
    #[test]
    fn only_unexpired_still_admits_upcoming_series() {
        let upcoming = windowed_series(Some(100), 200);

        let params = ListSeriesParams {
            only_unexpired: Some(true),
            ..Default::default()
        };

        assert!(params.matches_expiry(&upcoming, 50));
        assert!(
            params.matches_time(&upcoming, 50),
            "no tradeable_now filter set"
        );
    }

    #[test]
    fn matches_time_applies_both_filters() {
        let upcoming = windowed_series(Some(100), 200);

        let params = ListSeriesParams {
            only_unexpired: Some(true),
            tradeable_now: Some(true),
            ..Default::default()
        };

        assert!(!params.matches_time(&upcoming, 50), "not open yet");
        assert!(params.matches_time(&upcoming, 150), "inside the window");
        assert!(!params.matches_time(&upcoming, 250), "expired");
    }

    // --- due filter -------------------------------------------------------

    /// `has_resolvable_payoff` must dereference exactly the fields the clearing
    /// canister's payoff formula needs: nothing extra for `Binary`, a `strike`
    /// for `Call`/`Put`, and at least one outcome for `Categorical`.
    #[test]
    fn has_resolvable_payoff_matches_required_data() {
        let mut binary = series_with_id("s");
        binary.payoff_type = PayoffType::Binary;
        binary.strike = None;
        assert!(binary.has_resolvable_payoff(), "binary needs no strike");

        let mut call_no_strike = series_with_id("s");
        call_no_strike.payoff_type = PayoffType::Call;
        call_no_strike.strike = None;
        assert!(
            !call_no_strike.has_resolvable_payoff(),
            "a call with no strike cannot settle"
        );

        let mut put_with_strike = series_with_id("s");
        put_with_strike.payoff_type = PayoffType::Put;
        put_with_strike.strike = Some(Price::new(100, 8));
        assert!(put_with_strike.has_resolvable_payoff());

        let mut categorical_empty = series_with_id("s");
        categorical_empty.payoff_type = PayoffType::Categorical;
        categorical_empty.outcomes = Some(vec![]);
        assert!(
            !categorical_empty.has_resolvable_payoff(),
            "a categorical with no outcomes cannot settle"
        );

        let mut categorical = series_with_id("s");
        categorical.payoff_type = PayoffType::Categorical;
        categorical.outcomes = Some(vec![Outcome {
            id: "yes".to_owned().into(),
            title: "Yes".to_owned(),
            description: None,
            icon_url: None,
        }]);
        assert!(categorical.has_resolvable_payoff());
    }

    /// A due series is expired (expiry is inclusive), resolvable, and carries a
    /// resolution clause.
    #[test]
    fn due_filter_selects_expired_resolvable_series() {
        let series = windowed_series(None, 200); // Binary, clause "r", no strike needed

        let params = ListSeriesParams {
            due: Some(true),
            ..Default::default()
        };

        assert!(!params.matches_due(&series, 199), "still live, not due");
        assert!(
            params.matches_due(&series, 200),
            "due at exactly expiry (inclusive)"
        );
        assert!(params.matches_due(&series, 250), "still due after expiry");
    }

    /// An expired market whose payoff cannot be settled — a call with no strike —
    /// is not due work: the solver would only hit a payoff error on it.
    #[test]
    fn due_filter_excludes_unresolvable_expired_series() {
        let mut series = windowed_series(None, 200);
        series.payoff_type = PayoffType::Call;
        series.strike = None;

        let params = ListSeriesParams {
            due: Some(true),
            ..Default::default()
        };

        assert!(
            !params.matches_due(&series, 250),
            "expired but unresolvable must be excluded"
        );
    }

    /// An expired market with a blank resolution clause is not due: there is no
    /// stated rule to resolve it by. (`add_series` rejects empty clauses, so this
    /// guards a defensive edge rather than a common state.)
    #[test]
    fn due_filter_excludes_blank_resolution_clause() {
        let mut series = windowed_series(None, 200);
        series.resolution = Resolution::new("   ");

        let params = ListSeriesParams {
            due: Some(true),
            ..Default::default()
        };

        assert!(!params.matches_due(&series, 250));
    }

    /// `due` unset or `Some(false)` filters nothing.
    #[test]
    fn due_filter_unset_admits_everything() {
        let live = windowed_series(None, 200);

        let unset = ListSeriesParams::default();
        assert!(unset.matches_due(&live, 50), "None applies no filter");

        let disabled = ListSeriesParams {
            due: Some(false),
            ..Default::default()
        };
        assert!(
            disabled.matches_due(&live, 50),
            "Some(false) applies no filter"
        );
    }

    /// Stable state written before `start_ns` existed must still decode, with the
    /// field defaulting to `None`. Candid treats an absent `opt` record field as
    /// null, which is why this needs no migration — but the guarantee is load
    /// bearing enough to pin with a test.
    #[test]
    fn legacy_blob_without_start_ns_decodes_as_none() {
        /// The current [`Series`] shape minus `start_ns`, mirroring what older
        /// canister versions wrote to stable memory.
        #[derive(candid::CandidType)]
        struct PreStartNsSeries {
            series_id: SeriesId,
            underlying: String,
            expiry_ns: u64,
            payoff_type: PayoffType,
            strike: Option<Price>,
            price_precision: u8,
            payout_unit: PayoutUnit,
            outcomes: Option<Vec<crate::types::series::Outcome>>,
            oracle_source: String,
            creator: Principal,
            created_at_ns: u64,
            title: String,
            description: Description,
            resolution: Resolution,
            icon_url: Option<String>,
            banner_url: Option<String>,
            balance_domain: BalanceDomain,
            trading_access: Vec<TradingAccess>,
            engine_id: Option<crate::types::EngineId>,
            forked_from: Option<SeriesId>,
            locale: Option<String>,
        }

        let legacy = PreStartNsSeries {
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
            description: Description::plain("d"),
            resolution: Resolution::new("r"),
            icon_url: None,
            banner_url: None,
            balance_domain: BalanceDomain::Settlement,
            trading_access: vec![TradingAccess::Open],
            engine_id: None,
            forked_from: None,
            locale: None,
        };

        let bytes = candid::encode_one(&legacy).expect("encode legacy series");
        let decoded: Series = candid::decode_one(&bytes).expect("decode into current Series");

        assert_eq!(decoded.start_ns, None);
        assert_eq!(decoded.expiry_ns, 1_000);
    }
}
