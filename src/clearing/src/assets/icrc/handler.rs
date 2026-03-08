use candid::Nat;
use icrc_ledger_types::{
    icrc1::{
        account::Account,
        transfer::{TransferArg, TransferError},
    },
    icrc2::transfer_from::{TransferFromArgs, TransferFromError},
};
use num_traits::ToPrimitive;
use shared::types::{asset::errors::AssetError, Asset};

use crate::{
    assets::{
        asset::params::{AssetBalanceOfParams, AssetTransferFromParams, AssetTransferParams},
        types::AssetAmount,
    },
    traits::ClearingAccountExt,
    types::account::{AssetAccount, ExternalAssetAccount},
};

/// Implementation of [`AssetHandler`] for ICRC-1 and ICRC-2 compatible ledgers.
pub struct IcrcHandler;

impl IcrcHandler {
    /// Retrieves the balance of an ICRC account.
    pub async fn balance_of(&self, params: AssetBalanceOfParams<'_>) -> Result<u128, AssetError> {
        let ledger_id = params.asset.as_icrc()?;

        let account = self.resolve_account(params.account);

        let (ledger_balance,): (Nat,) = ic_cdk::call(*ledger_id, "icrc1_balance_of", (account,))
            .await
            .map_err(|(code, msg)| AssetError::CallError {
                method: "icrc1_balance_of".to_string(),
                code: code as i32,
                message: msg,
            })?;

        ledger_balance.0.to_u128().ok_or(AssetError::MathOverflow)
    }

    /// Retrieves the transfer fee of an ICRC ledger.
    pub async fn get_fee(&self, asset: &Asset) -> Result<u128, AssetError> {
        let ledger_id = asset.as_icrc()?;

        let (fee_nat,): (Nat,) =
            ic_cdk::call(*ledger_id, "icrc1_fee", ())
                .await
                .map_err(|(code, msg)| AssetError::CallError {
                    method: "icrc1_fee".to_string(),
                    code: code as i32,
                    message: msg,
                })?;

        fee_nat.0.to_u128().ok_or(AssetError::MathOverflow)
    }

    /// Executes an ICRC-1 transfer.
    pub async fn transfer(&self, params: AssetTransferParams<'_>) -> Result<u128, AssetError> {
        let ledger_id = params.asset.as_icrc()?;

        let from_account = self.resolve_account(params.from)?;

        let to_account = self.resolve_account(params.to)?;

        let AssetAmount::Fixed(amount_u128) = params.amount;

        let icrc_args = TransferArg {
            from_subaccount: from_account.subaccount,
            to: to_account,
            amount: Nat::from(amount_u128),
            fee: None,
            memo: None,
            created_at_time: params.created_at_time_ns,
        };

        let (res,): (Result<Nat, TransferError>,) =
            ic_cdk::call(*ledger_id, "icrc1_transfer", (icrc_args,))
                .await
                .map_err(|(code, msg)| AssetError::CallError {
                    method: "icrc1_transfer".to_string(),
                    code: code as i32,
                    message: msg,
                })?;

        match res {
            Ok(block) => block.0.to_u128().ok_or(AssetError::MathOverflow),
            Err(TransferError::Duplicate { duplicate_of }) => {
                duplicate_of.0.to_u128().ok_or(AssetError::MathOverflow)
            }
            Err(e) => Err(AssetError::TransferError(format!("{:?}", e))),
        }
    }

    /// Executes an ICRC-2 transfer_from call.
    pub async fn transfer_from(
        &self,
        params: AssetTransferFromParams<'_>,
    ) -> Result<u128, AssetError> {
        let ledger_id = params.asset.as_icrc()?;

        let spender_account = self.resolve_account(params.spender)?;

        let from_account = self.resolve_account(params.from)?;

        let to_account = self.resolve_account(params.to)?;

        let AssetAmount::Fixed(amount_u128) = params.amount;

        let icrc_args = TransferFromArgs {
            spender_subaccount: spender_account.subaccount,
            from: from_account,
            to: to_account,
            amount: Nat::from(amount_u128),
            fee: None,
            memo: None,
            created_at_time: params.created_at_time_ns,
        };

        let (res,): (Result<Nat, TransferFromError>,) =
            ic_cdk::call(*ledger_id, "icrc2_transfer_from", (icrc_args,))
                .await
                .map_err(|(code, msg)| AssetError::CallError {
                    method: "icrc2_transfer_from".to_string(),
                    code: code as i32,
                    message: msg,
                })?;

        match res {
            Ok(block) => block.0.to_u128().ok_or(AssetError::MathOverflow),
            Err(TransferFromError::Duplicate { duplicate_of }) => {
                duplicate_of.0.to_u128().ok_or(AssetError::MathOverflow)
            }
            Err(e) => Err(AssetError::TransferError(format!("{:?}", e))),
        }
    }

    /// Resolves a [`AssetAccount`] into a concrete [`Account`].
    fn resolve_account(&self, account: AssetAccount) -> Result<Account, AssetError> {
        match account {
            AssetAccount::UserClearing(u) => Ok(u.clearing_account()),
            AssetAccount::CanisterMain => Ok(Account {
                owner: ic_cdk::id(),
                subaccount: None,
            }),
            AssetAccount::External(ExternalAssetAccount::Principal(principal)) => Ok(Account {
                owner: principal,
                subaccount: None,
            }),
            AssetAccount::External(ExternalAssetAccount::Icrc { owner, subaccount }) => {
                Ok(Account { owner, subaccount })
            }
            AssetAccount::External(ExternalAssetAccount::Evm(_)) => {
                Err(AssetError::InvalidAssetForHandler)
            }
        }
    }
}
