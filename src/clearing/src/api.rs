use candid::{Nat, Principal};
use ic_cdk::api::time;
use ic_cdk_macros::{query, update};
use shared::{Event, EventType, MarginAccount, Position};

use crate::{
    memory::{EVENTS, MARGIN_ACCOUNTS, NEXT_EVENT_ID, POSITIONS},
    types::PositionProof,
};

#[update]
pub fn deposit_collateral(amount: Nat) {
    let caller = ic_cdk::caller();
    let amount_u128: u128 = amount.0.try_into().unwrap_or(0);

    MARGIN_ACCOUNTS.with(|accounts| {
        let mut accounts = accounts.borrow_mut();
        let account = accounts.entry(caller).or_insert(MarginAccount {
            user: caller,
            collateral_balance: 0,
            required_margin: 0,
        });
        account.collateral_balance += amount_u128;
    });
}

#[update]
pub fn withdraw_collateral(amount: Nat) {
    let caller = ic_cdk::caller();
    let amount_u128: u128 = amount.0.try_into().unwrap_or(0);

    MARGIN_ACCOUNTS.with(|accounts| {
        let mut accounts = accounts.borrow_mut();
        if let Some(account) = accounts.get_mut(&caller) {
            if account.collateral_balance >= amount_u128 + account.required_margin {
                account.collateral_balance -= amount_u128;
            } else {
                ic_cdk::trap("Insufficient excess margin");
            }
        } else {
            ic_cdk::trap("No margin account found");
        }
    });
}

#[update]
pub fn submit_matched_trade(
    series_id: String,
    buyer: Principal,
    seller: Principal,
    qty: i128,
    price: u64,
) -> bool {
    let required_margin = qty.unsigned_abs() * (price as u128) / 1000000;

    MARGIN_ACCOUNTS.with(|accounts| {
        let mut accounts = accounts.borrow_mut();

        let buyer_account = accounts.entry(buyer).or_insert(MarginAccount {
            user: buyer,
            collateral_balance: 0,
            required_margin: 0,
        });
        buyer_account.required_margin += required_margin;
        if buyer_account.required_margin > buyer_account.collateral_balance {
            ic_cdk::trap("Buyer insufficient margin");
        }

        let seller_account = accounts.entry(seller).or_insert(MarginAccount {
            user: seller,
            collateral_balance: 0,
            required_margin: 0,
        });
        seller_account.required_margin += required_margin;
        if seller_account.required_margin > seller_account.collateral_balance {
            ic_cdk::trap("Seller insufficient margin");
        }
    });

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

    true
}

#[query]
pub fn get_position(user: Principal, series_id: String) -> Option<Position> {
    POSITIONS.with(|positions| positions.borrow().get(&(user, series_id)).cloned())
}

#[query]
pub fn get_margin_account(user: Principal) -> Option<MarginAccount> {
    MARGIN_ACCOUNTS.with(|accounts| accounts.borrow().get(&user).cloned())
}

#[update]
pub fn settle_series(series_id: String, settlement_price: u64) {
    POSITIONS.with(|positions| {
        let mut positions = positions.borrow_mut();
        let users: Vec<Principal> = positions
            .keys()
            .filter(|(_, sid)| *sid == series_id)
            .map(|(u, _)| *u)
            .collect();

        for user in users {
            if let Some(pos) = positions.remove(&(user, series_id.clone())) {
                let payoff = (pos.net_qty as f64) * (settlement_price as f64);

                MARGIN_ACCOUNTS.with(|accounts| {
                    let mut accounts = accounts.borrow_mut();
                    if let Some(account) = accounts.get_mut(&user) {
                        if payoff >= 0.0 {
                            account.collateral_balance += payoff as u128;
                        } else {
                            account.collateral_balance -= payoff.abs() as u128;
                        }
                        account.required_margin = 0;
                    }
                });
            }
        }
    });
}

#[update]
pub fn freeze_position_for_transfer(user: Principal, series_id: String) -> Option<PositionProof> {
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

#[update]
pub fn accept_position_transfer(proof: PositionProof) -> bool {
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
    true
}
