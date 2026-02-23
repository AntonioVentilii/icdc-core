use std::{cell::RefCell, collections::BTreeMap};

use candid::Principal;
use ic_cdk::storage;
use shared::{
    constants::{CKUSDC_LEDGER, ICP_LEDGER},
    types::{Series, SeriesId},
};

use crate::{
    types::{
        event::Event,
        margin::{MarginAccount, Position},
        plan::{DepositPlan, SettlementPlan, WithdrawalPlan},
        state::StableState,
        trade::{TradeId, TransferId},
        user::{DepositKey, User, WithdrawalKey},
    },
    PositionProof,
};

thread_local! {
    pub static POSITIONS: RefCell<BTreeMap<(User, SeriesId), Position>> = const { RefCell::new(BTreeMap::new()) };
    pub static MARGIN_ACCOUNTS: RefCell<BTreeMap<User, MarginAccount>> = const { RefCell::new(BTreeMap::new()) };
    pub static SERIES: RefCell<BTreeMap<SeriesId, Series>> = const { RefCell::new(BTreeMap::new()) };
    pub static EVENTS: RefCell<Vec<Event>> = const { RefCell::new(Vec::new()) };
    pub static NEXT_EVENT_ID: RefCell<u64> = const { RefCell::new(0) };
    pub static REGISTRY_CANISTER: RefCell<Principal> = const { RefCell::new(Principal::anonymous()) };
    pub static DEPOSIT_PLANS: RefCell<BTreeMap<DepositKey, DepositPlan>> = const { RefCell::new(BTreeMap::new()) };
    pub static WITHDRAWAL_PLANS: RefCell<BTreeMap<WithdrawalKey, WithdrawalPlan>> = const { RefCell::new(BTreeMap::new()) };
    pub static EXECUTED_TRADES: RefCell<BTreeMap<TradeId, u64>> = const { RefCell::new(BTreeMap::new()) };
    pub static FROZEN_TRANSFERS: RefCell<BTreeMap<TransferId, PositionProof>> = const { RefCell::new(BTreeMap::new()) };
    pub static ACCEPTED_TRANSFERS: RefCell<BTreeMap<TransferId, bool>> = const { RefCell::new(BTreeMap::new()) };
    pub static SETTLEMENT_PLANS: RefCell<BTreeMap<SeriesId, SettlementPlan>> = const { RefCell::new(BTreeMap::new()) }
}

pub fn save_state() {
    let positions: Vec<Position> = POSITIONS.with(|p| p.borrow().values().cloned().collect());
    let accounts: BTreeMap<User, MarginAccount> = MARGIN_ACCOUNTS.with(|a| a.borrow().clone());
    let series: BTreeMap<SeriesId, Series> = SERIES.with(|s| s.borrow().clone());
    let events: Vec<Event> = EVENTS.with(|e| e.borrow().clone());
    let next_id: u64 = NEXT_EVENT_ID.with(|id| *id.borrow());
    let registry: Principal = REGISTRY_CANISTER.with(|r| *r.borrow());
    let deposit_plans: BTreeMap<DepositKey, DepositPlan> =
        DEPOSIT_PLANS.with(|d| d.borrow().clone());
    let withdrawal_plans: BTreeMap<WithdrawalKey, WithdrawalPlan> =
        WITHDRAWAL_PLANS.with(|w| w.borrow().clone());
    let executed_trades: BTreeMap<TradeId, u64> = EXECUTED_TRADES.with(|t| t.borrow().clone());
    let frozen_transfers: BTreeMap<TransferId, PositionProof> =
        FROZEN_TRANSFERS.with(|t| t.borrow().clone());
    let accepted_transfers: BTreeMap<TransferId, bool> =
        ACCEPTED_TRANSFERS.with(|t| t.borrow().clone());
    let settlement_plans: BTreeMap<SeriesId, SettlementPlan> =
        SETTLEMENT_PLANS.with(|s| s.borrow().clone());

    let state = StableState {
        positions,
        accounts,
        series,
        events,
        next_id,
        registry,
        deposit_plans,
        withdrawal_plans,
        executed_trades,
        frozen_transfers,
        accepted_transfers,
        settlement_plans,
    };

    storage::stable_save((state,)).expect("Save failed");
}

pub fn restore_state() {
    let (state,): (StableState,) = storage::stable_restore().expect("Restore failed");

    let StableState {
        positions,
        accounts,
        series,
        events,
        next_id,
        registry,
        deposit_plans,
        withdrawal_plans,
        executed_trades,
        frozen_transfers,
        accepted_transfers,
        settlement_plans,
    } = state;

    POSITIONS.with(|p| {
        let mut p = p.borrow_mut();
        for pos in positions {
            p.insert((pos.user, pos.series_id.clone()), pos);
        }
    });

    MARGIN_ACCOUNTS.with(|w| *w.borrow_mut() = accounts);
    SERIES.with(|s| *s.borrow_mut() = series);
    EVENTS.with(|e| *e.borrow_mut() = events);
    NEXT_EVENT_ID.with(|id| *id.borrow_mut() = next_id);
    REGISTRY_CANISTER.with(|r| *r.borrow_mut() = registry);
    DEPOSIT_PLANS.with(|d| *d.borrow_mut() = deposit_plans);
    WITHDRAWAL_PLANS.with(|w| *w.borrow_mut() = withdrawal_plans);
    EXECUTED_TRADES.with(|t| *t.borrow_mut() = executed_trades);
    FROZEN_TRANSFERS.with(|t| *t.borrow_mut() = frozen_transfers);
    ACCEPTED_TRANSFERS.with(|t| *t.borrow_mut() = accepted_transfers);
    SETTLEMENT_PLANS.with(|s| *s.borrow_mut() = settlement_plans);
}

pub fn icp_ledger() -> Principal {
    Principal::from_text(ICP_LEDGER).expect("invalid ICP_LEDGER")
}

pub fn ckusdc_ledger() -> Principal {
    Principal::from_text(CKUSDC_LEDGER).expect("invalid CKUSDC_LEDGER")
}
