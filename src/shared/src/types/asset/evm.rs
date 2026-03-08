use std::fmt;

use candid::{CandidType, Deserialize};
use serde::Serialize;

/// Unique identifier for an EVM-compatible chain.
///
/// IDs may be found on: <https://chainlist.org/>
pub type ChainId = u64;
impl From<Chain> for ChainId {
    fn from(chain: Chain) -> Self {
        chain.id()
    }
}

/// Represents a supported EVM-compatible blockchain network.
///
/// Each variant corresponds to a well-known chain and maps to its
/// canonical EVM `chain_id` as defined in the Ethereum ecosystem.
///
/// Chain IDs follow the standard used by EVM networks:
/// https://chainlist.org/
#[derive(
    CandidType, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord,
)]
pub enum Chain {
    Ethereum,
    Base,
    Bsc,
    Polygon,
}
impl Chain {
    /// Returns the canonical EVM chain ID for this network.
    pub fn id(self) -> ChainId {
        match self {
            Self::Ethereum => 1,
            Self::Base => 8453,
            Self::Bsc => 56,
            Self::Polygon => 137,
        }
    }

    /// Returns the symbol of the native gas token for this chain.
    pub fn native_symbol(self) -> &'static str {
        match self {
            Self::Ethereum => "ETH",
            Self::Base => "ETH",
            Self::Bsc => "BNB",
            Self::Polygon => "POL",
        }
    }
}
impl TryFrom<ChainId> for Chain {
    type Error = ();

    fn try_from(id: ChainId) -> Result<Self, Self::Error> {
        match id {
            1 => Ok(Self::Ethereum),
            8453 => Ok(Self::Base),
            56 => Ok(Self::Bsc),
            137 => Ok(Self::Polygon),
            _ => Err(()),
        }
    }
}

/// Represents a native asset on an EVM-compatible chain
/// (for example ETH on Ethereum/Base, POL on Polygon, or BNB on BSC).
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct NativeEvmAsset {
    /// The EVM chain where this native asset is used.
    pub chain_id: ChainId,
    /// The number of decimals the native asset uses.
    pub decimals: u8,
}
impl NativeEvmAsset {
    pub fn decimals(&self) -> u32 {
        self.decimals as u32
    }
}
impl fmt::Display for NativeEvmAsset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "native-{}", self.chain_id)
    }
}

/// Unique identifier for an ERC-20 token (contract address).
pub type ErcTokenId = String;

/// Represents an ERC-20 token on a specific EVM chain.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ErcToken {
    /// The contract address of the token.
    pub token_address: ErcTokenId,
    /// The ID of the EVM chain where the token is deployed.
    pub chain_id: ChainId,
    /// The number of decimals the token uses (e.g., 18 for ETH, 6 for USDC).
    pub decimals: u8,
}
impl ErcToken {
    pub fn decimals(&self) -> u32 {
        self.decimals as u32
    }
}
impl fmt::Display for ErcToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-{}", self.token_address, self.chain_id)
    }
}
