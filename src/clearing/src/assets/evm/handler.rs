use candid::{Nat, Principal};
use num_traits::ToPrimitive;
use shared::types::{
    asset::errors::AssetError,
    evm::{Chain, EvmAssetRef},
    Asset,
};

use super::client::{ChainFusionSignerClient, EvmRpcClient, RpcConfig, RpcService};
use crate::{
    assets::asset::params::{AssetBalanceOfParams, AssetTransferFromParams, AssetTransferParams},
    memory::EVM_ADDRESSES,
    types::account::{AssetAccount, ExternalAssetAccount},
};

pub struct EvmHandler {
    pub rpc_client: EvmRpcClient,
    pub signer_client: ChainFusionSignerClient,
}

impl EvmHandler {
    pub fn new(rpc_canister: Principal, signer_canister: Principal) -> Self {
        Self {
            rpc_client: EvmRpcClient::new(rpc_canister),
            signer_client: ChainFusionSignerClient::new(signer_canister),
        }
    }

    pub async fn balance_of(&self, params: AssetBalanceOfParams<'_>) -> Result<u128, AssetError> {
        let address = self.resolve_account(params.account).await?;

        match params.asset.as_evm()? {
            EvmAssetRef::Native(native) => {
                let chain =
                    Chain::try_from(native.chain_id).map_err(|_| AssetError::UnsupportedAsset)?;

                let config = self.get_rpc_config(chain);

                let res: Nat = self
                    .rpc_client
                    .get_balance(address, "latest".to_string(), config)
                    .await
                    .map_err(|e| AssetError::CallError {
                        method: "eth_getBalance".to_string(),
                        code: 0,
                        message: e,
                    })?;

                let balance = res.0.to_u128().ok_or(AssetError::MathOverflow)?;

                Ok(balance)
            }

            EvmAssetRef::Erc20(token) => {
                let chain =
                    Chain::try_from(token.chain_id).map_err(|_| AssetError::UnsupportedAsset)?;

                let config = self.get_rpc_config(chain);

                let data = format!(
                    "0x70a08231000000000000000000000000{}",
                    address.trim_start_matches("0x")
                );

                let res = self
                    .rpc_client
                    .call(
                        token.token_address.clone(),
                        data,
                        "latest".to_string(),
                        config,
                    )
                    .await
                    .map_err(|e| AssetError::CallError {
                        method: "eth_call".to_string(),
                        code: 0,
                        message: e,
                    })?;

                let balance = u128::from_str_radix(res.trim_start_matches("0x"), 16)
                    .map_err(|_| AssetError::MathOverflow)?;

                Ok(balance)
            }
        }
    }

    pub async fn transfer(&self, _params: AssetTransferParams<'_>) -> Result<u128, AssetError> {
        Err(AssetError::TransferError(
            "EVM transfer not fully implemented yet".to_string(),
        ))
    }

    pub async fn transfer_from(
        &self,
        _params: AssetTransferFromParams<'_>,
    ) -> Result<u128, AssetError> {
        Err(AssetError::TransferError(
            "EVM transferFrom not fully implemented yet".to_string(),
        ))
    }

    #[allow(dead_code)]
    pub async fn get_fee(&self, _asset: &Asset) -> Result<u128, AssetError> {
        Ok(21000) // Placeholder for ETH transfer
    }

    async fn get_or_fetch_address(&self, principal: Principal) -> Result<String, AssetError> {
        if let Some(addr) = EVM_ADDRESSES.with(|a| a.borrow().get(&principal).cloned()) {
            return Ok(addr);
        }

        let address = self
            .signer_client
            .eth_address(principal)
            .await
            .map_err(|e| AssetError::CallError {
                method: "eth_address".to_string(),
                code: 0,
                message: e,
            })?;

        EVM_ADDRESSES.with(|a| a.borrow_mut().insert(principal, address.clone()));

        Ok(address)
    }

    /// Resolves a [`AssetAccount`] into a concrete [`Account`].
    async fn resolve_account(&self, account: AssetAccount) -> Result<String, AssetError> {
        match account {
            AssetAccount::UserClearing(u) => self.get_or_fetch_address(u.principal()).await,
            AssetAccount::CanisterMain => self.get_or_fetch_address(ic_cdk::id()).await,
            AssetAccount::External(ExternalAssetAccount::Principal(principal)) => {
                self.get_or_fetch_address(principal).await
            }
            AssetAccount::External(ExternalAssetAccount::Icrc { .. }) => {
                Err(AssetError::InvalidAssetForHandler)
            }
            AssetAccount::External(ExternalAssetAccount::Evm(address)) => Ok(address),
        }
    }

    fn get_rpc_config(&self, chain: Chain) -> RpcConfig {
        let service = match chain {
            Chain::Ethereum => RpcService::EthereumMainnet,
            Chain::Base => RpcService::BaseMainnet,
            Chain::Bsc => RpcService::BscMainnet,
            Chain::Polygon => RpcService::PolygonMainnet,
        };
        RpcConfig { service }
    }
}
