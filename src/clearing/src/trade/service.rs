use core::cell::RefCell;

use shared::types::{BalanceDomain, Series};

use crate::{
    api::trade::errors::TradeError,
    memory::{
        index_executed_trade, ACCOUNT_STATES, ASSET_METRICS, COLLATERAL_ASSETS, EVENTS,
        EXECUTED_TRADES, NEXT_EVENT_ID, POSITIONS,
    },
    payoffs::get_required_margin,
    trade::types::ExecuteTradeParams,
    types::{
        errors::CommonError,
        event::{Event, EventType, SeriesTradePoint},
        margin::{AccountState, Position, PositionsMap},
    },
    utils::{
        series::ensure_series_registered,
        system::{canister_id, now_ns},
    },
};

/// Internal shared logic for executing a trade and updating positions/margin.
pub(crate) async fn internal_execute_trade(params: ExecuteTradeParams) -> Result<bool, TradeError> {
    let series_id = params.series_id.clone();

    let series = ensure_series_registered(&series_id).await?;

    execute_trade_impl(&series, params)
}

/// Internal implementation of trade execution.
///
/// 1. Calculates margin requirements and cash flows.
/// 2. Validates solvency for both parties.
/// 3. Atomically updates account balances and positions.
/// 4. Emits execution events.
pub(crate) fn execute_trade_impl(
    series: &Series,
    params: ExecuteTradeParams,
) -> Result<bool, TradeError> {
    if EXECUTED_TRADES.with(|m| m.borrow().contains_key(&params.trade_id)) {
        return Ok(true);
    }

    if params.buyer == params.seller {
        return Err(TradeError::SelfTradingNotAllowed);
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
        outcome_id,
    } = params;

    let configs = COLLATERAL_ASSETS.with(|c| c.borrow().clone());
    let metrics = ASSET_METRICS.with(|m| m.borrow().clone());

    let (
        buyer_cash_delta,
        seller_cash_delta,
        buyer_reserved_delta,
        seller_reserved_delta,
        new_buyer_margin_usd,
        new_seller_margin_usd,
    ): (i128, i128, i128, i128, u128, u128) =
        POSITIONS.with(|positions: &RefCell<PositionsMap>| {
            let positions = positions.borrow();

            let old_buyer_margin = positions
                .get(&(buyer, series_id.clone(), outcome_id.clone()))
                .map_or(0, |p| p.reserved_margin_usd);
            let old_seller_margin = positions
                .get(&(seller, series_id.clone(), outcome_id.clone()))
                .map_or(0, |p| p.reserved_margin_usd);

            let old_buyer_qty = positions
                .get(&(buyer, series_id.clone(), outcome_id.clone()))
                .map_or(0, |p| p.net_qty);
            let old_seller_qty = positions
                .get(&(seller, series_id.clone(), outcome_id.clone()))
                .map_or(0, |p| p.net_qty);

            let new_buyer_qty = old_buyer_qty + qty;
            let new_seller_qty = old_seller_qty - qty;

            let new_buyer_margin_usd =
                get_required_margin(series, &price, new_buyer_qty, &outcome_id).map_err(|e| {
                    TradeError::Common(CommonError::Internal(format!(
                        "Payoff calculation failed: {e:?}"
                    )))
                })?;
            let new_seller_margin_usd =
                get_required_margin(series, &price, new_seller_qty, &outcome_id).map_err(|e| {
                    TradeError::Common(CommonError::Internal(format!(
                        "Payoff calculation failed: {e:?}"
                    )))
                })?;

            let old_buyer_margin_at_price =
                get_required_margin(series, &price, old_buyer_qty, &outcome_id).map_err(|e| {
                    TradeError::Common(CommonError::Internal(format!(
                        "Payoff calculation failed: {e:?}"
                    )))
                })?;
            let old_seller_margin_at_price =
                get_required_margin(series, &price, old_seller_qty, &outcome_id).map_err(|e| {
                    TradeError::Common(CommonError::Internal(format!(
                        "Payoff calculation failed: {e:?}"
                    )))
                })?;

            // Mark-to-Market Cashflow Model:
            // We calculate the realized PnL of the existing position
            // by comparing its cost-basis (old_margin) to its value at
            // the CURRENT trade price (old_margin_at_price).
            let buyer_cash_delta =
                (old_buyer_margin.cast_signed()) - (old_buyer_margin_at_price.cast_signed());
            let seller_cash_delta =
                (old_seller_margin.cast_signed()) - (old_seller_margin_at_price.cast_signed());

            // Margin Delta:
            // We calculate how much the TOTAL reserved margin in the account should change.
            // This must account for:
            // 1. The change in the position's margin requirement (new_margin - old_pos_margin)
            // 2. The release of the specific order's collateral (unblock_amount)
            let b_reserved_delta = (new_buyer_margin_usd.cast_signed())
                - (old_buyer_margin.cast_signed())
                - (buyer_unblock_amount.unwrap_or(0).cast_signed());
            let s_reserved_delta = (new_seller_margin_usd.cast_signed())
                - (old_seller_margin.cast_signed())
                - (seller_unblock_amount.unwrap_or(0).cast_signed());

            Ok((
                buyer_cash_delta,
                seller_cash_delta,
                b_reserved_delta,
                s_reserved_delta,
                new_buyer_margin_usd,
                new_seller_margin_usd,
            ))
        })?;

    // Atomicity: Validation and Calculation Phase
    // No state and no memory is mutated during this phase.
    // If ANY check fails (InsufficientMargin, etc.), we return early with an Err,
    // leaving the system state exactly as it was.
    ACCOUNT_STATES.with(|accounts| {
        let accounts = accounts.borrow();
        let domain = series.balance_domain;

        if domain == BalanceDomain::Social {
            return Ok(());
        }

        // 1. Validate Buyer
        // We check if the post-trade equity covers the target reserved margin.
        let buyer_acc = accounts
            .get(&buyer)
            .cloned()
            .unwrap_or_else(|| AccountState::new(buyer));

        // Calculate target reserved margin based on the delta.
        let target_buyer_reserved = if buyer_reserved_delta > 0 {
            buyer_acc.get_reserved_margin_usd(domain) + (buyer_reserved_delta.cast_unsigned())
        } else {
            buyer_acc
                .get_reserved_margin_usd(domain)
                .saturating_sub(buyer_reserved_delta.unsigned_abs())
        };

        let buyer_equity = buyer_acc.calculate_raw_equity_i128(domain, &configs, &metrics);

        if buyer_equity < (target_buyer_reserved.cast_signed()) {
            return Err(TradeError::InsufficientMargin {
                user: buyer,
                balance: buyer_acc.calculate_equity_usd(domain, &configs, &metrics),
                required: target_buyer_reserved,
            });
        }

        // 2. Validate Seller
        let seller_acc = accounts
            .get(&seller)
            .cloned()
            .unwrap_or_else(|| AccountState::new(seller));

        let target_seller_reserved = if seller_reserved_delta > 0 {
            seller_acc.get_reserved_margin_usd(domain) + (seller_reserved_delta.cast_unsigned())
        } else {
            seller_acc
                .get_reserved_margin_usd(domain)
                .saturating_sub(seller_reserved_delta.unsigned_abs())
        };

        let seller_equity = seller_acc.calculate_raw_equity_i128(domain, &configs, &metrics);
        if seller_equity < (target_seller_reserved.cast_signed()) {
            return Err(TradeError::InsufficientMargin {
                user: seller,
                balance: seller_acc.calculate_equity_usd(domain, &configs, &metrics),
                required: target_seller_reserved,
            });
        }

        Ok(())
    })?;

    // Mutation Phase: Apply all changes only after all validations pass.
    ACCOUNT_STATES.with(|accounts| {
        let mut accounts = accounts.borrow_mut();
        let domain = series.balance_domain;

        // Update Buyer
        let buyer_acc = accounts
            .entry(buyer)
            .or_insert_with(|| AccountState::new(buyer));
        let buyer_cash = buyer_acc.get_cash_balance_usd(domain);
        buyer_acc.set_cash_balance_usd(domain, buyer_cash - buyer_cash_delta);

        // If buyer == seller, this is the same object. We must fetch again if we want to be safe,
        // but here we can just update it.
        let current_reserved = buyer_acc.get_reserved_margin_usd(domain);
        if buyer_reserved_delta > 0 {
            buyer_acc.set_reserved_margin_usd(
                domain,
                current_reserved + (buyer_reserved_delta.cast_unsigned()),
            );
        } else {
            buyer_acc.set_reserved_margin_usd(
                domain,
                current_reserved.saturating_sub(buyer_reserved_delta.unsigned_abs()),
            );
        }

        // Update Seller
        let seller_acc = accounts
            .entry(seller)
            .or_insert_with(|| AccountState::new(seller));
        let seller_cash = seller_acc.get_cash_balance_usd(domain);
        seller_acc.set_cash_balance_usd(domain, seller_cash - seller_cash_delta);

        let current_reserved = seller_acc.get_reserved_margin_usd(domain);
        if seller_reserved_delta > 0 {
            seller_acc.set_reserved_margin_usd(
                domain,
                current_reserved + (seller_reserved_delta.cast_unsigned()),
            );
        } else {
            seller_acc.set_reserved_margin_usd(
                domain,
                current_reserved.saturating_sub(seller_reserved_delta.unsigned_abs()),
            );
        }
    });

    POSITIONS.with(|positions: &RefCell<PositionsMap>| {
        let mut positions = positions.borrow_mut();

        let b_pos = positions
            .entry((buyer, series_id.clone(), outcome_id.clone()))
            .or_insert(Position {
                user: buyer,
                series_id: series_id.clone(),
                outcome_id: outcome_id.clone(),
                net_qty: 0,
                reserved_margin_usd: 0,
            });
        b_pos.net_qty += qty;
        b_pos.reserved_margin_usd = new_buyer_margin_usd;

        let s_pos = positions
            .entry((seller, series_id.clone(), outcome_id.clone()))
            .or_insert(Position {
                user: seller,
                series_id: series_id.clone(),
                outcome_id,
                net_qty: 0,
                reserved_margin_usd: 0,
            });
        s_pos.net_qty -= qty;
        s_pos.reserved_margin_usd = new_seller_margin_usd;
    });

    let event_id = NEXT_EVENT_ID.with(|id| {
        let mut id = id.borrow_mut();
        let current = *id;
        *id += 1;
        current
    });

    // Single timestamp for the trade so both counterparty rows — and the
    // price-history point derived from them — agree.
    let timestamp = now_ns();

    EVENTS.with(|events| {
        let mut events = events.borrow_mut();

        // Buyer Event
        events.push(Event {
            event_id,
            clearing_id: canister_id(),
            series_id: series_id.clone(),
            user: buyer,
            qty,
            price: price.clone(),
            event_type: EventType::Executed,
            timestamp,
        });

        // Seller Event (using same event_id or should it be different? Usually history shows the
        // interaction) We use the same event_id as it's the same trade, but now indexed for
        // both users.
        events.push(Event {
            event_id,
            clearing_id: canister_id(),
            series_id: series_id.clone(),
            user: seller,
            qty: -qty,
            price: price.clone(),
            event_type: EventType::Executed,
            timestamp,
        });
    });

    // Maintain the per-series price-history index incrementally (one point per
    // trade), keeping `list_series_trade_history` reads off the full event log.
    index_executed_trade(
        &series_id,
        SeriesTradePoint {
            event_id,
            price,
            qty,
            timestamp,
        },
    );

    EXECUTED_TRADES.with(|m| {
        m.borrow_mut().insert(trade_id, event_id);
    });

    Ok(true)
}

#[cfg(test)]
mod tests {
    use candid::Principal;
    use shared::types::{
        BalanceDomain, Description, PayoffType, PayoutUnit, Price, Resolution, Series, SeriesId,
    };

    use crate::{
        api::trade::errors::TradeError,
        memory::{ACCOUNT_STATES, SERIES},
        trade::{service::execute_trade_impl, types::ExecuteTradeParams},
        types::{margin::AccountState, trade::TradeId, user::User},
    };

    #[test]
    fn trade_atomicity_on_failure() {
        let buyer_p = Principal::from_slice(&[1]);
        let seller_p = Principal::from_slice(&[2]);
        let buyer = User(buyer_p);
        let seller = User(seller_p);
        let series_id = SeriesId::from("test".to_owned());

        let series = Series {
            resolution: Resolution::new("Resolved per oracle at expiry"),
            series_id: series_id.clone(),
            underlying: "BTC".to_owned(),
            expiry_ns: 2_000_000_000,
            payoff_type: PayoffType::Call,
            strike: Some(Price::new(50_000_000_000, 8)),
            settlement_cap: None,
            price_precision: 8,
            payout_unit: PayoutUnit::usd(),
            oracle_source: "oracle".to_owned(),
            creator: Principal::anonymous(),
            created_at_ns: 1_000_000_000,
            title: "Test".to_owned(),
            description: Description::plain("Test Description"),
            outcomes: None,
            icon_url: None,
            banner_url: None,
            balance_domain: BalanceDomain::Settlement,
            trading_access: vec![],
            engine_id: None,
            forked_from: None,
            locale: None,
        };

        // Initialize state
        SERIES.with(|s| s.borrow_mut().insert(series_id.clone(), series.clone()));

        ACCOUNT_STATES.with(|accounts| {
            let mut accounts = accounts.borrow_mut();
            accounts.clear();

            // Buyer has enough margin ($2000)
            let mut b_acc = AccountState::new(buyer);
            b_acc.set_cash_balance_usd(BalanceDomain::Settlement, 20_000_000);
            accounts.insert(buyer, b_acc);

            // Seller has NO margin
            let mut s_acc = AccountState::new(seller);
            s_acc.set_cash_balance_usd(BalanceDomain::Settlement, 0);
            accounts.insert(seller, s_acc);
        });

        let params = ExecuteTradeParams {
            trade_id: TradeId::from("trade_1".to_owned()),
            series_id: series_id.clone(),
            buyer,
            seller,
            qty: 1,                               // 1 unit
            price: Price::new(60_000_000_000, 8), // $600
            buyer_unblock_amount: None,
            seller_unblock_amount: None,
            outcome_id: None,
        };

        // Trade should fail for seller due to insufficient margin
        let result = execute_trade_impl(&series, params);
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
                b_acc.get_cash_balance_usd(BalanceDomain::Settlement),
                20_000_000,
                "Buyer's cash should NOT be debited on seller failure"
            );
        });
    }

    #[test]
    fn self_trading() {
        let user_p = Principal::from_slice(&[1]);
        let user = User(user_p);
        let series_id = SeriesId::from("test".to_owned());

        let series = Series {
            resolution: Resolution::new("Resolved per oracle at expiry"),
            series_id: series_id.clone(),
            underlying: "BTC".to_owned(),
            expiry_ns: 2_000_000_000,
            payoff_type: PayoffType::Call,
            strike: Some(Price::new(50_000_000_000, 8)),
            settlement_cap: None,
            price_precision: 8,
            payout_unit: PayoutUnit::usd(),
            oracle_source: "oracle".to_owned(),
            creator: Principal::anonymous(),
            created_at_ns: 1_000_000_000,
            title: "Test".to_owned(),
            description: Description::plain("Test Description"),
            outcomes: None,
            icon_url: None,
            banner_url: None,
            balance_domain: BalanceDomain::Settlement,
            trading_access: vec![],
            engine_id: None,
            forked_from: None,
            locale: None,
        };

        let params = ExecuteTradeParams {
            trade_id: TradeId::from("trade_self".to_owned()),
            series_id: series_id.clone(),
            buyer: user,
            seller: user,
            qty: 1,
            price: Price::new(60_000_000_000, 8),
            buyer_unblock_amount: None,
            seller_unblock_amount: None,
            outcome_id: None,
        };

        let result = execute_trade_impl(&series, params);
        assert!(matches!(result, Err(TradeError::SelfTradingNotAllowed)));
    }
}
