use std::collections::BTreeMap;

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
use num_traits::ToPrimitive;
use shared::types::{Asset, Series, SeriesId};

use crate::{
    account::is_supported_asset,
    error::ClearingError,
    guards::{caller_is_controller, caller_is_not_anonymous},
    memory::{
        DEPOSIT_PLANS, EVENTS, MARGIN_ACCOUNTS, NEXT_EVENT_ID, POSITIONS, REGISTRY_CANISTER,
        SERIES, WITHDRAWAL_PLANS,
    },
    params::{
        DepositCollateralParams, FreezePositionForTransferParams, GetPositionParams,
        SettleSeriesParams, SubmitMatchedTradeParams, WithdrawCollateralParams,
    },
    results::{
        AcceptPositionTransferResult, DepositCollateralResult, SettleSeriesResult,
        SubmitMatchedTradeResult, WithdrawCollateralResult,
    },
    series::ensure_series_registered,
    traits::ClearingAccountExt,
    types::{
        DepositPlan, Event, EventType, MarginAccount, PlanStatus, Position, PositionProof, User,
        WithdrawalPlan,
    },
};

#[update(guard = "caller_is_not_anonymous")]
pub async fn deposit_collateral(params: DepositCollateralParams) -> DepositCollateralResult {
    let result: Result<(), ClearingError> = (async {
        let user: User = ic_cdk::caller().into();

        let DepositCollateralParams {
            amount,
            asset,
            deposit_id,
        } = params;

        if !is_supported_asset(&asset) {
            return Err(ClearingError::UnsupportedLedger);
        }

        let Asset::Icrc(ledger_id) = asset;

        // ---------- Phase A: Build plan (no awaits) ----------
        // If plan exists, we resume (or no-op if finalised).
        let mut plan =
            DepositPlan::get_or_create(deposit_id.clone(), user, asset.clone(), amount.clone());

        // Already done → idempotent success.
        if plan.status == PlanStatus::Finalised {
            return Ok(());
        }

        // ---------- Phase B: Execute transfer (async, resumable) ----------
        if plan.receipt.is_none() {
            // Mark executing (durably) before the await.
            plan.status = PlanStatus::Executing;

            DEPOSIT_PLANS.with(|m| m.borrow_mut().insert(deposit_id.clone(), plan.clone()));

            // TODO: is the sender correct? And the receiver?
            let args = TransferFromArgs {
                spender_subaccount: None,
                from: Account {
                    owner: user.principal(),
                    subaccount: None,
                },
                to: plan.to_account,
                amount: amount.clone(),
                fee: None,
                memo: None,
                created_at_time: plan.idempotency.to_created_at_time(),
            };

            let (res,): (Result<candid::Nat, TransferFromError>,) =
                ic_cdk::call(ledger_id, "icrc2_transfer_from", (args,))
                    .await
                    .map_err(|(code, msg)| {
                        ClearingError::TransferFailed(format!("RB: {:?}: {}", code, msg))
                    })?;

            match res {
                Ok(block_index) => {
                    plan.receipt = Some(block_index.into());
                }
                Err(TransferFromError::Duplicate { duplicate_of }) => {
                    // Treat Duplicate as success
                    plan.receipt = Some(duplicate_of.into())
                }
                Err(e) => {
                    // Keep plan persisted so retry resumes safely.
                    DEPOSIT_PLANS.with(|m| m.borrow_mut().insert(deposit_id.clone(), plan.clone()));
                    return Err(ClearingError::TransferFailed(format!("{:?}", e)));
                }
            }

            // Persist progress AFTER success/duplicate, before doing anything else.
            DEPOSIT_PLANS.with(|m| m.borrow_mut().insert(deposit_id.clone(), plan.clone()));
        }

        // ---------- Phase C: Finalise (no awaits, idempotent) ----------
        if plan.receipt.is_some() && plan.status != PlanStatus::Finalised {
            let amount_u128: u128 = amount.clone().0.try_into().unwrap_or(0);

            MARGIN_ACCOUNTS.with(|accounts| {
                let mut accounts = accounts.borrow_mut();
                let account = accounts.entry(user).or_insert(MarginAccount {
                    user,
                    balances: BTreeMap::new(),
                    required_margin: 0,
                });
                let current = account.get_balance(&asset);
                account.set_balance(asset.clone(), current + amount_u128);
            });

            plan.status = PlanStatus::Finalised;

            DEPOSIT_PLANS.with(|m| m.borrow_mut().insert(deposit_id.clone(), plan));
        }

        Ok(())
    })
    .await;

    result.into()
}

#[update(guard = "caller_is_not_anonymous")]
pub async fn withdraw_collateral(params: WithdrawCollateralParams) -> WithdrawCollateralResult {
    let result: Result<(), ClearingError> = (async {
        let user: User = ic_cdk::caller().into();

        let WithdrawCollateralParams {
            amount,
            asset,
            withdrawal_id,
        } = params;

        if !is_supported_asset(&asset) {
            return Err(ClearingError::UnsupportedLedger);
        }

        let Asset::Icrc(ledger_id) = asset;

        let amount_u128: u128 = amount
            .0
            .clone()
            .try_into()
            .map_err(|_| ClearingError::WithdrawCollateralMathOverflow)?;

        // Phase A: Build plan (durable, no awaits)
        let mut plan = WithdrawalPlan::get_or_create(
            withdrawal_id.clone(),
            user,
            asset.clone(),
            amount.clone(),
        );

        if plan.status == PlanStatus::Finalised {
            return Ok(());
        }

        // Phase B: execute transfer (async, resumable + ledger idempotency)
        if plan.receipt.is_none() {
            // Persist that we’re executing before the await
            plan.status = PlanStatus::Executing;

            WITHDRAWAL_PLANS.with(|m| m.borrow_mut().insert(withdrawal_id.clone(), plan.clone()));

            let args = TransferArg {
                from_subaccount: Some(plan.from_subaccount),
                to: plan.to_account,
                amount: plan.amount.clone(),
                fee: None,
                memo: None,
                created_at_time: plan.idempotency.to_created_at_time(),
            };

            let (res,): (Result<candid::Nat, TransferError>,) =
                ic_cdk::call(ledger_id, "icrc1_transfer", (args,))
                    .await
                    .map_err(|(code, msg)| {
                        ClearingError::TransferFailed(format!("RB: {:?}: {}", code, msg))
                    })?;

            match res {
                Ok(block_index) => {
                    plan.receipt = Some(block_index.into());
                }
                Err(TransferError::Duplicate { duplicate_of }) => {
                    plan.receipt = Some(duplicate_of.into());
                }
                Err(e) => {
                    // Persist plan so retries resume and ledger dedupe can work
                    WITHDRAWAL_PLANS
                        .with(|m| m.borrow_mut().insert(withdrawal_id.clone(), plan.clone()));
                    return Err(ClearingError::TransferFailed(format!("{:?}", e)));
                }
            }

            // Persist after successful transfer / duplicate
            WITHDRAWAL_PLANS.with(|m| m.borrow_mut().insert(withdrawal_id.clone(), plan.clone()));
        }

        // Phase C: finalise (no awaits, idempotent)
        if plan.receipt.is_some() && plan.status != PlanStatus::Finalised {
            // IMPORTANT: enforce internal risk/margin check HERE (no awaits)
            // (Put back your margin eligibility logic, but based on internal balances + required
            // margin.)
            MARGIN_ACCOUNTS.with(|accounts| {
                let mut accounts = accounts.borrow_mut();
                let account = accounts.entry(user).or_insert(MarginAccount {
                    user,
                    balances: BTreeMap::new(),
                    required_margin: 0,
                });
                let current = account.get_balance(&asset);
                if current < amount_u128 {
                    return Err(ClearingError::InsufficientExcessMargin {
                        current: candid::Nat::from(current),
                        requested: amount.clone(),
                        required: amount.clone(), // replace later with true required margin logic
                    });
                }
                account.set_balance(asset.clone(), current - amount_u128);
                Ok(())
            })?;

            plan.status = PlanStatus::Finalised;

            WITHDRAWAL_PLANS.with(|m| m.borrow_mut().insert(withdrawal_id, plan));
        };

        Ok(())
    })
    .await;

    result.into()
}

#[update(guard = "caller_is_controller")]
pub fn set_registry_canister(registry: Principal) {
    REGISTRY_CANISTER.with(|r| {
        *r.borrow_mut() = registry;
    });
}

#[update(guard = "caller_is_not_anonymous")]
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
                balances: BTreeMap::new(),
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
                balances: BTreeMap::new(),
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

#[update(guard = "caller_is_not_anonymous")]
pub async fn get_margin_account(user: Principal) -> Result<MarginAccount, ClearingError> {
    // TODO: only allow caller to fetch self???

    let user: User = user.into();

    let from_account = user.clearing_account();

    let assets_to_refresh = MARGIN_ACCOUNTS.with(|accounts| {
        accounts
            .borrow()
            .get(&user)
            .map(|m| m.tracked_assets())
            .unwrap_or_default()
    });

    let mut balances: BTreeMap<Asset, u128> = BTreeMap::new();

    for asset in assets_to_refresh.iter().cloned() {
        let Asset::Icrc(ledger_id) = asset.clone();

        let (ledger_balance,): (candid::Nat,) =
            ic_cdk::call(ledger_id, "icrc1_balance_of", (from_account,))
                .await
                .map_err(|(code, msg)| {
                    ClearingError::FetchingBalanceFailed(format!(
                        "icrc1_balance_of {:?}: {}",
                        code, msg
                    ))
                })?;

        let bal_u128: u128 = ledger_balance
            .0
            .try_into()
            .map_err(|_| ClearingError::BalanceMathOverflow)?;

        balances.insert(asset, bal_u128);
    }

    MARGIN_ACCOUNTS.with(|accounts| {
        let mut accounts = accounts.borrow_mut();
        let acct = accounts.entry(user).or_insert(MarginAccount {
            user,
            balances: BTreeMap::new(),
            required_margin: 0,
        });

        acct.balances = balances.clone();
    });

    MARGIN_ACCOUNTS.with(|accounts| {
        accounts
            .borrow()
            .get(&user)
            .cloned()
            .ok_or(ClearingError::NoMarginAccountFound)
    })
}

#[update(guard = "caller_is_not_anonymous")]
pub async fn get_margin_account_fresh(user: Principal) -> Result<MarginAccount, ClearingError> {
    // TODO: only allow caller to fetch self???
    let user: User = user.into();

    let from_account = user.clearing_account();

    let required_margin_u128 = MARGIN_ACCOUNTS.with(|accounts| {
        accounts
            .borrow()
            .get(&user)
            .map(|m| m.required_margin)
            .unwrap_or(0)
    });

    let assets_to_refresh = MARGIN_ACCOUNTS.with(|accounts| {
        accounts
            .borrow()
            .get(&user)
            .map(|m| m.tracked_assets())
            .unwrap_or_default()
    });

    let mut balances: BTreeMap<Asset, u128> = BTreeMap::new();

    for asset in assets_to_refresh.iter().cloned() {
        let Asset::Icrc(ledger_id) = asset.clone();

        let (ledger_balance,): (candid::Nat,) =
            ic_cdk::call(ledger_id, "icrc1_balance_of", (from_account,))
                .await
                .map_err(|(code, msg)| {
                    ClearingError::FetchingBalanceFailed(format!(
                        "icrc1_balance_of {:?}: {}",
                        code, msg
                    ))
                })?;

        let bal_u128: u128 = ledger_balance
            .0
            .try_into()
            .map_err(|_| ClearingError::BalanceMathOverflow)?;

        balances.insert(asset, bal_u128);
    }

    MARGIN_ACCOUNTS.with(|accounts| {
        let mut accounts = accounts.borrow_mut();
        let acct = accounts.entry(user).or_insert(MarginAccount {
            user,
            balances: BTreeMap::new(),
            required_margin: 0,
        });

        acct.balances = balances.clone();
    });

    Ok(MarginAccount {
        user,
        balances,
        required_margin: required_margin_u128,
    })
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
pub fn get_positions(user: Principal) -> Vec<(SeriesId, Position)> {
    let user: User = user.into();

    POSITIONS.with(|positions| {
        positions
            .borrow()
            .iter()
            .filter(|((u, _), _)| *u == user)
            .map(|((_, series_id), position)| (series_id.clone(), position.clone()))
            .collect()
    })
}

#[query]
pub fn list_series() -> Vec<Series> {
    SERIES.with(|s| s.borrow().values().cloned().collect())
}

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

#[update(guard = "caller_is_controller")]
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

#[update(guard = "caller_is_controller")]
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
