use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};

use crate::{
    constants::VUSD_LEDGER,
    types::asset::{errors::AssetError, Asset},
};

#[derive(
    CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
pub enum PayoutUnit {
    Fiat(FiatUnit),
    Asset(Asset),
    NonMonetary(NonMonetaryUnit),
}

impl PayoutUnit {
    pub fn as_id_bytes(&self) -> Vec<u8> {
        match self {
            PayoutUnit::Fiat(f) => format!("FIAT-{}", f.as_str()).into_bytes(),
            PayoutUnit::Asset(a) => format!("ASSET-{}", a.to_string().to_uppercase()).into_bytes(),
            PayoutUnit::NonMonetary(n) => format!("NONMONETARY-{}", n.as_str()).into_bytes(),
        }
    }

    pub fn usd() -> Self {
        PayoutUnit::Fiat(FiatUnit::Usd)
    }

    /// Converts the economic unit to its canonical token rail.
    pub fn to_asset(&self) -> Result<Asset, AssetError> {
        match self {
            PayoutUnit::Asset(a) => Ok(a.clone()),
            PayoutUnit::Fiat(FiatUnit::Usd) => Principal::from_text(VUSD_LEDGER)
                .map(Asset::Icrc)
                .map_err(|_| AssetError::InvalidAssetId(VUSD_LEDGER.to_string())),
            _ => Err(AssetError::UnsupportedAsset),
        }
    }
}

#[derive(
    CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
pub enum FiatUnit {
    Usd,
    Eur,
    Gbp,
    Chf,
}

impl FiatUnit {
    pub fn as_str(&self) -> &'static str {
        match self {
            FiatUnit::Usd => "USD",
            FiatUnit::Eur => "EUR",
            FiatUnit::Gbp => "GBP",
            FiatUnit::Chf => "CHF",
        }
    }
}

#[derive(
    CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
pub enum NonMonetaryUnit {
    Points,
}

impl NonMonetaryUnit {
    pub fn as_str(&self) -> &'static str {
        match self {
            NonMonetaryUnit::Points => "POINTS",
        }
    }
}
