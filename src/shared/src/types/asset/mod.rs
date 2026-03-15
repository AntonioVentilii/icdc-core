use core::fmt::{Display, Formatter, Result as FmtResult};

use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};

use crate::types::{
    asset::errors::AssetError,
    evm::{ErcToken, EvmAssetRef, NativeEvmAsset},
};

pub mod errors;
pub mod evm;

/// Represents a supported asset in the ICDC ecosystem.
#[derive(
    CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
pub enum Asset {
    /// An ICRC-compliant token identified by its canister [`Principal`].
    Icrc(Principal),
    /// A native asset on an EVM-compatible chain.
    NativeEvm(NativeEvmAsset),
    /// An ERC-20 token on an EVM-compatible chain.
    Erc20(ErcToken),
}

impl Asset {
    /// Returns the ledger canister if this asset is an ICRC token.
    pub fn as_icrc(&self) -> Result<&Principal, AssetError> {
        match self {
            Self::Icrc(ledger_id) => Ok(ledger_id),
            _ => Err(AssetError::InvalidAssetForHandler),
        }
    }

    /// Returns a reference to the underlying EVM asset, whether it's a native asset or an ERC
    /// token.
    pub fn as_evm(&self) -> Result<EvmAssetRef<'_>, AssetError> {
        match self {
            Self::NativeEvm(a) => Ok(EvmAssetRef::Native(a)),
            Self::Erc20(t) => Ok(EvmAssetRef::Erc20(t)),
            Self::Icrc(_) => Err(AssetError::InvalidAssetForHandler),
        }
    }
}

impl Display for Asset {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::Icrc(p) => write!(f, "ICRC-{}", p.to_text()),
            Self::NativeEvm(n) => write!(f, "NATIVE-{}", n.chain_id),
            Self::Erc20(t) => write!(f, "ERC20-{}-{}", t.chain_id, t.token_address),
        }
    }
}
