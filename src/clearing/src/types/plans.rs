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

/// The execution status of a background operation plan.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum PlanStatus {
    /// The plan is created but execution has not started.
    Planned,
    /// The plan is currently being executed.
    Executing,
    /// The plan has been successfully completed.
    Finalised,
}

/// Input parameters for creating a [`DepositPlan`].
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct DepositPlanParams {
    /// Unique identifier for the deposit.
    pub deposit_id: DepositId,
    /// The user making the deposit.
    pub user: User,
    /// The asset being deposited.
    pub asset: Asset,
    /// The amount being deposited.
    pub amount: candid::Nat,
}

/// Input parameters for creating a [`WithdrawalPlan`].
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct WithdrawalPlanParams {
    /// Unique identifier for the withdrawal.
    pub withdrawal_id: WithdrawalId,
    /// The user making the withdrawal.
    pub user: User,
    /// The asset being withdrawn.
    pub asset: Asset,
    /// The amount being withdrawn.
    pub amount: candid::Nat,
    /// The destination principal and optional subaccount.
    pub to_account: (candid::Principal, Option<[u8; 32]>),
}

/// Input parameters for creating a [`SettlementPlan`].
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct SettlementPlanParams {
    /// The unique identifier of the derivative series being settled.
    pub series_id: SeriesId,
    /// The final settlement price from the oracle.
    pub settlement_price: u64,
    /// The asset in which the settlement occurs.
    pub settlement_asset: Asset,
    /// The protocol fee applied to the settlement.
    pub fee: u128,
    /// A list of positions involved in the settlement.
    pub positions: Vec<(User, i128)>,
    /// Users who owe collateral for the settlement.
    pub payers: Vec<(User, u128)>,
    /// Users who are owed collateral for the settlement.
    pub receivers: Vec<(User, u128)>,
    /// Internal accounting updates required for the settlement.
    pub accounting_updates: Vec<(User, i8, u128, u128)>,
}

/// A plan for processing a collateral deposit in the background.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct DepositPlan {
    /// Unique identifier for the deposit.
    pub deposit_id: DepositId,
    /// The user making the deposit.
    pub user: User,
    /// The asset being deposited.
    pub asset: Asset,
    /// The amount being deposited.
    pub amount: candid::Nat,
    /// Current execution status of the plan.
    pub status: PlanStatus,
    /// Idempotency key in nanoseconds for ledger transfers.
    pub idempotency_ns: PaymentIdempotency,
    /// Proof of successful transfer, if completed.
    pub receipt: Option<PaymentReceipt>,
}
impl DepositPlan {
    /// Retrieves an existing deposit plan or creates a new one if it doesn't exist.
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

            let idempotency_ns = ic_cdk::api::time().into();

            let plan = DepositPlan {
                deposit_id,
                user,
                asset,
                amount,
                status: PlanStatus::Planned,
                idempotency_ns,
                receipt: None,
            };

            m.insert(key, plan.clone());
            plan
        })
    }
}

/// A plan for processing a collateral withdrawal in the background.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct WithdrawalPlan {
    /// Unique identifier for the withdrawal.
    pub withdrawal_id: WithdrawalId,
    /// The user making the withdrawal.
    pub user: User,
    /// The asset being withdrawn.
    pub asset: Asset,
    /// The amount being withdrawn.
    pub amount: candid::Nat,
    /// The destination principal and optional subaccount.
    pub to_account: (candid::Principal, Option<[u8; 32]>),
    /// Current execution status of the plan.
    pub status: PlanStatus,
    /// Idempotency key in nanoseconds for ledger transfers.
    pub idempotency_ns: PaymentIdempotency,
    /// Proof of successful transfer, if completed.
    pub receipt: Option<PaymentReceipt>,
    /// The amount successfully reserved for withdrawal.
    pub reserved_amount: Option<u128>,
}
impl WithdrawalPlan {
    /// Retrieves an existing withdrawal plan or creates a new one if it doesn't exist.
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

            let idempotency_ns = ic_cdk::api::time().into();

            let plan = WithdrawalPlan {
                withdrawal_id,
                user,
                asset,
                amount,
                to_account,
                status: PlanStatus::Planned,
                idempotency_ns,
                receipt: None,
                reserved_amount: None,
            };

            m.insert(key, plan.clone());
            plan
        })
    }
}

/// A plan for processing a derivative series settlement in the background.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct SettlementPlan {
    /// The unique identifier of the derivative series.
    pub series_id: SeriesId,
    /// The final settlement price.
    pub settlement_price: u64,
    /// The asset used for settlement.
    pub settlement_asset: Asset,
    /// The protocol fee.
    pub fee: u128,
    /// Detailed position snapshots at the time of settlement.
    pub positions: Vec<(User, i128)>,
    /// List of payers and their respective owed amounts.
    pub payers: Vec<(User, u128)>,
    /// List of receivers and their respective owed amounts.
    pub receivers: Vec<(User, u128)>,
    /// List of accounting updates: (user, sign, profit_loss, margin_to_release).
    pub accounting_updates: Vec<(User, i8, u128, u128)>,
    /// Tracks progress through the payers list.
    pub payer_cursor: usize,
    /// Tracks progress through the receivers list.
    pub receiver_cursor: usize,
    /// Tracks progress through accounting updates.
    pub accounting_cursor: usize,
    /// Whether all accounting updates have been applied to margin accounts.
    pub accounting_applied: bool,
    /// Current execution status of the plan.
    pub status: PlanStatus,
    /// Base idempotency key in nanoseconds for transfers.
    pub idempotency_ns: PaymentIdempotency,
    /// Receipts for successful payer transfers.
    pub payer_receipts: Vec<Option<PaymentReceipt>>,
    /// Receipts for successful receiver transfers.
    pub receiver_receipts: Vec<Option<PaymentReceipt>>,
}
impl SettlementPlan {
    /// Returns a unique idempotency step identifier for a payer transfer.
    pub fn payer_step(&self, idx: u32) -> u64 {
        idx as u64
    }

    /// Returns a unique idempotency step identifier for a receiver transfer.
    pub fn receiver_step(&self, idx: u32) -> u64 {
        10_000u64 + (idx as u64)
    }

    /// Retrieves an existing settlement plan or creates a new one if it doesn't exist.
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

            let idempotency_ns = ic_cdk::api::time().into();

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
                idempotency_ns,
                payer_receipts: vec![None; payers_len],
                receiver_receipts: vec![None; receivers_len],
            };

            m.insert(key, plan.clone());
            plan
        })
    }
}
