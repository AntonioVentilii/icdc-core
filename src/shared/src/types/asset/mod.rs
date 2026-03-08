use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};

use crate::{
    constants::{CKUSDC_LEDGER, ICP_LEDGER},
    types::{
        evm::{Chain, ErcToken, NativeEvmAsset},
        helpers::{native_evm_asset, usdc_token, usdt_token},
    },
};

pub mod evm;
pub mod helpers;

/// Represents a supported asset in the ICDC ecosystem.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Asset {
    /// An ICRC-compliant token identified by its canister [`Principal`].
    Icrc(Principal),
    /// A native asset on an EVM-compatible chain.
    NativeEvm(NativeEvmAsset),
    /// An ERC-20 token on an EVM-compatible chain.
    Erc20(ErcToken),
}

/// Supported assets for settlement of derivative contracts.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SettlementAsset {
    /// Internet Computer Protocol (ICP) utility token.
    Icp,
    /// Chain-key USDC (ckUSDC) stablecoin.
    CkUsdc,
    /// Native gas token of an EVM chain (for example ETH, POL, or BNB).
    Native(Chain),
    /// USDC stablecoin on an EVM chain.
    Usdc(Chain),
    /// USDT stablecoin on an EVM chain.
    Usdt(Chain),
}
impl SettlementAsset {
    /// Returns the number of decimals for this asset.
    pub fn decimals(&self) -> u32 {
        match self {
            SettlementAsset::Icp => 8,
            SettlementAsset::CkUsdc => 6,
            SettlementAsset::Native(chain) => native_evm_asset(*chain).decimals(),
            SettlementAsset::Usdc(chain) => usdc_token(*chain).decimals(),
            SettlementAsset::Usdt(chain) => usdt_token(*chain).decimals(),
        }
    }

    /// Returns the canonical symbol for this settlement asset.
    pub fn symbol(&self) -> &'static str {
        match self {
            Self::Icp => "ICP",
            Self::CkUsdc => "ckUSDC",
            Self::Native(chain) => chain.native_symbol(),
            Self::Usdc(_) => "USDC",
            Self::Usdt(_) => "USDT",
        }
    }

    /// Returns the unique identifier bytes used for ID generation.
    pub fn as_id_bytes(&self) -> Vec<u8> {
        match self {
            Self::Icp | Self::CkUsdc => self.symbol().as_bytes().to_vec(),
            Self::Native(chain) | Self::Usdc(chain) | Self::Usdt(chain) => {
                format!("{}-{}", self.symbol(), chain.id()).into_bytes()
            }
        }
    }

    /// Converts the settlement asset to its generic [`Asset`] representation.
    pub fn to_asset(&self) -> Asset {
        match self {
            SettlementAsset::Icp => Asset::Icrc(Principal::from_text(ICP_LEDGER).unwrap()),
            SettlementAsset::CkUsdc => Asset::Icrc(Principal::from_text(CKUSDC_LEDGER).unwrap()),
            SettlementAsset::Native(chain) => Asset::NativeEvm(native_evm_asset(*chain)),
            SettlementAsset::Usdc(chain) => Asset::Erc20(usdc_token(*chain)),
            SettlementAsset::Usdt(chain) => Asset::Erc20(usdt_token(*chain)),
        }
    }
}
