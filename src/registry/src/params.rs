use candid::{CandidType, Deserialize};
use serde::Serialize;
use shared::types::{OracleMetadata, PayoffType, Series, SeriesId, SettlementAsset};

/// Input parameters for registering a new derivative series.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct AddSeriesParams {
    /// The underlying asset ticker (case-insensitive, e.g., "ICP").
    pub underlying: String,
    /// Expiry timestamp in nanoseconds since UNIX epoch.
    pub expiry_ns: u64,
    /// The payoff model for the series.
    pub payoff_type: PayoffType,
    /// The option strike price, if applicable.
    pub strike: Option<u64>,
    /// The asset in which the contract is settled.
    pub settlement_asset: SettlementAsset,
    /// The price oracle identifier (case-insensitive, e.g., "Coingecko").
    pub oracle_source: String,
    /// A short, descriptive title for the series.
    pub title: String,
    /// A detailed description of the series.
    pub description: String,
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
        let limit = params.and_then(|p| p.limit).unwrap_or(u64::MAX) as usize;

        // Pre-allocate the vector with a sensible cap (100) to prevent
        // massive allocations if the requested limit is extremely high.
        let mut items = Vec::with_capacity(std::cmp::min(limit, 100));
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
    /// Filter by the payoff model.
    pub payoff_type: Option<PayoffType>,
    /// Filter by the strike price.
    pub strike: Option<u64>,
    /// Filter by the settlement asset.
    pub settlement_asset: Option<SettlementAsset>,
    /// Filter by the price oracle identifier (case-insensitive, partial match).
    pub oracle_source: Option<String>,
    /// Filter by the principal identifier of the creator.
    pub creator: Option<candid::Principal>,
    /// Filter by a search term in the title or description (case-insensitive, partial match).
    pub search_term: Option<String>,
    /// Optional pagination parameters.
    pub pagination: Option<PaginationParams>,
}
impl ListSeriesParams {
    /// Returns true if the provided series matches all defined filter criteria.
    pub fn matches(&self, series: &Series) -> bool {
        if let Some(underlying) = &self.underlying {
            if series.underlying.to_lowercase() != underlying.to_lowercase() {
                return false;
            }
        }

        if let Some(payoff_type) = &self.payoff_type {
            if &series.payoff_type != payoff_type {
                return false;
            }
        }

        if let Some(strike) = self.strike {
            if series.strike != Some(strike) {
                return false;
            }
        }

        if let Some(settlement_asset) = &self.settlement_asset {
            if &series.settlement_asset != settlement_asset {
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
            let matches_description = series.description.to_lowercase().contains(&search_term);
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
    pub authorized_principals: Vec<candid::Principal>,
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
    pub add_principals: Vec<candid::Principal>,
    /// Principals to be removed from the authorised list.
    pub remove_principals: Vec<candid::Principal>,
}

#[cfg(test)]
mod tests {
    use candid::Principal;
    use shared::types::{PayoffType, SeriesId, SettlementAsset};

    use super::*;

    fn create_test_series() -> Series {
        Series {
            series_id: SeriesId::from("test-id".to_string()),
            underlying: "ICP".to_string(),
            expiry_ns: 1000,
            payoff_type: PayoffType::Call,
            strike: Some(100),
            settlement_asset: SettlementAsset::Icp,
            oracle_source: "Coingecko".to_string(),
            creator: Principal::anonymous(),
            created_at_ns: 0,
            title: "ICP Call".to_string(),
            description: "Test description".to_string(),
        }
    }

    mod list_series_params {
        use super::*;

        #[test]
        fn test_matches_underlying() {
            let series = create_test_series();
            let params = ListSeriesParams {
                underlying: Some("icp".to_string()),
                ..Default::default()
            };
            assert!(params.matches(&series));

            let params = ListSeriesParams {
                underlying: Some("btc".to_string()),
                ..Default::default()
            };
            assert!(!params.matches(&series));
        }

        #[test]
        fn test_matches_payoff_type() {
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
        fn test_matches_oracle_partial() {
            let series = create_test_series();
            let params = ListSeriesParams {
                oracle_source: Some("GECKO".to_string()),
                ..Default::default()
            };
            assert!(params.matches(&series));
        }

        #[test]
        fn test_matches_search_term() {
            let series = create_test_series();

            // Match title
            let params = ListSeriesParams {
                search_term: Some("call".to_string()),
                ..Default::default()
            };
            assert!(params.matches(&series));

            // Match description
            let params = ListSeriesParams {
                search_term: Some("test".to_string()),
                ..Default::default()
            };
            assert!(params.matches(&series));

            // Match ID
            let params = ListSeriesParams {
                search_term: Some("id".to_string()),
                ..Default::default()
            };
            assert!(params.matches(&series));

            // No match
            let params = ListSeriesParams {
                search_term: Some("nomatch".to_string()),
                ..Default::default()
            };
            assert!(!params.matches(&series));
        }
    }

    mod pagination_params {
        use super::*;

        #[test]
        fn test_apply_pagination_cursor() {
            let s1 = create_test_series();
            let mut s2 = s1.clone();
            s2.series_id = SeriesId::from("test-id-2".to_string());
            let mut s3 = s1.clone();
            s3.series_id = SeriesId::from("test-id-3".to_string());

            let items = vec![
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
            assert_eq!(next, Some(SeriesId::from("test-id-3".to_string())));

            // From cursor
            let pagination = Some(PaginationParams {
                limit: Some(1),
                cursor: Some(SeriesId::from("test-id".to_string())),
            });
            let (result, next) =
                PaginationParams::apply(pagination.as_ref(), items_ref.clone().into_iter());
            assert_eq!(result.len(), 1);
            assert_eq!(result[0].series_id.as_str(), "test-id-2");
            assert_eq!(next, Some(SeriesId::from("test-id-3".to_string())));
        }
    }
}
