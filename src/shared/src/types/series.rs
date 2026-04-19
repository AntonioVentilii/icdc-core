use candid::{CandidType, Principal};
use hex::encode;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    constants::{DEFAULT_SOCIAL_MAX_PER_HOUR, DEFAULT_SOCIAL_MAX_PER_USER},
    types::{
        description::Description, domain::BalanceDomain, engine::EngineId, groups::TradingAccess,
        payout::PayoutUnit, price::Price,
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

/// Parameters used to generate a unique [`SeriesId`].
pub struct SeriesIdParams<'a> {
    pub underlying: &'a str,
    pub expiry_ns: u64,
    pub payoff_type: &'a PayoffType,
    pub strike: Option<&'a Price>,
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
    /// Trading access policies for the forked series. Must be `Restricted`.
    pub trading_access: Vec<TradingAccess>,
    /// The Engine on whose behalf this fork is created.
    /// Controllers may omit this (`None`). Non-controller callers must provide
    /// a valid `EngineId` on which they hold the `Creator` role.
    pub engine_id: Option<EngineId>,
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
    /// then takes `limit` items. It returns those items (cloned) and the ID
    /// of the *next* available item (if any) to be used as a cursor in subsequent requests.
    pub fn apply<'a, I>(params: Option<&Self>, iter: I) -> (Vec<Series>, Option<SeriesId>)
    where
        I: Iterator<Item = (&'a SeriesId, &'a Series)>,
    {
        use core::cmp::min;

        let cursor = params.and_then(|p| p.cursor.as_ref());
        // Default to u64::MAX if no limit is provided ("give them all").
        let limit =
            usize::try_from(params.and_then(|p| p.limit).unwrap_or(u64::MAX)).unwrap_or(usize::MAX);

        // Pre-allocate the vector with a sensible cap (100) to prevent
        // massive allocations if the requested limit is extremely high.
        let mut items = Vec::with_capacity(min(limit, 100));
        let mut next_cursor = None;

        // Fallback: Skip items up to the cursor if the caller didn't use a range optimization.
        let mut iter = iter.skip_while(move |(id, _)| cursor.is_some_and(|c| *id <= c));

        // Collect up to 'limit' items for the current page.
        for _ in 0..limit {
            if let Some((_, s)) = iter.next() {
                items.push(s.clone());
            } else {
                // Iterator exhausted before reaching the limit.
                return (items, None);
            }
        }

        // Check if there's at least one more item to provide a 'next_cursor'.
        if let Some((id, _)) = iter.next() {
            next_cursor = Some(id.clone());
        }

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
        BalanceDomain, Description, PayoffType, PayoutUnit, Price, Series, SeriesId,
        SeriesIdParams, TradingAccess,
    };

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
            price_precision: 8,
            payout_unit: PayoutUnit::usd(),
            outcomes: None,
            oracle_source: "coingecko".to_owned(),
            creator: Principal::anonymous(),
            created_at_ns: 1_700_000_000,
            title: "Long ICP Call".to_owned(),
            description: Description::plain("A vanilla call option on ICP"),
            icon_url: None,
            banner_url: None,
            balance_domain: BalanceDomain::Settlement,
            trading_access: vec![TradingAccess::Open],
            engine_id: None,
            forked_from: None,
        };

        assert_eq!(series.title, "Long ICP Call");
        assert_eq!(series.description.plain, "A vanilla call option on ICP");
        assert_eq!(series.creator, Principal::anonymous());
        assert_eq!(series.price_precision, 8);
    }
}
