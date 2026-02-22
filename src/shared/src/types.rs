use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::constants::{CKUSDC_LEDGER, ICP_LEDGER};

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum Asset {
    Icrc(Principal),
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum SettlementAsset {
    Icp,
    CkUsdc,
}
impl SettlementAsset {
    pub fn as_id_bytes(&self) -> &'static [u8] {
        match self {
            SettlementAsset::Icp => b"ICP",
            SettlementAsset::CkUsdc => b"ckUSDC",
        }
    }

    pub fn to_asset(&self) -> Asset {
        match self {
            SettlementAsset::Icp => Asset::Icrc(Principal::from_text(ICP_LEDGER).unwrap()),
            SettlementAsset::CkUsdc => Asset::Icrc(Principal::from_text(CKUSDC_LEDGER).unwrap()),
        }
    }
}

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
impl PayoffType {
    pub fn as_id_bytes(&self) -> &'static [u8] {
        match self {
            PayoffType::Binary => b"BINARY",
            PayoffType::Call => b"CALL",
            PayoffType::Put => b"PUT",
        }
    }
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct Series {
    pub series_id: String,
    pub underlying: String,
    pub expiry: u64,
    pub payoff_type: PayoffType,
    pub strike: Option<u64>,
    pub settlement_asset: SettlementAsset,
    pub oracle_source: String,
}
impl Series {
    pub fn generate_id(
        underlying: &str,
        expiry: u64,
        payoff_type: &PayoffType,
        strike: Option<u64>,
        settlement_asset: &SettlementAsset,
        oracle_source: &str,
    ) -> String {
        let mut hasher = Sha256::new();

        // 🔐 Domain separator (versioned for future upgrades)
        hasher.update(b"DERIV_SERIES_V1");

        // Explicit field separators to avoid ambiguity
        hasher.update(b"|UNDERLYING|");
        hasher.update(underlying.as_bytes());

        hasher.update(b"|EXPIRY|");
        hasher.update(expiry.to_be_bytes());

        hasher.update(b"|PAYOFF|");
        hasher.update(payoff_type.as_id_bytes());

        hasher.update(b"|STRIKE|");
        match strike {
            Some(s) => hasher.update(s.to_be_bytes()),
            None => hasher.update(b"NONE"),
        }

        hasher.update(b"|SETTLEMENT|");
        hasher.update(settlement_asset.as_id_bytes());

        hasher.update(b"|ORACLE|");
        hasher.update(oracle_source.as_bytes());

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
    pub balances: Vec<(Asset, u128)>, // (Asset, Balance)
    pub required_margin: u128,
}
impl MarginAccount {
    pub fn get_balance(&self, asset: &Asset) -> u128 {
        self.balances
            .iter()
            .find(|(a, _)| a == asset)
            .map(|(_, b)| *b)
            .unwrap_or(0)
    }

    pub fn set_balance(&mut self, asset: Asset, amount: u128) {
        if let Some(pos) = self.balances.iter().position(|(a, _)| a == &asset) {
            self.balances[pos].1 = amount;
        } else {
            self.balances.push((asset, amount));
        }
    }
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
