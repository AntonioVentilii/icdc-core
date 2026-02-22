use candid::{CandidType, Deserialize};
use serde::Serialize;
use shared::types::PayoffType;

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct AddSeriesParams {
    pub underlying: String,
    pub expiry: u64,
    pub payoff_type: PayoffType,
    pub strike: Option<u64>,
    pub settlement_asset: String,
    pub oracle_source: String,
}
