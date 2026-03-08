use shared::types::{asset::errors::AssetError, Asset};

use crate::{
    assets::{
        asset::params::{AssetBalanceOfParams, AssetTransferFromParams, AssetTransferParams},
        evm, icrc,
    },
    memory::CONFIG,
};

/// A unified handler for different types of assets (e.g., ICRC-1, ICRC-2).
pub enum AssetHandler {
    /// Handler for ICRC compatible tokens.
    Icrc(icrc::IcrcHandler),
    /// Handler for EVM compatible tokens.
    Evm(evm::EvmHandler),
}

impl AssetHandler {
    /// Retrieves the balance of a specific account for the given asset.
    pub async fn balance_of(&self, params: AssetBalanceOfParams<'_>) -> Result<u128, AssetError> {
        match self {
            AssetHandler::Icrc(h) => h.balance_of(params).await,
            AssetHandler::Evm(h) => h.balance_of(params).await,
        }
    }

    /// Transfers a specified amount of the asset from one account to another.
    pub async fn transfer(&self, params: AssetTransferParams<'_>) -> Result<u128, AssetError> {
        match self {
            AssetHandler::Icrc(h) => h.transfer(params).await,
            AssetHandler::Evm(h) => h.transfer(params).await,
        }
    }

    /// Transfers a specified amount of the asset from a spender's allowance.
    pub async fn transfer_from(
        &self,
        params: AssetTransferFromParams<'_>,
    ) -> Result<u128, AssetError> {
        match self {
            AssetHandler::Icrc(h) => h.transfer_from(params).await,
            AssetHandler::Evm(h) => h.transfer_from(params).await,
        }
    }

    /// Retrieves the transfer fee for the given asset.
    #[allow(dead_code)]
    pub async fn get_fee(&self, asset: &Asset) -> Result<u128, AssetError> {
        match self {
            AssetHandler::Icrc(h) => h.get_fee(asset).await,
            AssetHandler::Evm(h) => h.get_fee(asset).await,
        }
    }
}

/// Returns the appropriate [`AssetHandler`] for a given [`Asset`].
pub fn get_handler(asset: &Asset) -> Result<AssetHandler, AssetError> {
    match asset {
        Asset::Icrc(_) => Ok(AssetHandler::Icrc(icrc::IcrcHandler)),
        Asset::NativeEvm(_) | Asset::Erc20(_) => {
            let config = CONFIG.with(|c| c.borrow().clone());

            Ok(AssetHandler::Evm(evm::EvmHandler::new(
                config.evm_rpc,
                config.signer_canister,
            )))
        }
    }
}
