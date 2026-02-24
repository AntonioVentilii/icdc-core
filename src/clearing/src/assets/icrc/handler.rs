use candid::Nat;
use icrc_ledger_types::{
    icrc1::{
        account::Account,
        transfer::{TransferArg, TransferError},
    },
    icrc2::transfer_from::{TransferFromArgs, TransferFromError},
};
use num_traits::ToPrimitive;
use shared::types::Asset;

use crate::{
    assets::{
        asset::params::{AssetBalanceOfParams, AssetTransferFromParams, AssetTransferParams},
        types::AssetAmount,
    },
    traits::ClearingAccountExt,
    types::{account::LedgerAccount, errors::LedgerError},
};

pub struct IcrcHandler;

impl IcrcHandler {
    fn resolve_account(&self, account: LedgerAccount) -> Account {
        match account {
            LedgerAccount::UserClearing(u) => u.clearing_account(),
            LedgerAccount::CanisterMain => Account {
                owner: ic_cdk::id(),
                subaccount: None,
            },
            LedgerAccount::External(owner, subaccount) => Account { owner, subaccount },
        }
    }

    pub async fn balance_of(&self, params: AssetBalanceOfParams<'_>) -> Result<u128, LedgerError> {
        let Asset::Icrc(ledger_id) = params.asset;

        let account = self.resolve_account(params.account);

        let (ledger_balance,): (Nat,) = ic_cdk::call(*ledger_id, "icrc1_balance_of", (account,))
            .await
            .map_err(|(code, msg)| {
                LedgerError::FetchingBalanceFailed(format!("RB: {:?}: {}", code, msg))
            })?;

        ledger_balance
            .0
            .to_u128()
            .ok_or_else(|| LedgerError::TransferFailed("balance math overflow".to_string()))
    }

    async fn get_fee(&self, asset: &Asset) -> Result<u128, LedgerError> {
        let Asset::Icrc(ledger_id) = asset;

        let (fee_nat,): (Nat,) =
            ic_cdk::call(*ledger_id, "icrc1_fee", ())
                .await
                .map_err(|(code, msg)| {
                    LedgerError::FetchingFeeFailed(format!("RB: {:?}: {}", code, msg))
                })?;

        fee_nat
            .0
            .to_u128()
            .ok_or_else(|| LedgerError::TransferFailed("fee math overflow".to_string()))
    }

    pub async fn transfer(&self, params: AssetTransferParams<'_>) -> Result<u128, LedgerError> {
        let Asset::Icrc(ledger_id) = params.asset;

        let from_account = self.resolve_account(params.from);

        let to_account = self.resolve_account(params.to);

        let amount_u128 = match params.amount {
            AssetAmount::Fixed(a) => a,
            AssetAmount::DeductAll => {
                let balance = self
                    .balance_of(AssetBalanceOfParams {
                        asset: params.asset,
                        account: LedgerAccount::External(
                            from_account.owner,
                            from_account.subaccount,
                        ),
                    })
                    .await?;
                let fee = self.get_fee(params.asset).await?;
                if balance <= fee {
                    return Err(LedgerError::TransferFailed(
                        "insufficient balance for fee".to_string(),
                    ));
                }
                balance - fee
            }
        };

        let icrc_args = TransferArg {
            from_subaccount: from_account.subaccount,
            to: to_account,
            amount: Nat::from(amount_u128),
            fee: None,
            memo: None,
            created_at_time: params.created_at_time,
        };

        let (res,): (Result<Nat, TransferError>,) =
            ic_cdk::call(*ledger_id, "icrc1_transfer", (icrc_args,))
                .await
                .map_err(|(code, msg)| {
                    LedgerError::TransferFailed(format!("RB: {:?}: {}", code, msg))
                })?;

        match res {
            Ok(block) => block
                .0
                .to_u128()
                .ok_or_else(|| LedgerError::TransferFailed("block index overflow".to_string())),
            Err(TransferError::Duplicate { duplicate_of }) => duplicate_of
                .0
                .to_u128()
                .ok_or_else(|| LedgerError::TransferFailed("block index overflow".to_string())),
            Err(e) => Err(LedgerError::TransferFailed(format!("{:?}", e))),
        }
    }

    pub async fn transfer_from(
        &self,
        params: AssetTransferFromParams<'_>,
    ) -> Result<u128, LedgerError> {
        let Asset::Icrc(ledger_id) = params.asset;
        let spender_account = self.resolve_account(params.spender);
        let from_account = self.resolve_account(params.from);
        let to_account = self.resolve_account(params.to);

        let amount_u128 = match params.amount {
            AssetAmount::Fixed(a) => a,
            AssetAmount::DeductAll => {
                let balance = self
                    .balance_of(AssetBalanceOfParams {
                        asset: params.asset,
                        account: LedgerAccount::External(
                            from_account.owner,
                            from_account.subaccount,
                        ),
                    })
                    .await?;
                let fee = self.get_fee(params.asset).await?;
                if balance <= fee {
                    return Err(LedgerError::TransferFailed(
                        "insufficient balance for fee".to_string(),
                    ));
                }
                balance - fee
            }
        };

        let icrc_args = TransferFromArgs {
            spender_subaccount: spender_account.subaccount,
            from: from_account,
            to: to_account,
            amount: Nat::from(amount_u128),
            fee: None,
            memo: None,
            created_at_time: params.created_at_time,
        };

        let (res,): (Result<Nat, TransferFromError>,) =
            ic_cdk::call(*ledger_id, "icrc2_transfer_from", (icrc_args,))
                .await
                .map_err(|(code, msg)| {
                    LedgerError::TransferFailed(format!("RB: {:?}: {}", code, msg))
                })?;

        match res {
            Ok(block) => block
                .0
                .to_u128()
                .ok_or_else(|| LedgerError::TransferFailed("block index overflow".to_string())),
            Err(TransferFromError::Duplicate { duplicate_of }) => duplicate_of
                .0
                .to_u128()
                .ok_or_else(|| LedgerError::TransferFailed("block index overflow".to_string())),
            Err(e) => Err(LedgerError::TransferFailed(format!("{:?}", e))),
        }
    }
}
