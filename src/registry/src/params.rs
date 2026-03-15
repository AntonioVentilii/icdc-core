use core::cmp::min;

use candid::{CandidType, Deserialize, Principal};
use serde::Serialize;
use shared::types::{
    BalanceDomain, Description, OracleMetadata, Outcome, PayoffType, PayoutUnit, Price, Series,
    SeriesId,
};

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

/// Input parameters for registering a new price oracle.
#[derive(CandidType, Deserialize, Clone, Debug)]
pub struct AddOracleParams {
    /// Unique identifier for the oracle (e.g., "COINGECKO").
    pub oracle_id: String,
    /// Initial information about the oracle.
    pub metadata: OracleMetadata,
    /// Initial list of authorised principals.
    pub authorized_principals: Vec<Principal>,
}

/// Input parameters for updating an existing oracle's metadata.
#[derive(CandidType, Deserialize, Clone, Debug)]
pub struct UpdateOracleMetadataParams {
    /// The unique identifier of the oracle to update.
    pub oracle_id: String,
    /// The updated metadata.
    pub metadata: OracleMetadata,
}

/// Input parameters for managing authorised principals of an oracle.
#[derive(CandidType, Deserialize, Clone, Debug)]
pub struct ManageOraclePrincipalsParams {
    /// The unique identifier of the oracle.
    pub oracle_id: String,
    /// Principals to be added to the authorised list.
    pub add_principals: Vec<Principal>,
    /// Principals to be removed from the authorised list.
    pub remove_principals: Vec<Principal>,
}

#[cfg(test)]
mod tests {
    use candid::Principal;
    use shared::types::{
        BalanceDomain, Description, PayoffType, PayoutUnit, Price, Series, SeriesId,
    };

    fn create_test_series() -> Series {
        Series {
            series_id: SeriesId::from("test-id".to_owned()),
            balance_domain: BalanceDomain::Settlement,
            underlying: "ICP".to_owned(),
            expiry_ns: 1000,
            payoff_type: PayoffType::Call,
            strike: Some(Price::new(100, 8)),
            price_precision: 8,
            payout_unit: PayoutUnit::usd(),
            oracle_source: "Coingecko".to_owned(),
            creator: Principal::anonymous(),
            created_at_ns: 0,
            title: "ICP Call".to_owned(),
            description: Description::plain("Test description"),
            outcomes: None,
            icon_url: None,
            banner_url: None,
        }
    }

    mod list_series_params {
        use super::*;
        use crate::ListSeriesParams;

        #[test]
        fn matches_underlying() {
            let series = create_test_series();
            let params = ListSeriesParams {
                underlying: Some("icp".to_owned()),
                ..Default::default()
            };
            assert!(params.matches(&series));

            let params = ListSeriesParams {
                underlying: Some("btc".to_owned()),
                ..Default::default()
            };
            assert!(!params.matches(&series));
        }

        #[test]
        fn matches_payoff_type() {
            let series = create_test_series();
            let params = ListSeriesParams {
                payoff_type: Some(PayoffType::Call),
                ..Default::default()
            };
            assert!(params.matches(&series));

            let params = ListSeriesParams {
                payoff_type: Some(PayoffType::Put),
                ..Default::default()
            };
            assert!(!params.matches(&series));
        }

        #[test]
        fn matches_oracle_partial() {
            let series = create_test_series();
            let params = ListSeriesParams {
                oracle_source: Some("GECKO".to_owned()),
                ..Default::default()
            };
            assert!(params.matches(&series));
        }

        #[test]
        fn matches_search_term() {
            let series = create_test_series();

            // Match title
            let params = ListSeriesParams {
                search_term: Some("call".to_owned()),
                ..Default::default()
            };
            assert!(params.matches(&series));

            // Match description
            let params = ListSeriesParams {
                search_term: Some("test".to_owned()),
                ..Default::default()
            };
            assert!(params.matches(&series));

            // Match ID
            let params = ListSeriesParams {
                search_term: Some("id".to_owned()),
                ..Default::default()
            };
            assert!(params.matches(&series));

            // No match
            let params = ListSeriesParams {
                search_term: Some("nomatch".to_owned()),
                ..Default::default()
            };
            assert!(!params.matches(&series));
        }
    }

    mod pagination_params {
        use shared::types::{Series, SeriesId};

        use crate::{params::tests::create_test_series, PaginationParams};

        #[test]
        fn apply_pagination_cursor() {
            let s1 = create_test_series();
            let mut s2 = s1.clone();
            s2.series_id = SeriesId::from("test-id-2".to_owned());
            let mut s3 = s1.clone();
            s3.series_id = SeriesId::from("test-id-3".to_owned());

            let items = [
                (s1.series_id.clone(), s1),
                (s2.series_id.clone(), s2),
                (s3.series_id.clone(), s3),
            ];

            let items_ref: Vec<(&SeriesId, &Series)> =
                items.iter().map(|(id, s)| (id, s)).collect();

            // No pagination
            let (result, next) = PaginationParams::apply(None, items_ref.clone().into_iter());
            assert_eq!(result.len(), 3);
            assert!(next.is_none());

            // Limit and next cursor
            let pagination = Some(PaginationParams {
                limit: Some(2),
                cursor: None,
            });
            let (result, next) =
                PaginationParams::apply(pagination.as_ref(), items_ref.clone().into_iter());
            assert_eq!(result.len(), 2);
            assert_eq!(next, Some(SeriesId::from("test-id-3".to_owned())));

            // From cursor
            let pagination = Some(PaginationParams {
                limit: Some(1),
                cursor: Some(SeriesId::from("test-id".to_owned())),
            });
            let (result, next) =
                PaginationParams::apply(pagination.as_ref(), items_ref.clone().into_iter());
            assert_eq!(result.len(), 1);
            assert_eq!(
                result.first().map(|r| r.series_id.as_str()),
                Some("test-id-2")
            );
            assert_eq!(next, Some(SeriesId::from("test-id-3".to_owned())));
        }
    }
}
