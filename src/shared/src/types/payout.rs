use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};

use crate::{
    constants::{ICP_LEDGER, VUSD_LEDGER},
    types::asset::Asset,
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

    /// Converts the economic unit to its canonical ICDP token rail.
    pub fn to_asset(&self) -> Asset {
        match self {
            PayoutUnit::Fiat(FiatUnit::Usd) => {
                Asset::Icrc(Principal::from_text(VUSD_LEDGER).expect("Invalid vUSD principal"))
            }
            PayoutUnit::Crypto(CanonicalCryptoUnit::Icp) => {
                Asset::Icrc(Principal::from_text(ICP_LEDGER).expect("Invalid ICP principal"))
            }
            _ => {
                // For other units, we might not have a default rail yet.
                // This will be expanded as we add more canonical rails.
                panic!("No canonical asset rail for {:?}", self);
            }
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
}

impl CanonicalCryptoUnit {
    pub fn as_str(&self) -> &'static str {
        match self {
            CanonicalCryptoUnit::Btc => "BTC",
            CanonicalCryptoUnit::Eth => "ETH",
            CanonicalCryptoUnit::Icp => "ICP",
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
