use std::collections::BTreeMap;

use ic_cdk::api::time;
use ic_cdk_macros::update;

use crate::{
    guards::{caller_is_controller, caller_is_not_anonymous},
    memory::{
        ACCEPTED_TRANSFERS, EVENTS, EXECUTED_TRADES, FROZEN_TRANSFERS, MARGIN_ACCOUNTS,
        NEXT_EVENT_ID, POSITIONS,
    },
    payoffs::get_required_margin,
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

/// Submits a matched trade from an exchange for clearing.
///
/// This method validates the series registration, calculates margin requirements,
/// checks for sufficient collateral, and updates the positions of both buyer and seller.
///
/// # Arguments
/// * `params` - The trade details including ID, series, buyer, seller, quantity, and price.
///
/// # Returns
/// * [`SubmitMatchedTradeResult::Ok(true)`] if the trade was successfully processed or was a
///   duplicate.
/// * [`SubmitMatchedTradeResult::Err`] if margin is insufficient or another error occurs.
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

        // Calculate margin requirements for the updated positions
        // We need to know the old requirements to calculate the delta
        let (
            buyer_delta_i128,
            seller_delta_i128,
            new_buyer_qty,
            new_buyer_margin,
            new_seller_qty,
            new_seller_margin,
        ) = POSITIONS.with(|positions| {
            let positions = positions.borrow();

            let old_buyer_margin = positions
                .get(&(buyer, series_id.clone()))
                .map(|p| p.locked_collateral)
                .unwrap_or(0);
            let old_seller_margin = positions
                .get(&(seller, series_id.clone()))
                .map(|p| p.locked_collateral)
                .unwrap_or(0);

            // New quantities
            let new_buyer_qty = positions
                .get(&(buyer, series_id.clone()))
                .map(|p| p.net_qty)
                .unwrap_or(0)
                + qty;
            let new_seller_qty = positions
                .get(&(seller, series_id.clone()))
                .map(|p| p.net_qty)
                .unwrap_or(0)
                - qty;

            // New margins
            let new_buyer_margin = get_required_margin(&series, price, new_buyer_qty);
            let new_seller_margin = get_required_margin(&series, price, new_seller_qty);

            (
                (new_buyer_margin as i128) - (old_buyer_margin as i128),
                (new_seller_margin as i128) - (old_seller_margin as i128),
                new_buyer_qty,
                new_buyer_margin,
                new_seller_qty,
                new_seller_margin,
            )
        });

        let buyer_delta = buyer_delta_i128;
        let seller_delta = seller_delta_i128;

        // Phase B: apply state changes (no awaits)
        MARGIN_ACCOUNTS.with(|accounts| {
            let mut accounts = accounts.borrow_mut();

            // Check Buyer
            let buyer_acc = accounts.entry(buyer).or_insert(MarginAccount {
                user: buyer,
                balances: BTreeMap::new(),
                required_margin: 0,
            });
            let buyer_collateral = buyer_acc.get_balance(&settlement_asset);
            let final_buyer_required = if buyer_delta > 0 {
                buyer_acc.required_margin + (buyer_delta as u128)
            } else {
                buyer_acc
                    .required_margin
                    .saturating_sub(buyer_delta.unsigned_abs())
            };

            if final_buyer_required > buyer_collateral {
                return Err(TradeError::InsufficientMargin {
                    user: buyer,
                    balance: buyer_collateral,
                    required: final_buyer_required,
                });
            }

            // Check Seller
            let seller_acc = accounts.entry(seller).or_insert(MarginAccount {
                user: seller,
                balances: BTreeMap::new(),
                required_margin: 0,
            });
            let seller_collateral = seller_acc.get_balance(&settlement_asset);
            let final_seller_required = if seller_delta > 0 {
                seller_acc.required_margin + (seller_delta as u128)
            } else {
                seller_acc
                    .required_margin
                    .saturating_sub(seller_delta.unsigned_abs())
            };

            if final_seller_required > seller_collateral {
                return Err(TradeError::InsufficientMargin {
                    user: seller,
                    balance: seller_collateral,
                    required: final_seller_required,
                });
            }

            // Apply changes
            accounts.get_mut(&buyer).unwrap().required_margin = final_buyer_required;
            accounts.get_mut(&seller).unwrap().required_margin = final_seller_required;

            Ok(())
        })?;

        POSITIONS.with(|positions| {
            let mut positions = positions.borrow_mut();

            let b_pos = positions
                .entry((buyer, series_id.clone()))
                .or_insert(Position {
                    user: buyer,
                    series_id: series_id.clone(),
                    net_qty: 0,
                    locked_collateral: 0,
                });
            b_pos.net_qty = new_buyer_qty;
            b_pos.locked_collateral = new_buyer_margin;

            let s_pos = positions
                .entry((seller, series_id.clone()))
                .or_insert(Position {
                    user: seller,
                    series_id: series_id.clone(),
                    net_qty: 0,
                    locked_collateral: 0,
                });
            s_pos.net_qty = new_seller_qty;
            s_pos.locked_collateral = new_seller_margin;
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

/// Freezes a user's position to prepare it for transfer to another clearing canister.
///
/// Once frozen, the position is removed from active state and a [`PositionProof`] is issued.
/// This method is gated to canister controllers.
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

/// Accepts a position transfer from another clearing canister.
///
/// This method validates the series and increases the target user's position based on the provided
/// proof. This method is gated to canister controllers.
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
                    locked_collateral: 0,
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
