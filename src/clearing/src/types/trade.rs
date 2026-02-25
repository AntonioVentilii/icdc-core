use candid::{CandidType, Deserialize};
use serde::Serialize;

/// A unique identifier for a matched trade.
#[derive(
    CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
pub struct TradeId(String);

/// A unique identifier for a position transfer operation.
#[derive(
    CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
pub struct TransferId(String);
