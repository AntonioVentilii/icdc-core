use shared::types::Series;

use crate::{
    api::trade::errors::TradeError,
    memory::{
        ACCOUNT_STATES, COLLATERAL_ASSETS, EVENTS, EXECUTED_TRADES, NEXT_EVENT_ID, POSITIONS,
    },
    payoffs::get_required_margin,
    trade::types::ExecuteTradeParams,
    types::{
        event::{Event, EventType},
        margin::{AccountState, Position},
    },
    utils::series::ensure_series_registered,
};

/// Internal shared logic for executing a trade and updating positions/margin.
pub(crate) async fn internal_execute_trade(params: ExecuteTradeParams) -> Result<bool, TradeError> {
    let series_id = params.series_id.clone();

    let series = ensure_series_registered(&series_id).await?;

    execute_trade_impl(series, params)
}

pub(crate) fn execute_trade_impl(
    series: Series,
    params: ExecuteTradeParams,
) -> Result<bool, TradeError> {
    if EXECUTED_TRADES.with(|m| m.borrow().contains_key(&params.trade_id)) {
        return Ok(true);
    }

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

    // Validation and Calculation Phase
    let (target_buyer_reserved, target_seller_reserved) = ACCOUNT_STATES.with(|accounts| {
        let accounts = accounts.borrow();

        // 1. Validate Buyer
        let buyer_acc = accounts
            .get(&buyer)
            .cloned()
            .unwrap_or_else(|| AccountState::new(buyer));
        let mut temp_buyer = buyer_acc.clone();

        temp_buyer.cash_balance_usd -= buyer_cost;
        if let Some(amt) = buyer_unblock_amount {
            temp_buyer.reserved_margin_usd = temp_buyer.reserved_margin_usd.saturating_sub(amt);
        }

        let target_buyer_reserved = if buyer_margin_delta > 0 {
            temp_buyer.reserved_margin_usd + (buyer_margin_delta as u128)
        } else {
            temp_buyer
                .reserved_margin_usd
                .saturating_sub(buyer_margin_delta.unsigned_abs())
        };

        let buyer_equity = temp_buyer.calculate_equity_usd(&configs);
        if (buyer_equity as i128) < (target_buyer_reserved as i128) {
            return Err(TradeError::InsufficientMargin {
                user: buyer,
                balance: buyer_equity,
                required: target_buyer_reserved,
            });
        }

        // 2. Validate Seller
        let seller_acc = accounts
            .get(&seller)
            .cloned()
            .unwrap_or_else(|| AccountState::new(seller));
        let mut temp_seller = seller_acc.clone();

        temp_seller.cash_balance_usd -= seller_cost;
        if let Some(amt) = seller_unblock_amount {
            temp_seller.reserved_margin_usd = temp_seller.reserved_margin_usd.saturating_sub(amt);
        }

        let target_seller_reserved = if seller_margin_delta > 0 {
            temp_seller.reserved_margin_usd + (seller_margin_delta as u128)
        } else {
            temp_seller
                .reserved_margin_usd
                .saturating_sub(seller_margin_delta.unsigned_abs())
        };

        let seller_equity = temp_seller.calculate_equity_usd(&configs);
        if (seller_equity as i128) < (target_seller_reserved as i128) {
            return Err(TradeError::InsufficientMargin {
                user: seller,
                balance: seller_equity,
                required: target_seller_reserved,
            });
        }

        Ok((target_buyer_reserved, target_seller_reserved))
    })?;

    // Mutation Phase: Apply all changes only after all validations pass.
    ACCOUNT_STATES.with(|accounts| {
        let mut accounts = accounts.borrow_mut();

        // Update Buyer
        let buyer_acc = accounts
            .entry(buyer)
            .or_insert_with(|| AccountState::new(buyer));
        buyer_acc.cash_balance_usd -= buyer_cost;
        if let Some(amt) = buyer_unblock_amount {
            buyer_acc.reserved_margin_usd = buyer_acc.reserved_margin_usd.saturating_sub(amt);
        }
        buyer_acc.reserved_margin_usd = target_buyer_reserved;

        // Update Seller
        let seller_acc = accounts
            .entry(seller)
            .or_insert_with(|| AccountState::new(seller));
        seller_acc.cash_balance_usd -= seller_cost;
        if let Some(amt) = seller_unblock_amount {
            seller_acc.reserved_margin_usd = seller_acc.reserved_margin_usd.saturating_sub(amt);
        }
        seller_acc.reserved_margin_usd = target_seller_reserved;
    });

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

        // Buyer Event
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

        // Seller Event (using same event_id or should it be different? Usually history shows the
        // interaction) We use the same event_id as it's the same trade, but now indexed for
        // both users.
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

#[cfg(test)]
mod tests {
    use candid::Principal;
    use shared::types::{
        description::Description, PayoffType, PayoutUnit, Price, Series, SeriesId,
    };

    use super::*;
    use crate::{
        memory::{ACCOUNT_STATES, SERIES},
        types::{trade::TradeId, user::User},
    };

    #[test]
    fn test_trade_atomicity_on_failure() {
        let buyer_p = Principal::from_slice(&[1]);
        let seller_p = Principal::from_slice(&[2]);
        let buyer = User(buyer_p);
        let seller = User(seller_p);
        let series_id = SeriesId::from("test".to_string());

        let series = Series {
            series_id: series_id.clone(),
            underlying: "BTC".to_string(),
            expiry_ns: 2000000000,
            payoff_type: PayoffType::Call,
            strike: Some(Price::new(50_000_000_000, 8)),
            price_precision: 8,
            payout_unit: PayoutUnit::usd(),
            oracle_source: "oracle".to_string(),
            creator: Principal::anonymous(),
            created_at_ns: 1000000000,
            title: "Test".to_string(),
            description: Description::plain("Test Description"),
        };

        // Initialize state
        SERIES.with(|s| s.borrow_mut().insert(series_id.clone(), series.clone()));

        ACCOUNT_STATES.with(|accounts| {
            let mut accounts = accounts.borrow_mut();
            accounts.clear();

            // Buyer has enough margin ($2000)
            let mut b_acc = AccountState::new(buyer);
            b_acc.cash_balance_usd = 2_000_000_000;
            accounts.insert(buyer, b_acc);

            // Seller has NO margin
            let mut s_acc = AccountState::new(seller);
            s_acc.cash_balance_usd = 0;
            accounts.insert(seller, s_acc);
        });

        let params = ExecuteTradeParams {
            trade_id: TradeId::from("trade_1".to_string()),
            series_id: series_id.clone(),
            buyer,
            seller,
            qty: 1,                               // 1 unit
            price: Price::new(60_000_000_000, 8), // $600
            buyer_unblock_amount: None,
            seller_unblock_amount: None,
        };

        // Trade should fail for seller due to insufficient margin
        let result = execute_trade_impl(series, params);
        assert!(result.is_err());

        if let Err(TradeError::InsufficientMargin { user, .. }) = result {
            assert_eq!(user, seller);
        } else {
            panic!("Expected InsufficientMargin for seller");
        }

        // Verify Buyer's balance was NOT decremented
        ACCOUNT_STATES.with(|accounts| {
            let accounts = accounts.borrow();
            let b_acc = accounts.get(&buyer).unwrap();
            assert_eq!(
                b_acc.cash_balance_usd, 2_000_000_000,
                "Buyer's cash should NOT be debited on seller failure"
            );
        });
    }
}
