use std::collections::BTreeMap;

use ic_cdk_macros::{query, update};

use super::{
    errors::TradeError,
    params::{
        CancelLimitOrderParams, FreezePositionForTransferParams, SubmitLimitOrderParams,
        SubmitMarketOrderParams, SubmitMatchedTradeParams,
    },
    results::{AcceptPositionTransferResult, SubmitMatchedTradeResult},
};
use crate::{
    guards::{caller_is_controller, caller_is_not_anonymous},
    memory::{
        ACCEPTED_TRANSFERS, EVENTS, EXECUTED_TRADES, FROZEN_TRANSFERS, LIMIT_ORDERS,
        MARGIN_ACCOUNTS, POSITIONS,
    },
    payoffs::get_required_margin,
    trade::{service::internal_execute_trade, types::ExecuteTradeParams},
    types::{
        event::{Event, EventType},
        margin::{MarginAccount, Position},
        state::PositionProof,
        trade::{LimitOrder, Side},
        user::User,
    },
    utils::series::ensure_series_registered,
};

/// Submits a limit order for the caller.
///
/// This atomically blocks the required collateral for the order.
#[update(guard = "caller_is_not_anonymous")]
pub async fn submit_limit_order(params: SubmitLimitOrderParams) -> SubmitMatchedTradeResult {
    let result: Result<bool, TradeError> = (async {
        let caller: User = ic_cdk::caller().into();

        let SubmitLimitOrderParams {
            order_id,
            series_id,
            side,
            qty,
            price,
        } = params;

        if LIMIT_ORDERS.with(|m| m.borrow().contains_key(&order_id)) {
            return Ok(true);
        }

        let series = ensure_series_registered(&series_id).await?;

        let settlement_asset = series.settlement_asset.to_asset();

        // Calculate required margin for this order (worst case)
        let required_margin = get_required_margin(&series, &price, qty);

        MARGIN_ACCOUNTS.with(|accounts| {
            let mut accounts = accounts.borrow_mut();
            let acc = accounts.entry(caller).or_insert(MarginAccount {
                user: caller,
                balances: BTreeMap::new(),
                reserved_balances: BTreeMap::new(),
                required_margin: 0,
            });

            acc.reserve_balance(settlement_asset.clone(), required_margin)
                .map_err(|available| TradeError::InsufficientMargin {
                    user: caller,
                    balance: available,
                    required: required_margin,
                })?;

            LIMIT_ORDERS.with(|m| {
                m.borrow_mut().insert(
                    order_id.clone(),
                    LimitOrder {
                        order_id,
                        creator: caller,
                        series_id,
                        side,
                        qty,
                        price,
                        block_index: required_margin,
                    },
                );
            });

            Ok(true)
        })
    })
    .await;

    result.into()
}

/// Submits a market order to match an existing limit order.
///
/// The caller is the taker.
#[update(guard = "caller_is_not_anonymous")]
pub async fn submit_market_order(params: SubmitMarketOrderParams) -> SubmitMatchedTradeResult {
    let result: Result<bool, TradeError> = (async {
        let taker: User = ic_cdk::caller().into();

        let SubmitMarketOrderParams {
            trade_id,
            matching_order_id,
        } = params;

        if EXECUTED_TRADES.with(|m| m.borrow().contains_key(&trade_id)) {
            return Ok(true);
        }

        let order = LIMIT_ORDERS.with(|m| {
            m.borrow_mut()
                .remove(&matching_order_id)
                .ok_or(TradeError::OrderNotFound(matching_order_id.clone()))
        })?;

        let (buyer, seller, b_unblock, s_unblock) = match order.side {
            Side::Buy => (order.creator, taker, Some(order.block_index), None),
            Side::Sell => (taker, order.creator, None, Some(order.block_index)),
        };

        internal_execute_trade(ExecuteTradeParams {
            trade_id,
            series_id: order.series_id,
            buyer,
            seller,
            qty: order.qty,
            price: order.price,
            buyer_unblock_amount: b_unblock,
            seller_unblock_amount: s_unblock,
        })
        .await
    })
    .await;

    result.into()
}

/// Cancels an existing limit order and releases the reserved collateral.
///
/// Only the creator of the order can cancel it.
#[update(guard = "caller_is_not_anonymous")]
pub async fn cancel_limit_order(params: CancelLimitOrderParams) -> SubmitMatchedTradeResult {
    let result: Result<bool, TradeError> = (async {
        let caller: User = ic_cdk::caller().into();

        let order_id = params.order_id;

        let order = LIMIT_ORDERS.with(|m| {
            let mut m = m.borrow_mut();

            if let Some(o) = m.get(&order_id) {
                if o.creator != caller {
                    return Err(TradeError::NotOrderCreator);
                }
                Ok(m.remove(&order_id).unwrap())
            } else {
                Err(TradeError::OrderNotFound(order_id))
            }
        })?;

        let series = ensure_series_registered(&order.series_id).await?;

        let settlement_asset = series.settlement_asset.to_asset();

        MARGIN_ACCOUNTS.with(|accounts| {
            let mut accounts = accounts.borrow_mut();
            if let Some(acc) = accounts.get_mut(&caller) {
                let _ = acc.release_balance(settlement_asset, order.block_index);
            }
        });

        Ok(true)
    })
    .await;

    result.into()
}

/// Submits a matched trade from an exchange for clearing.
///
/// TODO: until we implement an allowed list of exchange canisters, this is gated to controllers
#[update(guard = "caller_is_controller")]
pub async fn submit_matched_trade(params: SubmitMatchedTradeParams) -> SubmitMatchedTradeResult {
    let result = internal_execute_trade(ExecuteTradeParams {
        trade_id: params.trade_id,
        series_id: params.series_id,
        buyer: params.buyer,
        seller: params.seller,
        qty: params.qty,
        price: params.price,
        buyer_unblock_amount: params
            .buyer_unblock_amount
            .map(|n| n.0.try_into().unwrap_or(0)),
        seller_unblock_amount: params
            .seller_unblock_amount
            .map(|n| n.0.try_into().unwrap_or(0)),
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

/// Retrieves all active limit orders for the caller.
#[query(guard = "caller_is_not_anonymous")]
pub fn get_orders() -> Vec<LimitOrder> {
    let caller: User = ic_cdk::caller().into();

    LIMIT_ORDERS.with(|orders| {
        orders
            .borrow()
            .values()
            .filter(|o| o.creator == caller)
            .cloned()
            .collect()
    })
}

/// Retrieves the trade history (executed trades) for the caller.
#[query(guard = "caller_is_not_anonymous")]
pub fn get_trade_history() -> Vec<Event> {
    let caller: User = ic_cdk::caller().into();

    EVENTS.with(|events| {
        events
            .borrow()
            .iter()
            .filter(|e| e.user == caller && matches!(e.event_type, EventType::Executed))
            .cloned()
            .collect()
    })
}
