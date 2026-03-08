// Removed unused BTreeMap

use shared::constants::USD_DECIMALS;

use crate::{
    api::trade::errors::TradeError,
    memory::{
        ACCOUNT_STATES, COLLATERAL_ASSETS, EVENTS, EXECUTED_TRADES, NEXT_EVENT_ID, POSITIONS,
    },
    payoffs::{get_required_margin, scale_price, RoundingMode},
    trade::types::ExecuteTradeParams,
    types::{
        event::{Event, EventType},
        margin::{AccountState, Position},
    },
    utils::series::ensure_series_registered,
};

/// Internal shared logic for executing a trade and updating positions/margin.
pub(crate) async fn internal_execute_trade(params: ExecuteTradeParams) -> Result<bool, TradeError> {
    let ExecuteTradeParams {
        trade_id,
        series_id,
        buyer,
        seller,
        qty,
        price,
        buyer_unblock_amount,
        seller_unblock_amount,
    } = params;

    if EXECUTED_TRADES.with(|m| m.borrow().contains_key(&trade_id)) {
        return Ok(true);
    }

    let series = ensure_series_registered(&series_id).await?;
    let configs = COLLATERAL_ASSETS.with(|c| c.borrow().clone());

    // Calculate upfront collateral (cost) for both sides
    let buyer_cost = get_required_margin(&series, &price, qty) as i128;
    let seller_cost = get_required_margin(&series, &price, -qty) as i128;

    let (
        buyer_margin_delta,
        seller_margin_delta,
        new_buyer_qty,
        new_buyer_margin_usd,
        new_seller_qty,
        new_seller_margin_usd,
    ) = POSITIONS.with(|positions| {
        let positions = positions.borrow();

        let old_buyer_margin = positions
            .get(&(buyer, series_id.clone()))
            .map(|p| p.reserved_margin_usd)
            .unwrap_or(0);
        let old_seller_margin = positions
            .get(&(seller, series_id.clone()))
            .map(|p| p.reserved_margin_usd)
            .unwrap_or(0);

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

        let new_buyer_margin_usd = get_required_margin(&series, &price, new_buyer_qty);
        let new_seller_margin_usd = get_required_margin(&series, &price, new_seller_qty);

        (
            (new_buyer_margin_usd as i128) - (old_buyer_margin as i128),
            (new_seller_margin_usd as i128) - (old_seller_margin as i128),
            new_buyer_qty,
            new_buyer_margin_usd,
            new_seller_qty,
            new_seller_margin_usd,
        )
    });

    ACCOUNT_STATES.with(|accounts| {
        let mut accounts = accounts.borrow_mut();

        // Update Buyer
        {
            let buyer_acc = accounts
                .entry(buyer)
                .or_insert_with(|| AccountState::new(buyer));

            // 1. Pay for position (upfront collateral)
            buyer_acc.cash_balance_usd -= buyer_cost;

            // 2. Release any pre-blocked margin (Limit Order block_index now represents USD)
            if let Some(amt) = buyer_unblock_amount {
                buyer_acc.reserved_margin_usd =
                    buyer_acc.reserved_margin_usd.saturating_sub(amt as u128);
            }

            // Apply delta and check equity
            let target_reserved = if buyer_margin_delta > 0 {
                buyer_acc.reserved_margin_usd + (buyer_margin_delta as u128)
            } else {
                buyer_acc
                    .reserved_margin_usd
                    .saturating_sub(buyer_margin_delta.unsigned_abs())
            };

            // Note: with upfront cash payment, we don't need to add to reserved_margin_usd
            // periodically, but we must check that the user is still solvent.
            // For now, we keep the margin accounting but it will effectively be 0 for this position
            // if new_buyer_margin_usd is not added to reserved_margin_usd.
            // Wait, internal_execute_trade currently adds/subtracts margin_delta from
            // reserved_margin_usd. If we want to transition to fully
            // cash-collateralized, we should make margin_delta 0. Available is (equity
            // - reserved). If we increase reserved, we need to check if we stay >= 0.
            // New available would be equity - target_reserved.
            let equity = buyer_acc.calculate_equity_usd(&configs);

            if (equity as i128) < (target_reserved as i128) {
                return Err(TradeError::InsufficientMargin {
                    user: buyer,
                    balance: equity,
                    required: target_reserved,
                });
            }

            buyer_acc.reserved_margin_usd = target_reserved;
        }

        // Update Seller
        {
            let seller_acc = accounts
                .entry(seller)
                .or_insert_with(|| AccountState::new(seller));

            // 1. Pay for position (upfront collateral)
            seller_acc.cash_balance_usd -= seller_cost;

            if let Some(amt) = seller_unblock_amount {
                seller_acc.reserved_margin_usd =
                    seller_acc.reserved_margin_usd.saturating_sub(amt as u128);
            }

            let target_reserved = if seller_margin_delta > 0 {
                seller_acc.reserved_margin_usd + (seller_margin_delta as u128)
            } else {
                seller_acc
                    .reserved_margin_usd
                    .saturating_sub(seller_margin_delta.unsigned_abs())
            };

            let equity = seller_acc.calculate_equity_usd(&configs);

            if (equity as i128) < (target_reserved as i128) {
                return Err(TradeError::InsufficientMargin {
                    user: seller,
                    balance: equity,
                    required: target_reserved,
                });
            }

            seller_acc.reserved_margin_usd = target_reserved;
        }

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
                reserved_margin_usd: 0,
            });
        b_pos.net_qty = new_buyer_qty;
        b_pos.reserved_margin_usd = new_buyer_margin_usd;

        let s_pos = positions
            .entry((seller, series_id.clone()))
            .or_insert(Position {
                user: seller,
                series_id: series_id.clone(),
                net_qty: 0,
                reserved_margin_usd: 0,
            });
        s_pos.net_qty = new_seller_qty;
        s_pos.reserved_margin_usd = new_seller_margin_usd;
    });

    let event_id = NEXT_EVENT_ID.with(|id| {
        let mut id = id.borrow_mut();
        let current = *id;
        *id += 1;
        current
    });

    EVENTS.with(|events| {
        let mut events = events.borrow_mut();
        events.push(Event {
            event_id,
            clearing_id: ic_cdk::id(),
            series_id: series_id.clone(),
            user: buyer,
            qty,
            price: price.clone(),
            event_type: EventType::Executed,
            timestamp: ic_cdk::api::time(),
        });

        events.push(Event {
            event_id,
            clearing_id: ic_cdk::id(),
            series_id: series_id.clone(),
            user: seller,
            qty: -qty,
            price,
            event_type: EventType::Executed,
            timestamp: ic_cdk::api::time(),
        });
    });

    EXECUTED_TRADES.with(|m| {
        m.borrow_mut().insert(trade_id, event_id);
    });

    Ok(true)
}
