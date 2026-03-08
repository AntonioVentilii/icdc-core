use candid::CandidType;
use serde::{Deserialize, Serialize};
use shared::types::{AssetId, Price, SeriesId};

use crate::{
    api::admin::params::FundType,
    memory::{DEPOSIT_PLANS, FUND_WITHDRAWAL_PLANS, SETTLEMENT_PLANS, WITHDRAWAL_PLANS},
    types::{
        payment::{PaymentIdempotency, PaymentReceipt},
        user::{DepositId, User, WithdrawalId},
    },
    utils::system::now_ns,
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
    /// The collateral asset being deposited.
    pub asset_id: AssetId,
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
    /// The collateral asset being withdrawn.
    pub asset_id: AssetId,
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
    pub settlement_price: Price,
    /// The oracle source identifier for authorization.
    pub oracle_source: String,
    /// The protocol fee applied to the settlement (in USD units).
    pub fee: u128,
    /// The insurance fee collected for this settlement session (in USD units).
    pub insurance_fee: u128,
    /// A list of positions involved in the settlement with their accounting data.
    pub positions: Vec<SettlementPosition>,
}

/// Snapshot of a single user's position and its settlement accounting.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct SettlementPosition {
    pub user: User,
    pub net_qty: i128,
    pub reserved_margin_usd: u128,
    pub cashflow_usd: i128,
}

/// A plan for processing a collateral deposit in the background.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct DepositPlan {
    /// Unique identifier for the deposit.
    pub deposit_id: DepositId,
    /// The user making the deposit.
    pub user: User,
    /// The collateral asset being deposited.
    pub asset_id: AssetId,
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
                asset_id,
                amount,
            } = params;

            let key = (user, deposit_id.clone());

            if let Some(existing) = m.get(&key) {
                return existing.clone();
            }

            let idempotency_ns = now_ns().into();

            let plan = DepositPlan {
                deposit_id,
                user,
                asset_id,
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
    /// The collateral asset being withdrawn.
    pub asset_id: AssetId,
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
                asset_id,
                amount,
                to_account,
            } = params;

            let key = (user, withdrawal_id.clone());

            if let Some(existing) = m.get(&key) {
                return existing.clone();
            }

            let idempotency_ns = now_ns().into();

            let plan = WithdrawalPlan {
                withdrawal_id,
                user,
                asset_id,
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
    pub settlement_price: Price,
    /// The oracle source identifier for authorization.
    pub oracle_source: String,
    /// The protocol fee (in USD units).
    pub fee_usd: u128,
    /// The insurance fee (in USD units).
    pub insurance_fee_usd: u128,
    /// Detailed position snapshots and accounting updates.
    pub positions: Vec<SettlementPosition>,
    /// Tracks progress through accounting updates.
    pub accounting_cursor: usize,
    /// Whether all accounting updates have been applied to account states.
    pub accounting_applied: bool,
    /// Current execution status of the plan.
    pub status: PlanStatus,
    /// Base idempotency key in nanoseconds.
    pub idempotency_ns: PaymentIdempotency,
}

impl SettlementPlan {
    /// Retrieves an existing settlement plan or creates a new one if it doesn't exist.
    pub fn get_or_create(params: SettlementPlanParams) -> Self {
        SETTLEMENT_PLANS.with(|m| {
            let mut m = m.borrow_mut();

            let SettlementPlanParams {
                series_id,
                settlement_price,
                oracle_source,
                fee: fee_usd,
                insurance_fee: insurance_fee_usd,
                positions,
            } = params;

            let key = series_id.clone();

            if let Some(existing) = m.get(&key) {
                return existing.clone();
            }

            let idempotency_ns = now_ns().into();

            let plan = SettlementPlan {
                series_id,
                settlement_price,
                oracle_source,
                fee_usd,
                insurance_fee_usd,
                positions,
                accounting_cursor: 0,
                accounting_applied: false,
                status: PlanStatus::Planned,
                idempotency_ns,
            };

            m.insert(key, plan.clone());
            plan
        })
    }
}

/// Input parameters for creating a [`FundWithdrawalPlan`].
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct FundWithdrawalPlanParams {
    /// Controller-provided unique identifier for this withdrawal request.
    pub request_id: String,
    /// Whether to withdraw from Insurance or Treasury.
    pub fund_type: FundType,
    /// The specific asset to withdraw.
    pub asset_id: AssetId,
    /// The amount to withdraw.
    pub amount: u128,
    /// The destination principal.
    pub to: candid::Principal,
}

/// A plan for processing an admin fund withdrawal in the background.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct FundWithdrawalPlan {
    pub request_id: String,
    pub fund_type: FundType,
    pub asset_id: AssetId,
    pub amount: u128,
    pub to: candid::Principal,
    pub status: PlanStatus,
    pub idempotency_ns: PaymentIdempotency,
    pub receipt: Option<PaymentReceipt>,
}

impl FundWithdrawalPlan {
    /// Retrieves an existing fund withdrawal plan or creates a new one.
    pub fn get_or_create(params: FundWithdrawalPlanParams) -> Self {
        FUND_WITHDRAWAL_PLANS.with(|m| {
            let mut m = m.borrow_mut();

            if let Some(existing) = m.get(&params.request_id) {
                return existing.clone();
            }

            let idempotency_ns = now_ns().into();

            let plan = FundWithdrawalPlan {
                request_id: params.request_id.clone(),
                fund_type: params.fund_type,
                asset_id: params.asset_id,
                amount: params.amount,
                to: params.to,
                status: PlanStatus::Planned,
                idempotency_ns,
                receipt: None,
            };

            m.insert(params.request_id, plan.clone());
            plan
        })
    }
}
