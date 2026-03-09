use ic_cdk_macros::{query, update};
use shared::types::Series;

use super::{
    errors::TradeError,
    params::{
        CancelLimitOrderParams, FreezePositionForTransferParams, ListOrdersParams,
        SubmitLimitOrderParams, SubmitMarketOrderParams, SubmitMatchedTradeParams,
    },
    results::{AcceptPositionTransferResult, SubmitMatchedTradeResult},
};
use crate::{
    guards::{caller_is_controller, caller_is_not_anonymous},
    memory::{
        ACCEPTED_TRANSFERS, ACCOUNT_STATES, ASSET_METRICS, COLLATERAL_ASSETS, EVENTS,
        FROZEN_TRANSFERS, LIMIT_ORDERS, POSITIONS,
    },
    payoffs::get_required_margin,
    trade::{
        service::{execute_trade_impl, internal_execute_trade},
        types::ExecuteTradeParams,
    },
    types::{
        errors::CommonError,
        event::{Event, EventType},
        margin::{AccountState, Position},
        state::PositionProof,
        trade::{LimitOrder, Side, TradeId},
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
        let configs = COLLATERAL_ASSETS.with(|c| c.borrow().clone());
        let metrics = ASSET_METRICS.with(|m| m.borrow().clone());

        // Calculate required margin for this order in USD
        let required_margin_usd = get_required_margin(&series, &price, qty);

        ACCOUNT_STATES.with(|accounts| {
            let mut accounts = accounts.borrow_mut();
            let acc = accounts
                .entry(caller)
                .or_insert_with(|| AccountState::new(caller));

            let equity = acc.calculate_equity_usd(&configs, &metrics);
            let target_reserved = acc.reserved_margin_usd + required_margin_usd;

            if (equity as i128) < (target_reserved as i128) {
                return Err(TradeError::InsufficientMargin {
                    user: caller,
                    balance: equity,
                    required: target_reserved,
                });
            }

            acc.reserved_margin_usd = target_reserved;

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
                        blocked_margin_usd: required_margin_usd,
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

        let order = LIMIT_ORDERS.with(|m| {
            m.borrow()
                .get(&matching_order_id)
                .cloned()
                .ok_or(TradeError::OrderNotFound(matching_order_id.clone()))
        })?;

        let series = ensure_series_registered(&order.series_id).await?;

        submit_market_order_impl(taker, order, trade_id, series)
    })
    .await;

    result.into()
}

pub(crate) fn submit_market_order_impl(
    taker: User,
    order: LimitOrder,
    trade_id: TradeId,
    series: Series,
) -> Result<bool, TradeError> {
    // Verify the order still exists — it may have been cancelled during the await.
    LIMIT_ORDERS.with(|m| {
        m.borrow()
            .contains_key(&order.order_id)
            .then_some(())
            .ok_or_else(|| TradeError::OrderNotFound(order.order_id.clone()))
    })?;

    let (buyer, seller, b_unblock, s_unblock) = match order.side {
        Side::Buy => (order.creator, taker, Some(order.blocked_margin_usd), None),
        Side::Sell => (taker, order.creator, None, Some(order.blocked_margin_usd)),
    };

    execute_trade_impl(
        series,
        ExecuteTradeParams {
            trade_id: trade_id.clone(),
            series_id: order.series_id,
            buyer,
            seller,
            qty: order.qty,
            price: order.price,
            buyer_unblock_amount: b_unblock,
            seller_unblock_amount: s_unblock,
        },
    )?;

    // Only remove the order after successful execution
    LIMIT_ORDERS.with(|m| {
        m.borrow_mut().remove(&order.order_id);
    });

    Ok(true)
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

        ACCOUNT_STATES.with(|accounts| {
            let mut accounts = accounts.borrow_mut();
            if let Some(acc) = accounts.get_mut(&caller) {
                acc.reserved_margin_usd = acc
                    .reserved_margin_usd
                    .saturating_sub(order.blocked_margin_usd);
            }
        });

        Ok(true)
    })
    .await;

    result.into()
}

/// Submits a matched trade from an exchange for clearing.
///
/// NOTE: Access is currently restricted to authorized exchange intermediaries
/// (represented by canister controllers in this version).
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
        valuation_price,
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
                // Proofs are unsigned in the current cross-clearing protocol version.
                signature: vec![],
                valuation_price,
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

        let series = ensure_series_registered(&proof.series_id).await?;

        let valuation_price = proof
            .valuation_price
            .or_else(|| series.strike.clone())
            .ok_or_else(|| {
                TradeError::Common(CommonError::Internal(
                    "No valuation price available for transfer".to_string(),
                ))
            })?;

        POSITIONS.with(|positions| {
            let mut positions = positions.borrow_mut();
            let pos = positions
                .entry((proof.user, proof.series_id.clone()))
                .or_insert(Position {
                    user: proof.user,
                    series_id: proof.series_id.clone(),
                    net_qty: 0,
                    reserved_margin_usd: 0,
                });

            let old_margin_usd = pos.reserved_margin_usd;
            pos.net_qty += proof.qty;

            // Recalculate margin for the updated position state
            pos.reserved_margin_usd = get_required_margin(&series, &valuation_price, pos.net_qty);
            let margin_delta = (pos.reserved_margin_usd as i128) - (old_margin_usd as i128);

            // Update the user's aggregate margin reservation in their AccountState
            ACCOUNT_STATES.with(|accounts| {
                let mut accounts = accounts.borrow_mut();
                let acc = accounts
                    .entry(proof.user)
                    .or_insert_with(|| AccountState::new(proof.user));

                if margin_delta > 0 {
                    acc.reserved_margin_usd += margin_delta as u128;
                } else {
                    acc.reserved_margin_usd = acc
                        .reserved_margin_usd
                        .saturating_sub(margin_delta.unsigned_abs());
                }
            });
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

/// Returns a list of all active limit orders, potentially filtered by series.
#[query]
pub fn list_orders(params: ListOrdersParams) -> Vec<LimitOrder> {
    LIMIT_ORDERS.with(|orders| {
        let orders = orders.borrow();

        match params.series_id {
            Some(series_id) => orders
                .values()
                .filter(|o| o.series_id == series_id)
                .cloned()
                .collect(),
            None => orders.values().cloned().collect(),
        }
    })
}

#[cfg(test)]
mod tests {
    use candid::Principal;
    use shared::types::{Description, PayoffType, PayoutUnit, Price, Series, SeriesId};

    use super::*;
    use crate::{
        memory::{ACCOUNT_STATES, LIMIT_ORDERS},
        types::trade::OrderId,
    };

    fn test_series(series_id: &SeriesId) -> Series {
        Series {
            series_id: series_id.clone(),
            underlying: "BTC".to_string(),
            expiry_ns: 2000000000,
            payoff_type: PayoffType::Call,
            strike: Some(Price::new(50_000_000, 6)),
            price_precision: 6,
            payout_unit: PayoutUnit::usd(),
            oracle_source: "oracle".to_string(),
            creator: Principal::anonymous(),
            created_at_ns: 1000000000,
            title: "Test".to_string(),
            description: Description::plain("Test Description"),
        }
    }

    fn test_order(order_id: &OrderId, series_id: &SeriesId, creator: User) -> LimitOrder {
        LimitOrder {
            order_id: order_id.clone(),
            creator,
            series_id: series_id.clone(),
            side: Side::Buy,
            qty: 1,
            price: Price::new(60_000_000, 6),
            blocked_margin_usd: 60_000_000,
        }
    }

    #[test]
    fn test_market_order_atomicity_on_execution_failure() {
        let maker = User(Principal::from_slice(&[1]));
        let taker = User(Principal::from_slice(&[2]));
        let series_id = SeriesId::from("test_ser".to_string());
        let order_id = OrderId::from("order_1".to_string());
        let trade_id = TradeId::from("trade_1".to_string());

        let order = test_order(&order_id, &series_id, maker);

        LIMIT_ORDERS.with(|m| {
            let mut m = m.borrow_mut();
            m.clear();
            m.insert(order_id.clone(), order.clone());
        });

        // Maker has enough funds, but Taker (Seller) has NO funds
        ACCOUNT_STATES.with(|acc| {
            let mut acc = acc.borrow_mut();
            acc.clear();

            let mut m_acc = AccountState::new(maker);
            m_acc.cash_balance_usd = 100_000_000;
            m_acc.reserved_margin_usd = 60_000_000;
            acc.insert(maker, m_acc);

            let mut t_acc = AccountState::new(taker);
            t_acc.cash_balance_usd = 0; // Insolvency for seller
            acc.insert(taker, t_acc);
        });

        let result = submit_market_order_impl(taker, order, trade_id, test_series(&series_id));

        assert!(result.is_err());

        // Verify Limit Order STILL EXISTS (atomicity check)
        LIMIT_ORDERS.with(|m| {
            assert!(m.borrow().contains_key(&order_id));
        });
    }

    #[test]
    fn test_market_order_normal_flow() {
        let maker = User(Principal::from_slice(&[1]));
        let taker = User(Principal::from_slice(&[2]));
        let series_id = SeriesId::from("test_ser".to_string());
        let order_id = OrderId::from("order_1".to_string());
        let trade_id = TradeId::from("trade_1".to_string());

        let order = test_order(&order_id, &series_id, maker);

        LIMIT_ORDERS.with(|m| {
            let mut m = m.borrow_mut();
            m.clear();
            m.insert(order_id.clone(), order.clone());
        });

        // Both have enough funds
        ACCOUNT_STATES.with(|acc| {
            let mut acc = acc.borrow_mut();
            acc.clear();

            let mut m_acc = AccountState::new(maker);
            m_acc.cash_balance_usd = 1_000_000_000;
            m_acc.reserved_margin_usd = 60_000_000;
            acc.insert(maker, m_acc);

            let mut t_acc = AccountState::new(taker);
            t_acc.cash_balance_usd = 1_000_000_000;
            acc.insert(taker, t_acc);
        });

        let result = submit_market_order_impl(taker, order, trade_id, test_series(&series_id));

        assert!(result.is_ok());

        // Verify Limit Order WAS REMOVED
        LIMIT_ORDERS.with(|m| {
            assert!(!m.borrow().contains_key(&order_id));
        });
    }

    /// Simulates the scenario where a limit order is cancelled by a concurrent
    /// call between the initial read in `submit_market_order` and the execution
    /// in `submit_market_order_impl`. The impl should return `OrderNotFound`.
    #[test]
    fn test_market_order_cancelled_during_await() {
        let maker = User(Principal::from_slice(&[1]));
        let taker = User(Principal::from_slice(&[2]));
        let series_id = SeriesId::from("test_ser".to_string());
        let order_id = OrderId::from("order_1".to_string());
        let trade_id = TradeId::from("trade_1".to_string());

        // The order was read before the await, but is NOT in the map anymore
        // (simulating a concurrent cancel_limit_order).
        let order = test_order(&order_id, &series_id, maker);

        LIMIT_ORDERS.with(|m| m.borrow_mut().clear());
        ACCOUNT_STATES.with(|acc| acc.borrow_mut().clear());

        let result = submit_market_order_impl(taker, order, trade_id, test_series(&series_id));

        assert!(
            matches!(&result, Err(TradeError::OrderNotFound(id)) if *id == order_id),
            "expected OrderNotFound for a cancelled order, got: {result:?}"
        );
    }
}
