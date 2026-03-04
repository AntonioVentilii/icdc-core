use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};

use crate::constants::{CKUSDC_LEDGER, ICP_LEDGER};

/// Represents a supported asset in the ICDC ecosystem.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Asset {
    /// An ICRC-compliant token identified by its canister [`Principal`].
    Icrc(Principal),
}

/// Supported assets for settlement of derivative contracts.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum SettlementAsset {
    /// Internet Computer Protocol (ICP) utility token.
    Icp,
    /// Chain-key USDC (ckUSDC) stablecoin.
    CkUsdc,
}
impl SettlementAsset {
    /// Returns the unique identifier bytes used for ID generation.
    pub fn as_id_bytes(&self) -> &'static [u8] {
        match self {
            SettlementAsset::Icp => b"ICP",
            SettlementAsset::CkUsdc => b"ckUSDC",
        }
    }

    /// Converts the settlement asset to its generic [`Asset`] representation.
    pub fn to_asset(&self) -> Asset {
        match self {
            SettlementAsset::Icp => Asset::Icrc(Principal::from_text(ICP_LEDGER).unwrap()),
            SettlementAsset::CkUsdc => Asset::Icrc(Principal::from_text(CKUSDC_LEDGER).unwrap()),
        }
    }
}
