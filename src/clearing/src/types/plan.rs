use candid::CandidType;
use icrc_ledger_types::icrc1::account::{Account, Subaccount};
use serde::{Deserialize, Serialize};
use shared::types::Asset;

use crate::{
    memory::{DEPOSIT_PLANS, WITHDRAWAL_PLANS},
    traits::ClearingAccountExt,
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
    pub reserved_amount: Option<u128>,
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
                reserved_amount: None,
            };

            m.insert(withdrawal_id.clone(), plan.clone());
            plan
        })
    }
}
