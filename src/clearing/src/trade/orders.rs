use crate::{
    memory::{ACCOUNT_STATES, LIMIT_ORDERS},
    types::trade::LimitOrder,
};

/// Removes every resting limit order `selects` matches and releases each
/// order's `blocked_margin_usd` from its creator's reserved margin in the
/// order's balance domain.
///
/// The removal and the release happen in one synchronous step, so no caller
/// can observe an order that is gone while its margin is still reserved, or
/// the reverse. Every cancellation path goes through here (fills take orders
/// off the book in the matching code instead), which keeps that bookkeeping in
/// one place.
///
/// Returns the removed orders; an empty vector when nothing matched.
pub(crate) fn cancel_resting_orders(selects: impl Fn(&LimitOrder) -> bool) -> Vec<LimitOrder> {
    let cancelled: Vec<LimitOrder> = LIMIT_ORDERS.with(|orders| {
        let mut orders = orders.borrow_mut();
        let order_ids: Vec<_> = orders
            .values()
            .filter(|order| selects(order))
            .map(|order| order.order_id.clone())
            .collect();

        order_ids
            .iter()
            .filter_map(|order_id| orders.remove(order_id))
            .collect()
    });

    if cancelled.is_empty() {
        return cancelled;
    }

    ACCOUNT_STATES.with(|accounts| {
        let mut accounts = accounts.borrow_mut();
        for order in &cancelled {
            if let Some(account) = accounts.get_mut(&order.creator) {
                let domain = order.balance_domain;
                let current = account.get_reserved_margin_usd(domain);
                account.set_reserved_margin_usd(
                    domain,
                    current.saturating_sub(order.blocked_margin_usd),
                );
            }
        }
    });

    cancelled
}

#[cfg(test)]
mod tests {
    use candid::Principal;
    use shared::types::{BalanceDomain, Price, SeriesId};

    use super::cancel_resting_orders;
    use crate::{
        memory::{ACCOUNT_STATES, LIMIT_ORDERS},
        types::{
            margin::AccountState,
            trade::{LimitOrder, OrderId, Side},
            user::User,
        },
    };

    fn user(id: u8) -> User {
        Principal::from_slice(&[id; 29]).into()
    }

    fn order_id(id: &str) -> OrderId {
        OrderId::from(id.to_owned())
    }

    fn order(id: &str, creator: User, series_id: &str, blocked_margin_usd: u128) -> LimitOrder {
        LimitOrder {
            order_id: order_id(id),
            creator,
            series_id: SeriesId::from(series_id.to_owned()),
            outcome_id: None,
            side: Side::Buy,
            qty: 1,
            price: Price::new(1, 2),
            blocked_margin_usd,
            balance_domain: BalanceDomain::Settlement,
        }
    }

    fn reserved_margin(user: User) -> u128 {
        ACCOUNT_STATES.with(|accounts| {
            accounts.borrow().get(&user).map_or(0, |account| {
                account.get_reserved_margin_usd(BalanceDomain::Settlement)
            })
        })
    }

    #[test]
    fn cancel_resting_orders_removes_matches_and_releases_their_margin() {
        let alice = user(1);
        let bob = user(2);

        ACCOUNT_STATES.with(|accounts| {
            let mut accounts = accounts.borrow_mut();
            accounts.clear();
            for (owner, reserved) in [(alice, 500), (bob, 70)] {
                let mut account = AccountState::new(owner);
                account.set_reserved_margin_usd(BalanceDomain::Settlement, reserved);
                accounts.insert(owner, account);
            }
        });
        LIMIT_ORDERS.with(|orders| {
            let mut orders = orders.borrow_mut();
            orders.clear();
            for o in [
                order("a1", alice, "s1", 100),
                order("a2", alice, "s2", 300),
                order("b1", bob, "s1", 70),
            ] {
                orders.insert(o.order_id.clone(), o);
            }
        });

        let cancelled = cancel_resting_orders(|o| o.creator == alice);

        let mut cancelled_ids: Vec<_> = cancelled.into_iter().map(|o| o.order_id).collect();
        cancelled_ids.sort();
        assert_eq!(cancelled_ids, vec![order_id("a1"), order_id("a2")]);
        // The 100 left over shows the release subtracts the orders' margin
        // rather than zeroing the account's reservation.
        assert_eq!(reserved_margin(alice), 100);
        assert_eq!(reserved_margin(bob), 70);
        let remaining: Vec<_> =
            LIMIT_ORDERS.with(|orders| orders.borrow().keys().cloned().collect());
        assert_eq!(remaining, vec![order_id("b1")]);

        assert!(cancel_resting_orders(|o| o.creator == alice).is_empty());
        assert_eq!(reserved_margin(alice), 100);

        LIMIT_ORDERS.with(|orders| orders.borrow_mut().clear());
        ACCOUNT_STATES.with(|accounts| accounts.borrow_mut().clear());
    }
}
