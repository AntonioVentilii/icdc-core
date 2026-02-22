use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct CanisterStatus {
    pub version: String,
    pub cycles_balance: u128,
    pub memory_usage_bytes: u64,
    pub heap_memory_usage_bytes: u64,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum PayoffType {
    Binary,
    Call,
    Put,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct Series {
    pub series_id: String,
    pub underlying: String,
    pub expiry: u64,
    pub payoff_type: PayoffType,
    pub strike: Option<u64>,
    pub settlement_asset: String,
    pub oracle_source: String,
}
impl Series {
    pub fn generate_id(
        underlying: &str,
        expiry: u64,
        payoff_type: &PayoffType,
        strike: Option<u64>,
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(underlying.as_bytes());
        hasher.update(expiry.to_be_bytes());
        hasher.update(match payoff_type {
            PayoffType::Binary => b"binary" as &[u8],
            PayoffType::Call => b"call" as &[u8],
            PayoffType::Put => b"put" as &[u8],
        });
        if let Some(s) = strike {
            hasher.update(s.to_be_bytes());
        }
        hex::encode(hasher.finalize())
    }
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct Position {
    pub user: Principal,
    pub series_id: String,
    pub net_qty: i128,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct MarginAccount {
    pub user: Principal,
    pub collateral_balance: u128,
    pub required_margin: u128,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum EventType {
    OrderPlaced,
    Executed,
    Settled,
    Liquidated,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct Event {
    pub event_id: u64,
    pub clearing_id: Principal,
    pub series_id: String,
    pub user: Principal,
    pub qty: i128,
    pub price: u64,
    pub event_type: EventType,
    pub timestamp: u64,
}
