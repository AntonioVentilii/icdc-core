use shared::types::Asset;

use crate::{
    assets::{
        asset::params::{AssetBalanceOfParams, AssetTransferFromParams, AssetTransferParams},
        icrc::handler,
    },
    types::errors::LedgerError,
};

/// A unified handler for different types of assets (e.g., ICRC-1, ICRC-2).
pub enum AssetHandler {
    /// Handler for ICRC compatible tokens.
    Icrc(handler::IcrcHandler),
}

impl AssetHandler {
    /// Retrieves the balance of a specific account for the given asset.
    pub async fn balance_of(&self, params: AssetBalanceOfParams<'_>) -> Result<u128, LedgerError> {
        match self {
            AssetHandler::Icrc(h) => h.balance_of(params).await,
        }
    }

    /// Transfers a specified amount of the asset from one account to another.
    pub async fn transfer(&self, params: AssetTransferParams<'_>) -> Result<u128, LedgerError> {
        match self {
            AssetHandler::Icrc(h) => h.transfer(params).await,
        }
    }

    /// Transfers a specified amount of the asset from a spender's allowance.
    pub async fn transfer_from(
        &self,
        params: AssetTransferFromParams<'_>,
    ) -> Result<u128, LedgerError> {
        match self {
            AssetHandler::Icrc(h) => h.transfer_from(params).await,
        }
    }

    /// Retrieves the transfer fee for the given asset.
    pub async fn get_fee(&self, asset: &Asset) -> Result<u128, LedgerError> {
        match self {
            AssetHandler::Icrc(h) => h.get_fee(asset).await,
        }
    }
}

/// Returns the appropriate [`AssetHandler`] for a given [`Asset`].
pub fn get_handler(asset: &Asset) -> Result<AssetHandler, LedgerError> {
    match asset {
        Asset::Icrc(_) => Ok(AssetHandler::Icrc(handler::IcrcHandler)),
    }
}
