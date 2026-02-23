use std::collections::BTreeMap;

use candid::{CandidType, Principal};
use icrc_ledger_types::icrc1::account::{Account, Subaccount};
use serde::{Deserialize, Serialize};
use shared::types::{Asset, Series, SeriesId};

use crate::{
    memory::{DEPOSIT_PLANS, WITHDRAWAL_PLANS},
    traits::ClearingAccountExt,
};

#[derive(CandidType, Deserialize, Clone, Debug)]
pub struct PositionProof {
    pub user: User,
    pub series_id: SeriesId,
    pub qty: i128,
    pub clearing_id: Principal,
    pub signature: Vec<u8>,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DepositId(String);

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct WithdrawalId(String);

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum PlanStatus {
    Planned,
    Executing,
    Finalised,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum PaymentIdempotency {
    IcrcCreatedAtTime(u64), // created_at_time of the transfer that initiated the payment
}
impl PaymentIdempotency {
    pub fn to_created_at_time(&self) -> Option<u64> {
        match self {
            PaymentIdempotency::IcrcCreatedAtTime(time) => Some(*time),
        }
    }
}
impl From<u64> for PaymentIdempotency {
    fn from(value: u64) -> Self {
        PaymentIdempotency::IcrcCreatedAtTime(value)
    }
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum PaymentReceipt {
    IcrcBlockIndex(candid::Nat),
}
impl From<candid::Nat> for PaymentReceipt {
    fn from(value: candid::Nat) -> Self {
        PaymentReceipt::IcrcBlockIndex(value)
    }
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct DepositPlan {
    pub deposit_id: DepositId,
    pub user: User,
    pub asset: Asset,
    pub amount: candid::Nat,
    pub to_account: Account,
    pub status: PlanStatus,
    pub idempotency: PaymentIdempotency,
    pub receipt: Option<PaymentReceipt>,
}
impl DepositPlan {
    pub fn get_or_create(
        deposit_id: DepositId,
        user: User,
        asset: Asset,
        amount: candid::Nat,
    ) -> Self {
        DEPOSIT_PLANS.with(|m| {
            let mut m = m.borrow_mut();

            if let Some(existing) = m.get(&deposit_id) {
                return existing.clone();
            }

            let to_account = user.clearing_account();

            let idempotency = PaymentIdempotency::IcrcCreatedAtTime(ic_cdk::api::time());

            let plan = DepositPlan {
                deposit_id: deposit_id.clone(),
                user,
                asset,
                amount,
                to_account,
                status: PlanStatus::Planned,
                idempotency,
                receipt: None,
            };

            m.insert(deposit_id, plan.clone());
            plan
        })
    }
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct WithdrawalPlan {
    pub withdrawal_id: WithdrawalId,
    pub user: User,
    pub asset: Asset,
    pub amount: candid::Nat,
    pub from_subaccount: Subaccount,
    pub to_account: Account,
    pub status: PlanStatus,
    pub idempotency: PaymentIdempotency,
    pub receipt: Option<PaymentReceipt>,
}
impl WithdrawalPlan {
    pub fn get_or_create(
        withdrawal_id: WithdrawalId,
        user: User,
        asset: Asset,
        amount: candid::Nat,
    ) -> Self {
        WITHDRAWAL_PLANS.with(|m| {
            let mut m = m.borrow_mut();

            if let Some(existing) = m.get(&withdrawal_id) {
                return existing.clone();
            }

            let from_subaccount = user.clearing_subaccount();

            let plan = WithdrawalPlan {
                withdrawal_id: withdrawal_id.clone(),
                user,
                asset: asset.clone(),
                amount: amount.clone(),
                from_subaccount,
                to_account: Account {
                    owner: user.principal(),
                    subaccount: None,
                },
                status: PlanStatus::Planned,
                idempotency: ic_cdk::api::time().into(),
                receipt: None,
            };

            m.insert(withdrawal_id.clone(), plan.clone());
            plan
        })
    }
}

#[derive(
    CandidType, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
pub struct User(pub Principal);
impl User {
    pub fn principal(self) -> Principal {
        self.0
    }
}
impl From<Principal> for User {
    fn from(p: Principal) -> Self {
        Self(p)
    }
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct Position {
    pub user: User,
    pub series_id: SeriesId,
    pub net_qty: i128,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct MarginAccount {
    pub user: User,
    pub balances: BTreeMap<Asset, u128>, // (Asset, Balance)
    pub required_margin: u128,
}
impl MarginAccount {
    pub fn get_balance(&self, asset: &Asset) -> u128 {
        *self.balances.get(asset).unwrap_or(&0)
    }

    pub fn set_balance(&mut self, asset: Asset, amount: u128) {
        self.balances.insert(asset, amount);
    }

    pub fn tracked_assets(&self) -> Vec<Asset> {
        self.balances.keys().cloned().collect()
    }
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum EventType {
    OrderPlaced,
    Executed,
    Settled,
    Liquidated,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct Event {
    pub event_id: u64,
    pub clearing_id: Principal,
    pub series_id: SeriesId,
    pub user: User,
    pub qty: i128,
    pub price: u64,
    pub event_type: EventType,
    pub timestamp: u64,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct StableState {
    pub positions: Vec<Position>,
    pub accounts: BTreeMap<User, MarginAccount>,
    pub series: BTreeMap<SeriesId, Series>,
    pub events: Vec<Event>,
    pub next_id: u64,
    pub registry: Principal,
    pub deposit_plans: BTreeMap<DepositId, DepositPlan>,
    pub withdrawal_plans: BTreeMap<WithdrawalId, WithdrawalPlan>,
}
