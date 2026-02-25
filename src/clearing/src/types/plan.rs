use candid::CandidType;
use serde::{Deserialize, Serialize};
use shared::types::{Asset, SeriesId};

use crate::{
    memory::{DEPOSIT_PLANS, SETTLEMENT_PLANS, WITHDRAWAL_PLANS},
    types::{
        payment::{PaymentIdempotency, PaymentReceipt},
        user::{DepositId, User, WithdrawalId},
    },
};

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum PlanStatus {
    Planned,
    Executing,
    Finalised,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct DepositPlan {
    pub deposit_id: DepositId,
    pub user: User,
    pub asset: Asset,
    pub amount: candid::Nat,
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

            let key = (user, deposit_id.clone());

            if let Some(existing) = m.get(&key) {
                return existing.clone();
            }

            let idempotency = PaymentIdempotency::IcrcCreatedAtTime(ic_cdk::api::time());

            let plan = DepositPlan {
                deposit_id: deposit_id.clone(),
                user,
                asset,
                amount,
                status: PlanStatus::Planned,
                idempotency,
                receipt: None,
            };

            m.insert(key, plan.clone());
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
    pub to_account: (candid::Principal, Option<[u8; 32]>),
    pub status: PlanStatus,
    pub idempotency: PaymentIdempotency,
    pub receipt: Option<PaymentReceipt>,
    pub reserved_amount: Option<u128>,
}
impl WithdrawalPlan {
    pub fn get_or_create(
        withdrawal_id: WithdrawalId,
        user: User,
        asset: Asset,
        amount: candid::Nat,
        to_account: (candid::Principal, Option<[u8; 32]>),
    ) -> Self {
        WITHDRAWAL_PLANS.with(|m| {
            let mut m = m.borrow_mut();

            let key = (user, withdrawal_id.clone());

            if let Some(existing) = m.get(&key) {
                return existing.clone();
            }

            let plan = WithdrawalPlan {
                withdrawal_id: withdrawal_id.clone(),
                user,
                asset: asset.clone(),
                amount: amount.clone(),
                to_account,
                status: PlanStatus::Planned,
                idempotency: ic_cdk::api::time().into(),
                receipt: None,
                reserved_amount: None,
            };

            m.insert(key.clone(), plan.clone());
            plan
        })
    }
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct SettlementPlan {
    pub series_id: SeriesId,
    pub settlement_price: u64,
    pub settlement_asset: Asset,
    pub positions: Vec<(User, i128)>,
    pub payers: Vec<(User, u128)>,
    pub receivers: Vec<(User, u128)>,
    pub accounting_updates: Vec<(User, i8, u128, u128)>, // (user, sign, profit_loss, margin_to_release)
    pub payer_cursor: usize,
    pub receiver_cursor: usize,
    pub accounting_cursor: usize,
    pub accounting_applied: bool,
    pub status: PlanStatus,
    pub idempotency: PaymentIdempotency,
    pub payer_receipts: Vec<Option<PaymentReceipt>>,
    pub receiver_receipts: Vec<Option<PaymentReceipt>>,
}
impl SettlementPlan {
    pub fn payer_step(&self, idx: u32) -> u64 {
        idx as u64
    }

    pub fn receiver_step(&self, idx: u32) -> u64 {
        10_000u64 + (idx as u64)
    }

    pub fn get_or_create(
        series_id: SeriesId,
        settlement_price: u64,
        settlement_asset: Asset,
        positions: Vec<(User, i128)>,
        payers: Vec<(User, u128)>,
        receivers: Vec<(User, u128)>,
        accounting_updates: Vec<(User, i8, u128, u128)>,
    ) -> Self {
        SETTLEMENT_PLANS.with(|m| {
            let mut m = m.borrow_mut();

            if let Some(existing) = m.get(&series_id) {
                return existing.clone();
            }

            let idempotency = ic_cdk::api::time().into();

            let positions_len = positions.len();
            let plan = SettlementPlan {
                series_id: series_id.clone(),
                settlement_price,
                settlement_asset,
                positions,
                payers: payers.clone(),
                receivers: receivers.clone(),
                accounting_updates,
                payer_cursor: 0,
                receiver_cursor: 0,
                accounting_cursor: 0,
                accounting_applied: false,
                status: PlanStatus::Planned,
                idempotency,
                payer_receipts: vec![None; positions_len],
                receiver_receipts: vec![None; receivers.len()],
            };

            m.insert(series_id, plan.clone());
            plan
        })
    }
}
