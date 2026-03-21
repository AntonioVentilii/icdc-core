use candid::{CandidType, Nat, Principal};
use serde::{Deserialize, Serialize};
use shared::types::{AssetId, BalanceDomain, OutcomeId, SeriesId, SettlementInput};

use crate::{
    api::admin::params::FundType,
    memory::{
        DEPOSIT_PLANS, FUND_WITHDRAWAL_PLANS, MIGRATION_PLANS, SETTLEMENT_PLANS, WITHDRAWAL_PLANS,
    },
    types::{
        payment::{PaymentIdempotency, PaymentReceipt},
        trade::OrderId,
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
    pub amount: Nat,
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
    pub amount: Nat,
    /// The destination principal and optional subaccount.
    pub to_account: (Principal, Option<[u8; 32]>),
}

/// Input parameters for creating a [`SettlementPlan`].
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct SettlementPlanParams {
    /// The unique identifier of the derivative series being settled.
    pub series_id: SeriesId,
    /// The final settlement data from the oracle.
    pub settlement: SettlementInput,
    /// The oracle source identifier for authorization.
    pub oracle_source: String,
    /// The protocol fee applied to the settlement (in USD units).
    pub fee: u128,
    /// The insurance fee collected for this settlement session (in USD units).
    pub insurance_fee: u128,
    /// A list of positions involved in the settlement with their accounting data.
    pub positions: Vec<SettlementPosition>,
    /// The domain of the series being settled.
    pub balance_domain: BalanceDomain,
}

/// Snapshot of a single user's position and its settlement accounting.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct SettlementPosition {
    pub user: User,
    pub outcome_id: Option<OutcomeId>,
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
    pub amount: Nat,
    /// Current execution status of the plan.
    pub status: PlanStatus,
    /// Idempotency key in nanoseconds for ledger transfers.
    pub idempotency_ns: PaymentIdempotency,
    /// Proof of successful transfer, if completed.
    pub receipt: Option<PaymentReceipt>,
}
impl DepositPlan {
    /// Retrieves an existing deposit plan or creates a new one if it doesn't exist.
    #[must_use]
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
    pub amount: Nat,
    /// The destination principal and optional subaccount.
    pub to_account: (Principal, Option<[u8; 32]>),
    /// Current execution status of the plan.
    pub status: PlanStatus,
    /// Idempotency key in nanoseconds for ledger transfers.
    pub idempotency_ns: PaymentIdempotency,
    /// Proof of successful transfer, if completed.
    pub receipt: Option<PaymentReceipt>,
    /// The amount successfully reserved for withdrawal (token units).
    pub reserved_amount: Option<u128>,
    /// Sum of internal cash (USD) reserved for withdrawal (internal units).
    pub reserved_cash_usd: Option<i128>,
}
impl WithdrawalPlan {
    /// Retrieves an existing withdrawal plan or creates a new one if it doesn't exist.
    #[must_use]
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
                reserved_cash_usd: None,
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
    /// The final settlement data.
    pub settlement: SettlementInput,
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
    /// The domain of the series being settled.
    pub balance_domain: BalanceDomain,
}

impl SettlementPlan {
    /// Retrieves an existing settlement plan or creates a new one if it doesn't exist.
    #[must_use]
    pub fn get_or_create(params: SettlementPlanParams) -> Self {
        SETTLEMENT_PLANS.with(|m| {
            let mut m = m.borrow_mut();

            let SettlementPlanParams {
                series_id,
                settlement,
                oracle_source,
                fee: fee_usd,
                insurance_fee: insurance_fee_usd,
                positions,
                balance_domain,
            } = params;

            let key = series_id.clone();

            if let Some(existing) = m.get(&key) {
                return existing.clone();
            }

            let idempotency_ns = now_ns().into();

            let plan = SettlementPlan {
                series_id,
                settlement,
                oracle_source,
                fee_usd,
                insurance_fee_usd,
                positions,
                accounting_cursor: 0,
                accounting_applied: false,
                status: PlanStatus::Planned,
                idempotency_ns,
                balance_domain,
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
    pub to: Principal,
}

/// A plan for processing an admin fund withdrawal in the background.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct FundWithdrawalPlan {
    pub request_id: String,
    pub fund_type: FundType,
    pub asset_id: AssetId,
    pub amount: u128,
    pub to: Principal,
    pub status: PlanStatus,
    pub idempotency_ns: PaymentIdempotency,
    pub receipt: Option<PaymentReceipt>,
}

impl FundWithdrawalPlan {
    /// Retrieves an existing fund withdrawal plan or creates a new one.
    #[must_use]
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

/// Unique key for a migration plan.
pub type MigrationKey = (User, String);

/// Input parameters for creating a [`MigrationPlan`].
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct MigrationPlanParams {
    /// Caller-provided unique identifier for idempotency.
    pub migration_id: String,
    /// The user whose state is being migrated.
    pub user: User,
    /// The source domain.
    pub from_domain: BalanceDomain,
    /// The target domain.
    pub to_domain: BalanceDomain,
}

/// Snapshot of a position being migrated between domains.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct MigratedPosition {
    pub series_id: SeriesId,
    pub outcome_id: Option<OutcomeId>,
    pub net_qty: i128,
    pub reserved_margin_usd: u128,
}

/// Snapshot of an order being migrated between domains.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct MigratedOrder {
    pub order_id: OrderId,
    pub blocked_margin_usd: u128,
}

/// A plan for migrating a user's entire state from one domain to another.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct MigrationPlan {
    pub migration_id: String,
    pub user: User,
    pub from_domain: BalanceDomain,
    pub to_domain: BalanceDomain,
    pub status: PlanStatus,
    pub idempotency_ns: PaymentIdempotency,
    /// Snapshot of migrated positions for auditability.
    pub positions: Vec<MigratedPosition>,
    /// Snapshot of migrated orders for auditability.
    pub orders: Vec<MigratedOrder>,
    /// Snapshot of migrated balances (`asset_id` -> amount).
    pub balances: Vec<(AssetId, u128)>,
    /// Migrated cash balance (USD).
    pub cash_balance_usd: i128,
    /// Migrated reserved margin (USD).
    pub reserved_margin_usd: u128,
}

impl MigrationPlan {
    /// Retrieves an existing migration plan or creates a new one if it doesn't exist.
    #[must_use]
    pub fn get_or_create(params: MigrationPlanParams) -> Self {
        MIGRATION_PLANS.with(|m| {
            let mut m = m.borrow_mut();

            let key: MigrationKey = (params.user, params.migration_id.clone());

            if let Some(existing) = m.get(&key) {
                return existing.clone();
            }

            let idempotency_ns = now_ns().into();

            let plan = MigrationPlan {
                migration_id: params.migration_id,
                user: params.user,
                from_domain: params.from_domain,
                to_domain: params.to_domain,
                status: PlanStatus::Planned,
                idempotency_ns,
                positions: Vec::new(),
                orders: Vec::new(),
                balances: Vec::new(),
                cash_balance_usd: 0,
                reserved_margin_usd: 0,
            };

            m.insert(key, plan.clone());
            plan
        })
    }
}

/// Public settlement progress for a derivative series.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct SettlementStatusView {
    pub series_id: SeriesId,
    pub settlement: SettlementInput,
    pub oracle_source: String,
    pub fee_usd: u128,
    pub insurance_fee_usd: u128,
    pub accounting_cursor: usize,
    pub accounting_applied: bool,
    pub status: PlanStatus,
    pub total_positions: usize,
    pub balance_domain: BalanceDomain,
}

impl From<&SettlementPlan> for SettlementStatusView {
    fn from(plan: &SettlementPlan) -> Self {
        Self {
            series_id: plan.series_id.clone(),
            settlement: plan.settlement.clone(),
            oracle_source: plan.oracle_source.clone(),
            fee_usd: plan.fee_usd,
            insurance_fee_usd: plan.insurance_fee_usd,
            accounting_cursor: plan.accounting_cursor,
            accounting_applied: plan.accounting_applied,
            status: plan.status.clone(),
            total_positions: plan.positions.len(),
            balance_domain: plan.balance_domain,
        }
    }
}
