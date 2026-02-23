use std::collections::BTreeMap;

use ic_cdk::api::time;
use ic_cdk_macros::update;

use crate::{
    guards::{caller_is_controller, caller_is_not_anonymous},
    memory::{
        ACCEPTED_TRANSFERS, EVENTS, EXECUTED_TRADES, FROZEN_TRANSFERS, MARGIN_ACCOUNTS,
        NEXT_EVENT_ID, POSITIONS,
    },
    types::{
        errors::TradeError,
        event::{Event, EventType},
        margin::{MarginAccount, Position},
        params::{FreezePositionForTransferParams, SubmitMatchedTradeParams},
        results::{AcceptPositionTransferResult, SubmitMatchedTradeResult},
        state::PositionProof,
    },
    utils::series::ensure_series_registered,
};

#[update(guard = "caller_is_not_anonymous")]
pub async fn submit_matched_trade(params: SubmitMatchedTradeParams) -> SubmitMatchedTradeResult {
    let result: Result<bool, TradeError> = (async {
        let SubmitMatchedTradeParams {
            trade_id,
            series_id,
            buyer,
            seller,
            qty,
            price,
        } = params;

        if EXECUTED_TRADES.with(|m| m.borrow().contains_key(&trade_id)) {
            return Ok(true);
        }

        let series = ensure_series_registered(&series_id).await?;

        let settlement_asset = series.settlement_asset.to_asset();

        // TODO: Calculate real margin based on series parameters. For now we just do qty * price as
        // a placeholder.
        let required_margin = qty.unsigned_abs() * (price as u128) / 1_000_000;

        // Phase B: apply state changes (no awaits) BUT do it in a "commit-like" way
        // We will:
        // 1) update margin accounts
        // 2) update positions
        // 3) append event
        // 4) record EXECUTED_TRADES[trade_id] = event_id
        //
        // Because there is no await between these, this is atomic w.r.t. interleaving.

        MARGIN_ACCOUNTS.with(|accounts| {
            let mut accounts = accounts.borrow_mut();

            if buyer == seller {
                let acc = accounts.entry(buyer).or_insert(MarginAccount {
                    user: buyer,
                    balances: BTreeMap::new(),
                    required_margin: 0,
                });

                let collateral = acc.get_balance(&settlement_asset);

                let new_required = acc.required_margin + required_margin;

                if new_required > collateral {
                    return Err(TradeError::BuyerInsufficientMargin);
                }

                acc.required_margin = new_required;

                return Ok(());
            }

            let buyer_required_now = accounts.get(&buyer).map(|a| a.required_margin).unwrap_or(0);
            let buyer_collateral = accounts
                .get(&buyer)
                .map(|a| a.get_balance(&settlement_asset))
                .unwrap_or(0);

            let seller_required_now = accounts
                .get(&seller)
                .map(|a| a.required_margin)
                .unwrap_or(0);
            let seller_collateral = accounts
                .get(&seller)
                .map(|a| a.get_balance(&settlement_asset))
                .unwrap_or(0);

            let new_buyer_required = buyer_required_now + required_margin;
            if new_buyer_required > buyer_collateral {
                return Err(TradeError::BuyerInsufficientMargin);
            }

            let new_seller_required = seller_required_now + required_margin;
            if new_seller_required > seller_collateral {
                return Err(TradeError::SellerInsufficientMargin);
            }

            {
                let buyer_account = accounts.entry(buyer).or_insert(MarginAccount {
                    user: buyer,
                    balances: BTreeMap::new(),
                    required_margin: 0,
                });
                buyer_account.required_margin = new_buyer_required;
            }

            {
                let seller_account = accounts.entry(seller).or_insert(MarginAccount {
                    user: seller,
                    balances: BTreeMap::new(),
                    required_margin: 0,
                });
                seller_account.required_margin = new_seller_required;
            }

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
                series_id: series_id.clone(),
                user: buyer,
                qty,
                price,
                event_type: EventType::Executed,
                timestamp: time(),
            });
        });

        EXECUTED_TRADES.with(|m| {
            m.borrow_mut().insert(trade_id, event_id);
        });

        Ok(true)
    })
    .await;

    result.into()
}

#[update(guard = "caller_is_controller")]
pub fn freeze_position_for_transfer(
    params: FreezePositionForTransferParams,
) -> Option<PositionProof> {
    let FreezePositionForTransferParams {
        transfer_id,
        user,
        series_id,
    } = params;

    // If already frozen, return the same proof.
    if let Some(existing) = FROZEN_TRANSFERS.with(|m| m.borrow().get(&transfer_id).cloned()) {
        return Some(existing);
    }

    // Otherwise, freeze now.
    let proof_opt = POSITIONS.with(|positions| {
        let mut positions = positions.borrow_mut();

        positions
            .remove(&(user, series_id.clone()))
            .map(|pos| PositionProof {
                transfer_id: transfer_id.clone(),
                user,
                series_id,
                qty: pos.net_qty,
                clearing_id: ic_cdk::id(),
                signature: vec![], // TODO: sign proof later
            })
    });

    // Persist proof so retries are stable.
    if let Some(ref proof) = proof_opt {
        FROZEN_TRANSFERS.with(|m| {
            m.borrow_mut().insert(transfer_id, proof.clone());
        });
    }

    proof_opt
}

#[update(guard = "caller_is_controller")]
pub async fn accept_position_transfer(proof: PositionProof) -> AcceptPositionTransferResult {
    let result: Result<bool, TradeError> = (async {
        if ACCEPTED_TRANSFERS.with(|m| m.borrow().contains_key(&proof.transfer_id)) {
            return Ok(true);
        }

        ensure_series_registered(&proof.series_id).await?;

        POSITIONS.with(|positions| {
            let mut positions = positions.borrow_mut();
            let pos = positions
                .entry((proof.user, proof.series_id.clone()))
                .or_insert(Position {
                    user: proof.user,
                    series_id: proof.series_id.clone(),
                    net_qty: 0,
                });
            pos.net_qty += proof.qty;
        });

        ACCEPTED_TRANSFERS.with(|m| {
            m.borrow_mut().insert(proof.transfer_id.clone(), true);
        });

        Ok(true)
    })
    .await;

    result.into()
}
