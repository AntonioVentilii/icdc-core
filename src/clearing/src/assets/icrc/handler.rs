use candid::Nat;
use ic_cdk::{
    api::canister_self,
    call::{Call, CallFailed},
};
use icrc_ledger_types::{
    icrc1::{
        account::Account,
        transfer::{TransferArg, TransferError},
    },
    icrc2::transfer_from::{TransferFromArgs, TransferFromError},
};
use num_traits::ToPrimitive as _;
use shared::types::{asset::errors::AssetError, Asset};

use crate::{
    assets::{
        asset::params::{AssetBalanceOfParams, AssetTransferFromParams, AssetTransferParams},
        types::AssetAmount,
    },
    traits::ClearingAccountExt as _,
    types::account::{AssetAccount, ExternalAssetAccount},
};

fn map_ledger_call_failed(method: &str, err: CallFailed) -> AssetError {
    match err {
        CallFailed::CallRejected(r) => AssetError::CallError {
            method: method.to_owned(),
            code: i32::try_from(r.raw_reject_code()).unwrap_or(i32::MAX),
            message: r.reject_message().to_owned(),
        },
        e => AssetError::CallError {
            method: method.to_owned(),
            code: -1,
            message: e.to_string(),
        },
    }
}

/// Implementation of [`AssetHandler`] for ICRC-1 and ICRC-2 compatible ledgers.
pub struct IcrcHandler;

impl IcrcHandler {
    /// Retrieves the balance of an ICRC account.
    pub async fn balance_of(&self, params: AssetBalanceOfParams<'_>) -> Result<u128, AssetError> {
        let ledger_id = params.asset.as_icrc()?;

        let account = resolve_account(&params.account)?;

        let response = Call::bounded_wait(*ledger_id, "icrc1_balance_of")
            .with_args(&(account,))
            .await
            .map_err(|e| map_ledger_call_failed("icrc1_balance_of", e))?;

        let (ledger_balance,) = response
            .candid_tuple::<(Nat,)>()
            .map_err(|e| AssetError::CallError {
                method: "icrc1_balance_of".to_owned(),
                code: -2,
                message: e.to_string(),
            })?;

        ledger_balance.0.to_u128().ok_or(AssetError::MathOverflow)
    }

    /// Retrieves the transfer fee of an ICRC ledger.
    pub async fn get_fee(&self, asset: &Asset) -> Result<u128, AssetError> {
        let ledger_id = asset.as_icrc()?;

        let response = Call::bounded_wait(*ledger_id, "icrc1_fee")
            .await
            .map_err(|e| map_ledger_call_failed("icrc1_fee", e))?;

        let (fee_nat,) = response.candid_tuple::<(Nat,)>().map_err(|e| AssetError::CallError {
            method: "icrc1_fee".to_owned(),
            code: -2,
            message: e.to_string(),
        })?;

        fee_nat.0.to_u128().ok_or(AssetError::MathOverflow)
    }

    /// Retrieves the symbol of an ICRC ledger.
    pub async fn get_symbol(&self, asset: &Asset) -> Result<String, AssetError> {
        let ledger_id = asset.as_icrc()?;

        let response = Call::bounded_wait(*ledger_id, "icrc1_symbol")
            .await
            .map_err(|e| map_ledger_call_failed("icrc1_symbol", e))?;

        let (symbol,) = response.candid_tuple::<(String,)>().map_err(|e| AssetError::CallError {
            method: "icrc1_symbol".to_owned(),
            code: -2,
            message: e.to_string(),
        })?;

        Ok(symbol)
    }

    /// Retrieves the decimals of an ICRC ledger.
    pub async fn get_decimals(&self, asset: &Asset) -> Result<u8, AssetError> {
        let ledger_id = asset.as_icrc()?;

        let response = Call::bounded_wait(*ledger_id, "icrc1_decimals")
            .await
            .map_err(|e| map_ledger_call_failed("icrc1_decimals", e))?;

        let (decimals,) = response.candid_tuple::<(u8,)>().map_err(|e| AssetError::CallError {
            method: "icrc1_decimals".to_owned(),
            code: -2,
            message: e.to_string(),
        })?;

        Ok(decimals)
    }

    /// Executes an ICRC-1 transfer.
    pub async fn transfer(&self, params: AssetTransferParams<'_>) -> Result<u128, AssetError> {
        let ledger_id = params.asset.as_icrc()?;

        let from_account = resolve_account(&params.from)?;

        let to_account = resolve_account(&params.to)?;

        let AssetAmount::Fixed(amount_u128) = params.amount;

        let icrc_args = TransferArg {
            from_subaccount: from_account.subaccount,
            to: to_account,
            amount: Nat::from(amount_u128),
            fee: None,
            memo: None,
            created_at_time: params.created_at_time_ns,
        };

        let response = Call::bounded_wait(*ledger_id, "icrc1_transfer")
            .with_args(&(icrc_args,))
            .await
            .map_err(|e| map_ledger_call_failed("icrc1_transfer", e))?;

        let (res,) = response
            .candid_tuple::<(Result<Nat, TransferError>,)>()
            .map_err(|e| AssetError::CallError {
                method: "icrc1_transfer".to_owned(),
                code: -2,
                message: e.to_string(),
            })?;

        match res {
            Ok(block) => block.0.to_u128().ok_or(AssetError::MathOverflow),
            Err(TransferError::Duplicate { duplicate_of }) => {
                duplicate_of.0.to_u128().ok_or(AssetError::MathOverflow)
            }
            Err(e) => Err(AssetError::TransferError(format!("{e:?}"))),
        }
    }

    /// Executes an ICRC-2 `transfer_from` call.
    pub async fn transfer_from(
        &self,
        params: AssetTransferFromParams<'_>,
    ) -> Result<u128, AssetError> {
        let ledger_id = params.asset.as_icrc()?;

        let spender_account = resolve_account(&params.spender)?;

        let from_account = resolve_account(&params.from)?;

        let to_account = resolve_account(&params.to)?;

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

        let response = Call::bounded_wait(*ledger_id, "icrc2_transfer_from")
            .with_args(&(icrc_args,))
            .await
            .map_err(|e| map_ledger_call_failed("icrc2_transfer_from", e))?;

        let (res,) = response
            .candid_tuple::<(Result<Nat, TransferFromError>,)>()
            .map_err(|e| AssetError::CallError {
                method: "icrc2_transfer_from".to_owned(),
                code: -2,
                message: e.to_string(),
            })?;

        match res {
            Ok(block) => block.0.to_u128().ok_or(AssetError::MathOverflow),
            Err(TransferFromError::Duplicate { duplicate_of }) => {
                duplicate_of.0.to_u128().ok_or(AssetError::MathOverflow)
            }
            Err(e) => Err(AssetError::TransferError(format!("{e:?}"))),
        }
    }

    /// Fetches all relevant metadata for an ICRC token.
    pub async fn get_metadata(&self, asset: &Asset) -> Result<IcrcMetadata, AssetError> {
        let symbol = self.get_symbol(asset).await?;
        let decimals = self.get_decimals(asset).await?;
        let fee = self.get_fee(asset).await?;

        Ok(IcrcMetadata {
            symbol,
            decimals,
            fee,
        })
    }
}

pub struct IcrcMetadata {
    pub symbol: String,
    pub decimals: u8,
    pub fee: u128,
}

/// Resolves a [`AssetAccount`] into a concrete [`Account`].
fn resolve_account(account: &AssetAccount) -> Result<Account, AssetError> {
    match account {
        AssetAccount::UserClearing(u) => Ok(u.clearing_account()),
        AssetAccount::CanisterMain => Ok(Account {
            owner: canister_self(),
            subaccount: None,
        }),
        AssetAccount::External(ExternalAssetAccount::Principal(principal)) => Ok(Account {
            owner: *principal,
            subaccount: None,
        }),
        AssetAccount::External(ExternalAssetAccount::Icrc { owner, subaccount }) => Ok(Account {
            owner: *owner,
            subaccount: *subaccount,
        }),
        AssetAccount::External(ExternalAssetAccount::Evm(_)) => {
            Err(AssetError::InvalidAssetForHandler)
        }
    }
}
