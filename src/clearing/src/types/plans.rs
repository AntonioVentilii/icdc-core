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
pub struct DepositPlanParams {
    pub deposit_id: DepositId,
    pub user: User,
    pub asset: Asset,
    pub amount: candid::Nat,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct WithdrawalPlanParams {
    pub withdrawal_id: WithdrawalId,
    pub user: User,
    pub asset: Asset,
    pub amount: candid::Nat,
    pub to_account: (candid::Principal, Option<[u8; 32]>),
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct SettlementPlanParams {
    pub series_id: SeriesId,
    pub settlement_price: u64,
    pub settlement_asset: Asset,
    pub fee: u128,
    pub positions: Vec<(User, i128)>,
    pub payers: Vec<(User, u128)>,
    pub receivers: Vec<(User, u128)>,
    pub accounting_updates: Vec<(User, i8, u128, u128)>,
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
    pub fn get_or_create(params: DepositPlanParams) -> Self {
        DEPOSIT_PLANS.with(|m| {
            let mut m = m.borrow_mut();

            let DepositPlanParams {
                deposit_id,
                user,
                asset,
                amount,
            } = params;

            let key = (user, deposit_id.clone());

            if let Some(existing) = m.get(&key) {
                return existing.clone();
            }

            let idempotency = PaymentIdempotency::IcrcCreatedAtTime(ic_cdk::api::time());

            let plan = DepositPlan {
                deposit_id,
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
    pub fn get_or_create(params: WithdrawalPlanParams) -> Self {
        WITHDRAWAL_PLANS.with(|m| {
            let mut m = m.borrow_mut();

            let WithdrawalPlanParams {
                withdrawal_id,
                user,
                asset,
                amount,
                to_account,
            } = params;

            let key = (user, withdrawal_id.clone());

            if let Some(existing) = m.get(&key) {
                return existing.clone();
            }

            let plan = WithdrawalPlan {
                withdrawal_id,
                user,
                asset,
                amount,
                to_account,
                status: PlanStatus::Planned,
                idempotency: ic_cdk::api::time().into(),
                receipt: None,
                reserved_amount: None,
            };

            m.insert(key, plan.clone());
            plan
        })
    }
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct SettlementPlan {
    pub series_id: SeriesId,
    pub settlement_price: u64,
    pub settlement_asset: Asset,
    pub fee: u128,
    pub positions: Vec<(User, i128)>,
    pub payers: Vec<(User, u128)>,
    pub receivers: Vec<(User, u128)>,
    pub accounting_updates: Vec<(User, i8, u128, u128)>, /* (user, sign, profit_loss,
                                                          * margin_to_release) */
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

    pub fn get_or_create(params: SettlementPlanParams) -> Self {
        SETTLEMENT_PLANS.with(|m| {
            let mut m = m.borrow_mut();

            let SettlementPlanParams {
                series_id,
                settlement_price,
                settlement_asset,
                fee,
                positions,
                payers,
                receivers,
                accounting_updates,
            } = params;

            let key = series_id.clone();

            if let Some(existing) = m.get(&key) {
                return existing.clone();
            }

            let idempotency = ic_cdk::api::time().into();

            let payers_len = payers.len();
            let receivers_len = receivers.len();

            let plan = SettlementPlan {
                series_id,
                settlement_price,
                settlement_asset,
                fee,
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
                payer_receipts: vec![None; payers_len],
                receiver_receipts: vec![None; receivers_len],
            };

            m.insert(key, plan.clone());
            plan
        })
    }
}
