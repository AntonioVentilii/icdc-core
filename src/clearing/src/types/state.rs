use std::collections::BTreeMap;

use candid::{CandidType, Principal};
use serde::Deserialize;
use shared::{
    constants::{DEFAULT_INSURANCE_FEE_RATIO, DEFAULT_PROTOCOL_FEE_RATIO},
    types::{Price, Series, SeriesId},
};

use crate::types::{
    event::Event,
    margin::{AccountState, Position},
    plans::{DepositPlan, FundWithdrawalPlan, SettlementPlan, WithdrawalPlan},
    trade::{LimitOrder, OrderId, TradeId, TransferId},
    user::{DepositKey, User, WithdrawalKey},
};

/// A cryptographically signed proof of an open position, used for cross-canister transfers.
#[derive(CandidType, Deserialize, Clone, Debug)]
pub struct PositionProof {
    /// Unique identifier for the transfer operation.
    pub transfer_id: TransferId,
    /// The user whose position is being proven.
    pub user: User,
    /// The unique identifier of the derivative series.
    pub series_id: SeriesId,
    /// The quantity of the position.
    pub qty: i128,
    /// The principal of the clearing canister that issued the proof.
    pub clearing_id: Principal,
    /// The cryptographic signature of the proof data.
    pub signature: Vec<u8>,
    /// Optional valuation price for the position.
    pub valuation_price: Option<Price>,
}

/// Global configuration for the Clearing canister.
#[derive(CandidType, Deserialize, Clone, Debug)]
pub struct Config {
    /// The global insurance fund fee ratio in basis points (1 bp = 0.01%).
    pub insurance_fund_fee_ratio: u16,
    /// The global protocol fee ratio (Treasury) in basis points.
    pub protocol_fee_ratio: u16,
    /// The principal of the EVM RPC canister.
    pub evm_rpc: Principal,
    /// The principal of the Chain Fusion Signer canister.
    pub signer_canister: Principal,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            insurance_fund_fee_ratio: DEFAULT_INSURANCE_FEE_RATIO,
            protocol_fee_ratio: DEFAULT_PROTOCOL_FEE_RATIO,
            evm_rpc: Principal::anonymous(),
            signer_canister: Principal::anonymous(),
        }
    }
}

/// Represents the complete state of the clearing canister for persistence.
#[derive(CandidType, Deserialize, Clone, Debug)]
pub struct StableState {
    /// The global configuration.
    pub config: Config,
    /// All active positions in the system.
    pub positions: Vec<Position>,
    /// Mapping of users to their account states.
    pub accounts: BTreeMap<User, AccountState>,
    /// Cached information about registered series.
    pub series: BTreeMap<SeriesId, Series>,
    /// A log of significant system events.
    pub events: Vec<Event>,
    /// Counter for generating unique identifiers.
    pub next_id: u64,
    /// The principal of the Series Registry canister.
    pub registry: Principal,
    /// Active plans for collateral deposits.
    pub deposit_plans: BTreeMap<DepositKey, DepositPlan>,
    /// Active plans for collateral withdrawals.
    pub withdrawal_plans: BTreeMap<WithdrawalKey, WithdrawalPlan>,
    /// Active plans for admin fund withdrawals.
    pub fund_withdrawal_plans: BTreeMap<String, FundWithdrawalPlan>,
    /// Tracked execution IDs to prevent double-processing of trades.
    pub executed_trades: BTreeMap<TradeId, u64>,
    /// Positions currently frozen for transfer.
    pub frozen_transfers: BTreeMap<TransferId, PositionProof>,
    /// Status of position transfers (accepted or not).
    pub accepted_transfers: BTreeMap<TransferId, bool>,
    /// Active plans for series settlement.
    pub settlement_plans: BTreeMap<SeriesId, SettlementPlan>,
    /// Active limit orders.
    pub limit_orders: BTreeMap<OrderId, LimitOrder>,
    /// Accumulated fees in the insurance fund, per asset.
    pub insurance_fund: BTreeMap<shared::types::AssetId, u128>,
    /// Accumulated fees in the treasury (main fund), per asset.
    pub treasury: BTreeMap<shared::types::AssetId, u128>,
    /// Configuration for supported collateral assets.
    pub collateral_assets: BTreeMap<shared::types::AssetId, shared::types::CollateralAssetConfig>,
    /// Cached EVM addresses derived for principals.
    pub evm_addresses: BTreeMap<Principal, String>,
}
