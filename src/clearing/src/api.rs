use candid::Principal;
use ic_cdk::api::time;
use ic_cdk_macros::{query, update};
use icrc_ledger_types::{
    icrc1::{
        account::Account,
        transfer::{TransferArg, TransferError},
    },
    icrc2::transfer_from::{TransferFromArgs, TransferFromError},
};
use shared::types::{Asset, Event, EventType, MarginAccount, Position, Series};

use crate::{
    account::{derive_user_subaccount, is_supported_asset},
    error::ClearingError,
    memory::{EVENTS, MARGIN_ACCOUNTS, NEXT_EVENT_ID, POSITIONS, REGISTRY_CANISTER, SERIES},
    params::{
        DepositCollateralParams, FreezePositionForTransferParams, GetPositionParams,
        SettleSeriesParams, SubmitMatchedTradeParams, WithdrawCollateralParams,
    },
    results::{
        AcceptPositionTransferResult, DepositCollateralResult, SettleSeriesResult,
        SubmitMatchedTradeResult, WithdrawCollateralResult,
    },
    series::ensure_series_registered,
    types::PositionProof,
};

#[update]
pub async fn deposit_collateral(params: DepositCollateralParams) -> DepositCollateralResult {
    let result: Result<(), ClearingError> = (async {
        let caller = ic_cdk::caller();

        let DepositCollateralParams { amount, asset } = params;

        if !is_supported_asset(&asset) {
            return Err(ClearingError::UnsupportedLedger);
        }

        let Asset::Icrc(ledger_id) = asset;

        let amount_u128: u128 = amount.clone().0.try_into().unwrap_or(0);

        let subaccount = derive_user_subaccount(caller);

        let to_account = Account {
            owner: ic_cdk::id(),
            subaccount: Some(subaccount),
        };

        let args = TransferFromArgs {
            spender_subaccount: None,
            from: Account {
                owner: caller,
                subaccount: None,
            },
            to: to_account,
            amount: amount.clone(),
            fee: None,
            memo: None,
            created_at_time: None,
        };

        let (res,): (Result<candid::Nat, TransferFromError>,) =
            ic_cdk::call(ledger_id, "icrc2_transfer_from", (args,))
                .await
                .map_err(|(code, msg)| {
                    ClearingError::TransferFailed(format!("RB: {:?}: {}", code, msg))
                })?;

        res.map_err(|e| ClearingError::TransferFailed(format!("{:?}", e)))?;

        MARGIN_ACCOUNTS.with(|accounts| {
            let mut accounts = accounts.borrow_mut();
            let account = accounts.entry(caller).or_insert(MarginAccount {
                user: caller,
                balances: Vec::new(),
                required_margin: 0,
            });
            let current = account.get_balance(&asset);
            account.set_balance(asset, current + amount_u128);
        });

        Ok(())
    })
    .await;

    result.into()
}

#[update]
pub async fn withdraw_collateral(params: WithdrawCollateralParams) -> WithdrawCollateralResult {
    let result: Result<(), ClearingError> = (async {
        let caller = ic_cdk::caller();

        let WithdrawCollateralParams { amount, asset } = params;

        if !is_supported_asset(&asset) {
            return Err(ClearingError::UnsupportedLedger);
        }

        let Asset::Icrc(ledger_id) = asset;

        let amount_u128: u128 = amount.clone().0.try_into().unwrap_or(0);

        MARGIN_ACCOUNTS.with(|accounts| {
            let mut accounts = accounts.borrow_mut();
            if let Some(account) = accounts.get_mut(&caller) {
                let current_token_balance = account.get_balance(&asset);
                // Simple check for now: enough in this specific token
                // TODO: valuation across all assets with cross-assets collateral/settlements
                if current_token_balance >= amount_u128 {
                    Ok(())
                } else {
                    Err(ClearingError::InsufficientExcessMargin)
                }
            } else {
                Err(ClearingError::NoMarginAccountFound)
            }
        })?;

        let from_subaccount = derive_user_subaccount(caller);

        let args = TransferArg {
            from_subaccount: Some(from_subaccount),
            to: Account {
                owner: caller,
                subaccount: None,
            },
            amount: amount.clone(),
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

        MARGIN_ACCOUNTS.with(|accounts| {
            let mut accounts = accounts.borrow_mut();
            if let Some(account) = accounts.get_mut(&caller) {
                let current = account.get_balance(&asset);
                account.set_balance(asset, current - amount_u128);
            }
        });

        Ok(())
    })
    .await;

    result.into()
}

#[update]
pub fn set_registry_canister(registry: Principal) {
    // TODO: add authentication
    REGISTRY_CANISTER.with(|r| {
        *r.borrow_mut() = registry;
    });
}

#[update]
pub async fn submit_matched_trade(params: SubmitMatchedTradeParams) -> SubmitMatchedTradeResult {
    let result: Result<bool, ClearingError> = (async {
        let SubmitMatchedTradeParams {
            series_id,
            buyer,
            seller,
            qty,
            price,
        } = params;

        let series = ensure_series_registered(&series_id).await?;

        let settlement_asset = series.settlement_asset.to_asset();

        let required_margin = qty.unsigned_abs() * (price as u128) / 1000000;

        MARGIN_ACCOUNTS.with(|accounts| {
            let mut accounts = accounts.borrow_mut();

            let buyer_account = accounts.entry(buyer).or_insert(MarginAccount {
                user: buyer,
                balances: Vec::new(),
                required_margin: 0,
            });

            let buyer_collateral = buyer_account.get_balance(&settlement_asset);

            let new_buyer_required = buyer_account.required_margin + required_margin;
            if new_buyer_required > buyer_collateral {
                return Err(ClearingError::BuyerInsufficientMargin);
            }
            buyer_account.required_margin = new_buyer_required;

            let seller_account = accounts.entry(seller).or_insert(MarginAccount {
                user: seller,
                balances: Vec::new(),
                required_margin: 0,
            });

            let seller_collateral = seller_account.get_balance(&settlement_asset);

            let new_seller_required = seller_account.required_margin + required_margin;
            if new_seller_required > seller_collateral {
                return Err(ClearingError::SellerInsufficientMargin);
            }
            seller_account.required_margin = new_seller_required;
            Ok(())
        })?;

        POSITIONS.with(|positions| {
            let mut positions = positions.borrow_mut();

            let buyer_pos = positions
                .entry((buyer, series_id.clone()))
                .or_insert(Position {
                    user: buyer,
                    series_id: series_id.clone(),
                    net_qty: 0,
                });
            buyer_pos.net_qty += qty;

            let seller_pos = positions
                .entry((seller, series_id.clone()))
                .or_insert(Position {
                    user: seller,
                    series_id: series_id.clone(),
                    net_qty: 0,
                });
            seller_pos.net_qty -= qty;
        });

        let event_id = NEXT_EVENT_ID.with(|id| {
            let mut id = id.borrow_mut();
            let current = *id;
            *id += 1;
            current
        });

        EVENTS.with(|events| {
            events.borrow_mut().push(Event {
                event_id,
                clearing_id: ic_cdk::id(),
                series_id,
                user: buyer,
                qty,
                price,
                event_type: EventType::Executed,
                timestamp: time(),
            });
        });

        Ok(true)
    })
    .await;

    result.into()
}

#[query]
pub fn get_margin_account(user: Principal) -> Option<MarginAccount> {
    MARGIN_ACCOUNTS.with(|accounts| accounts.borrow().get(&user).cloned())
}

#[query]
pub fn get_position(params: GetPositionParams) -> Option<Position> {
    POSITIONS.with(|positions| {
        positions
            .borrow()
            .get(&(params.user, params.series_id))
            .cloned()
    })
}

#[query]
pub fn list_series() -> Vec<Series> {
    SERIES.with(|s| s.borrow().values().cloned().collect())
}

#[update]
pub async fn settle_series(params: SettleSeriesParams) -> SettleSeriesResult {
    let result: Result<(), ClearingError> = (async {
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

        let positions_to_settle: Vec<(Principal, i128)> = POSITIONS.with(|positions| {
            let mut positions = positions.borrow_mut();
            let users: Vec<Principal> = positions
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

        for (user, net_qty) in positions_to_settle {
            let payoff = (net_qty as f64) * (settlement_price as f64);
            let amount_u128 = payoff.abs() as u128;
            let amount_nat = candid::Nat::from(amount_u128);

            if payoff > 0.0 {
                // Canister pays user: Pool -> User Subaccount
                let args = TransferArg {
                    from_subaccount: None, // From Canister main account pool
                    to: Account {
                        owner: ic_cdk::id(),
                        subaccount: Some(derive_user_subaccount(user)),
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
            } else if payoff < 0.0 {
                // User pays canister: User Subaccount -> Pool
                let args = TransferArg {
                    from_subaccount: Some(derive_user_subaccount(user)),
                    to: Account {
                        owner: ic_cdk::id(),
                        subaccount: None, // To Canister main account
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

            // Update internal accounting
            MARGIN_ACCOUNTS.with(|accounts| {
                let mut accounts = accounts.borrow_mut();
                if let Some(account) = accounts.get_mut(&user) {
                    let current = account.get_balance(&settlement_asset_val);
                    if payoff >= 0.0 {
                        account.set_balance(settlement_asset_val.clone(), current + amount_u128);
                    } else {
                        account.set_balance(settlement_asset_val.clone(), current - amount_u128);
                    }
                    account.required_margin = 0;
                }
            });
        }

        Ok(())
    })
    .await;

    result.into()
}

#[update]
pub fn freeze_position_for_transfer(
    params: FreezePositionForTransferParams,
) -> Option<PositionProof> {
    let FreezePositionForTransferParams { user, series_id } = params;

    POSITIONS.with(|positions| {
        let mut positions = positions.borrow_mut();
        if let Some(pos) = positions.remove(&(user, series_id.clone())) {
            Some(PositionProof {
                user,
                series_id,
                qty: pos.net_qty,
                clearing_id: ic_cdk::id(),
                signature: vec![],
            })
        } else {
            None
        }
    })
}

#[update]
pub async fn accept_position_transfer(proof: PositionProof) -> AcceptPositionTransferResult {
    let result: Result<bool, ClearingError> = (async {
        ensure_series_registered(&proof.series_id).await?;

        POSITIONS.with(|positions| {
            let mut positions = positions.borrow_mut();
            let pos = positions
                .entry((proof.user, proof.series_id.clone()))
                .or_insert(Position {
                    user: proof.user,
                    series_id: proof.series_id,
                    net_qty: 0,
                });
            pos.net_qty += proof.qty;
        });

        Ok(true)
    })
    .await;

    result.into()
}
