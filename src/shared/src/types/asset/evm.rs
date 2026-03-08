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

    /// Returns the canonical native asset for a given chain.
    pub fn native(chain: Chain) -> Self {
        match chain {
            Chain::Ethereum | Chain::Base | Chain::Bsc | Chain::Polygon => Self {
                chain_id: chain.id(),
                decimals: 18,
            },
        }
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

    /// Returns the canonical USDC token for a given chain.
    pub fn usdc(chain: Chain) -> Self {
        match chain {
            Chain::Ethereum => Self {
                token_address: "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".to_string(),
                chain_id: chain.id(),
                decimals: 6,
            },
            Chain::Base => Self {
                token_address: "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913".to_string(),
                chain_id: chain.id(),
                decimals: 6,
            },
            Chain::Bsc => Self {
                token_address: "0x8ac76a51cc950d9822d68b83fe1ad97b32cd580d".to_string(),
                chain_id: chain.id(),
                decimals: 18,
            },
            Chain::Polygon => Self {
                token_address: "0x3c499c542cEF5E3811e1192ce70d8cC03d5c3359".to_string(),
                chain_id: chain.id(),
                decimals: 6,
            },
        }
    }

    /// Returns the canonical USDT token for a given chain.
    pub fn usdt(chain: Chain) -> Result<Self, crate::types::asset::errors::AssetError> {
        match chain {
            Chain::Ethereum => Ok(Self {
                token_address: "0xdAC17F958D2ee523a2206206994597C13D831ec7".to_string(),
                chain_id: chain.id(),
                decimals: 6,
            }),
            _ => Err(crate::types::asset::errors::AssetError::UnsupportedAsset),
        }
    }
}
impl fmt::Display for ErcToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-{}", self.token_address, self.chain_id)
    }
}

pub enum EvmAssetRef<'a> {
    Native(&'a NativeEvmAsset),
    Erc20(&'a ErcToken),
}
