use ic_cdk_macros::update;
use icrc_ledger_types::icrc1::{
    account::Account,
    transfer::{TransferArg, TransferError},
};
use num_traits::ToPrimitive;
use shared::types::Asset;

// use crate::types::margin::MarginAccount;
use crate::traits::ClearingAccountExt;
use crate::{
    guards::caller_is_controller,
    memory::{MARGIN_ACCOUNTS, POSITIONS, SERIES},
    types::{
        error::ClearingError, params::SettleSeriesParams, results::SettleSeriesResult, user::User,
    },
};

#[update(guard = "caller_is_controller")]
pub async fn settle_series(params: SettleSeriesParams) -> SettleSeriesResult {
    let result: Result<(), ClearingError> =
        (async {
            let SettleSeriesParams {
                series_id,
                settlement_price,
            } = params;

            let settlement_asset_val = SERIES
                .with(|s| {
                    s.borrow()
                        .get(&series_id)
                        .map(|ser| ser.settlement_asset.to_asset())
                })
                .ok_or(ClearingError::UnsupportedLedger)?;

            let Asset::Icrc(ledger_id) = settlement_asset_val;

            let (fee_nat,): (candid::Nat,) = ic_cdk::call(ledger_id, "icrc1_fee", ())
                .await
                .map_err(|(code, msg)| {
                    ClearingError::FetchingFeeFailed(format!("icrc1_fee RB: {:?}: {}", code, msg))
                })?;

            let fee_u128: u128 = fee_nat.0.to_u128().ok_or(ClearingError::FeeMathOverflow)?;

            let positions_to_settle: Vec<(User, i128)> = POSITIONS.with(|positions| {
                let mut positions = positions.borrow_mut();

                let users: Vec<User> = positions
                    .keys()
                    .filter(|(_, sid)| *sid == series_id)
                    .map(|(u, _)| *u)
                    .collect();

                let mut settlement_data = Vec::new();

                for user in users {
                    if let Some(pos) = positions.remove(&(user, series_id.clone())) {
                        settlement_data.push((user, pos.net_qty));
                    }
                }

                settlement_data
            });

            // Split into payers and receivers (using integer arithmetic avoids f64 surprises)
            let mut payers: Vec<(User, u128)> = Vec::new();
            let mut receivers: Vec<(User, u128)> = Vec::new();
            let mut accounting_updates: Vec<(User, i128, u128)> = Vec::new();

            for (user, net_qty) in positions_to_settle {
                let payoff_i128: i128 = net_qty
                    .checked_mul(settlement_price as i128)
                    .ok_or(ClearingError::PayoffMathOverflow)?;
                let amount_u128: u128 = payoff_i128.unsigned_abs();

                if payoff_i128 < 0 {
                    payers.push((user, amount_u128));
                    accounting_updates.push((user, -1, amount_u128));
                } else if payoff_i128 > 0 {
                    // Receiver economically pays payout fee: pool pays fee on-ledger, but receiver
                    // receives (and is credited) amount minus fee.
                    if amount_u128 <= fee_u128 {
                        return Err(ClearingError::TransferFailed(
                            "settlement payoff too small to cover fee".to_string(),
                        ));
                    }

                    let net_to_receiver: u128 = amount_u128 - fee_u128;

                    receivers.push((user, net_to_receiver));
                    accounting_updates.push((user, 1, net_to_receiver));
                } else {
                    // no transfer, but still clear required margin
                    accounting_updates.push((user, 0, 0));
                }
            }

            for (user, amount_u128) in payers.iter().copied() {
                let amount_nat = candid::Nat::from(amount_u128);

                let from_subaccount = user.clearing_subaccount();

                // Payer pays canister: User Subaccount -> Pool
                let args = TransferArg {
                    from_subaccount: Some(from_subaccount),
                    to: Account {
                        owner: ic_cdk::id(),
                        subaccount: None, // Canister main account pool
                    },
                    amount: amount_nat,
                    fee: None,
                    memo: None,
                    created_at_time: None,
                };

                let (res,): (Result<candid::Nat, TransferError>,) =
                    ic_cdk::call(ledger_id, "icrc1_transfer", (args,))
                        .await
                        .map_err(|(code, msg)| {
                            ClearingError::TransferFailed(format!("RB: {:?}: {}", code, msg))
                        })?;

                res.map_err(|e| ClearingError::TransferFailed(format!("{:?}", e)))?;
            }

            for (user, amount_u128) in receivers.iter().copied() {
                let amount_nat = candid::Nat::from(amount_u128);

                let to_account = user.clearing_account();

                // Canister pays receiver: Pool -> User Subaccount
                let args = TransferArg {
                    from_subaccount: None, // Canister main account pool
                    to: to_account,
                    amount: amount_nat,
                    fee: Some(fee_nat.clone()),
                    memo: None,
                    created_at_time: None,
                };

                let (res,): (Result<candid::Nat, TransferError>,) =
                    ic_cdk::call(ledger_id, "icrc1_transfer", (args,))
                        .await
                        .map_err(|(code, msg)| {
                            ClearingError::TransferFailed(format!("RB: {:?}: {}", code, msg))
                        })?;

                res.map_err(|e| ClearingError::TransferFailed(format!("{:?}", e)))?;
            }

            // Update internal accounting
            MARGIN_ACCOUNTS.with(|accounts| {
                let mut accounts = accounts.borrow_mut();

                for (user, sign, amount_u128) in accounting_updates {
                    if let Some(account) = accounts.get_mut(&user) {
                        let current = account.get_balance(&settlement_asset_val);

                        match sign {
                            1 => account
                                .set_balance(settlement_asset_val.clone(), current + amount_u128),
                            -1 => account
                                .set_balance(settlement_asset_val.clone(), current - amount_u128),
                            _ => {} // sign == 0 => no balance change
                        }

                        account.required_margin = 0;
                    }
                }
            });

            Ok(())
        })
        .await;

    result.into()
}
