use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::constants::{CKUSDC_LEDGER, ICP_LEDGER};

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
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

#[derive(
    CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
pub struct SeriesId(String);
// impl SeriesId {
//     pub fn new(value: String) -> Self {
//         Self(value)
//     }
//
//     pub fn as_str(&self) -> &str {
//         &self.0
//     }
// }
// impl Deref for SeriesId {
//     type Target = str;
//
//     fn deref(&self) -> &Self::Target {
//         &self.0
//     }
// }
impl From<String> for SeriesId {
    fn from(value: String) -> Self {
        Self(value)
    }
}
// impl From<&str> for SeriesId {
//     fn from(value: &str) -> Self {
//         Self(value.to_string())
//     }
// }

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
    pub series_id: SeriesId,
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
    ) -> SeriesId {
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

        let series_id = hex::encode(hasher.finalize());

        series_id.into()
    }
}
