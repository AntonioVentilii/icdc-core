use candid::{CandidType, Deserialize};
use serde::Serialize;
use shared::types::{PayoffType, Series, SettlementAsset};

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
    /// Number of items to skip.
    pub offset: Option<u64>,
}

impl PaginationParams {
    /// Applies pagination (offset and limit) to the provided iterator.
    pub fn apply<I, T>(params: Option<&Self>, iter: I) -> impl Iterator<Item = T>
    where
        I: Iterator<Item = T>,
    {
        let offset = params.and_then(|p| p.offset).unwrap_or(0) as usize;
        let limit = params.and_then(|p| p.limit).unwrap_or(u64::MAX) as usize;
        iter.skip(offset).take(limit)
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
        fn test_apply_pagination() {
            let items = vec![1, 2, 3, 4, 5];

            // No pagination
            let result: Vec<_> = PaginationParams::apply(None, items.clone().into_iter()).collect();
            assert_eq!(result, items);

            // Limit only
            let pagination = Some(PaginationParams {
                limit: Some(2),
                ..Default::default()
            });
            let result: Vec<_> =
                PaginationParams::apply(pagination.as_ref(), items.clone().into_iter()).collect();
            assert_eq!(result, vec![1, 2]);

            // Offset only
            let pagination = Some(PaginationParams {
                offset: Some(2),
                ..Default::default()
            });
            let result: Vec<_> =
                PaginationParams::apply(pagination.as_ref(), items.clone().into_iter()).collect();
            assert_eq!(result, vec![3, 4, 5]);

            // Limit and offset
            let pagination = Some(PaginationParams {
                limit: Some(2),
                offset: Some(1),
                ..Default::default()
            });
            let result: Vec<_> =
                PaginationParams::apply(pagination.as_ref(), items.clone().into_iter()).collect();
            assert_eq!(result, vec![2, 3]);
        }
    }
}
