use shared::types::Asset;

use crate::{
    assets::{
        asset::params::{AssetBalanceOfParams, AssetTransferFromParams, AssetTransferParams},
        icrc::handler,
    },
    types::errors::LedgerError,
};

pub enum AssetHandler {
    Icrc(handler::IcrcHandler),
}

impl AssetHandler {
    pub async fn balance_of(&self, params: AssetBalanceOfParams<'_>) -> Result<u128, LedgerError> {
        match self {
            AssetHandler::Icrc(h) => h.balance_of(params).await,
        }
    }

    pub async fn transfer(&self, params: AssetTransferParams<'_>) -> Result<u128, LedgerError> {
        match self {
            AssetHandler::Icrc(h) => h.transfer(params).await,
        }
    }

    pub async fn transfer_from(
        &self,
        params: AssetTransferFromParams<'_>,
    ) -> Result<u128, LedgerError> {
        match self {
            AssetHandler::Icrc(h) => h.transfer_from(params).await,
        }
    }

    pub async fn get_fee(&self, asset: &Asset) -> Result<u128, LedgerError> {
        match self {
            AssetHandler::Icrc(h) => h.get_fee(asset).await,
        }
    }
}

pub fn get_handler(asset: &Asset) -> Result<AssetHandler, LedgerError> {
    match asset {
        Asset::Icrc(_) => Ok(AssetHandler::Icrc(handler::IcrcHandler)),
    }
}
