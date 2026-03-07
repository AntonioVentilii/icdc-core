use std::collections::BTreeMap;

use crate::{
    api::trade::errors::TradeError,
    memory::{EVENTS, EXECUTED_TRADES, MARGIN_ACCOUNTS, NEXT_EVENT_ID, POSITIONS},
    payoffs::get_required_margin,
    trade::types::ExecuteTradeParams,
    types::{
        event::{Event, EventType},
        margin::{MarginAccount, Position},
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

    let settlement_asset = series.settlement_asset.to_asset();

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

        let new_buyer_margin = get_required_margin(&series, &price, new_buyer_qty);
        let new_seller_margin = get_required_margin(&series, &price, new_seller_qty);

        (
            (new_buyer_margin as i128) - (old_buyer_margin as i128),
            (new_seller_margin as i128) - (old_seller_margin as i128),
            new_buyer_qty,
            new_buyer_margin,
            new_seller_qty,
            new_seller_margin,
        )
    });

    MARGIN_ACCOUNTS.with(|accounts| {
        let mut accounts = accounts.borrow_mut();

        // Update Buyer
        let final_buyer_required = {
            let buyer_acc = accounts.entry(buyer).or_insert(MarginAccount {
                user: buyer,
                balances: BTreeMap::new(),
                reserved_balances: BTreeMap::new(),
                required_margin: 0,
            });

            if let Some(amt) = buyer_unblock_amount {
                let _ = buyer_acc.release_balance(settlement_asset.clone(), amt);
            }

            let buyer_available = buyer_acc.get_available_balance(&settlement_asset);

            let buyer_required = if buyer_delta_i128 > 0 {
                buyer_acc.required_margin + (buyer_delta_i128 as u128)
            } else {
                buyer_acc
                    .required_margin
                    .saturating_sub(buyer_delta_i128.unsigned_abs())
            };

            if buyer_required > buyer_available {
                return Err(TradeError::InsufficientMargin {
                    user: buyer,
                    balance: buyer_available,
                    required: buyer_required,
                });
            }

            buyer_required
        };

        // Update Seller
        let final_seller_required = {
            let seller_acc = accounts.entry(seller).or_insert(MarginAccount {
                user: seller,
                balances: BTreeMap::new(),
                reserved_balances: BTreeMap::new(),
                required_margin: 0,
            });

            if let Some(amt) = seller_unblock_amount {
                let _ = seller_acc.release_balance(settlement_asset.clone(), amt);
            }

            let seller_available = seller_acc.get_available_balance(&settlement_asset);

            let seller_required = if seller_delta_i128 > 0 {
                seller_acc.required_margin + (seller_delta_i128 as u128)
            } else {
                seller_acc
                    .required_margin
                    .saturating_sub(seller_delta_i128.unsigned_abs())
            };

            if seller_required > seller_available {
                return Err(TradeError::InsufficientMargin {
                    user: seller,
                    balance: seller_available,
                    required: seller_required,
                });
            }

            seller_required
        };

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
            timestamp: ic_cdk::api::time(),
        });
    });

    EXECUTED_TRADES.with(|m| {
        m.borrow_mut().insert(trade_id, event_id);
    });

    Ok(true)
}
