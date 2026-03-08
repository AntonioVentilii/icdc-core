use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};

use crate::{
    constants::{CKUSDC_LEDGER, CKUSDT_LEDGER, ICP_LEDGER, VUSD_LEDGER},
    types::asset::{errors::AssetError, Asset},
};

#[derive(
    CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
pub enum PayoutUnit {
    Fiat(FiatUnit),
    Crypto(CanonicalCryptoUnit),
    NonMonetary(NonMonetaryUnit),
}

impl PayoutUnit {
    pub fn as_id_bytes(&self) -> Vec<u8> {
        match self {
            PayoutUnit::Fiat(f) => format!("FIAT-{}", f.as_str()).into_bytes(),
            PayoutUnit::Crypto(c) => format!("CRYPTO-{}", c.as_str()).into_bytes(),
            PayoutUnit::NonMonetary(n) => format!("NONMONETARY-{}", n.as_str()).into_bytes(),
        }
    }

    pub fn usd() -> Self {
        PayoutUnit::Fiat(FiatUnit::Usd)
    }

    /// Converts the economic unit to its canonical token rail.
    pub fn to_asset(&self) -> Result<Asset, AssetError> {
        match self {
            PayoutUnit::Fiat(FiatUnit::Usd) => Principal::from_text(VUSD_LEDGER)
                .map(Asset::Icrc)
                .map_err(|_| AssetError::InvalidAssetId(VUSD_LEDGER.to_string())),
            PayoutUnit::Crypto(CanonicalCryptoUnit::Icp) => Principal::from_text(ICP_LEDGER)
                .map(Asset::Icrc)
                .map_err(|_| AssetError::InvalidAssetId(ICP_LEDGER.to_string())),
            PayoutUnit::Crypto(CanonicalCryptoUnit::Usdc) => Principal::from_text(CKUSDC_LEDGER)
                .map(Asset::Icrc)
                .map_err(|_| AssetError::InvalidAssetId(CKUSDC_LEDGER.to_string())),
            PayoutUnit::Crypto(CanonicalCryptoUnit::Usdt) => Principal::from_text(CKUSDT_LEDGER)
                .map(Asset::Icrc)
                .map_err(|_| AssetError::InvalidAssetId(CKUSDT_LEDGER.to_string())),
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
pub enum CanonicalCryptoUnit {
    Btc,
    Eth,
    Icp,
    Usdc,
    Usdt,
}

impl CanonicalCryptoUnit {
    pub fn as_str(&self) -> &'static str {
        match self {
            CanonicalCryptoUnit::Btc => "BTC",
            CanonicalCryptoUnit::Eth => "ETH",
            CanonicalCryptoUnit::Icp => "ICP",
            CanonicalCryptoUnit::Usdc => "USDC",
            CanonicalCryptoUnit::Usdt => "USDT",
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
