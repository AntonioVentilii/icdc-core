use std::collections::BTreeMap;

use candid::{CandidType, Principal};
use serde::Deserialize;
use shared::types::{Series, SeriesId};

use crate::types::{
    event::Event,
    margin::{MarginAccount, Position},
    plans::{DepositPlan, SettlementPlan, WithdrawalPlan},
    trade::{TradeId, TransferId},
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
}

/// Represents the complete state of the clearing canister for persistence.
#[derive(CandidType, Deserialize, Clone, Debug)]
pub struct StableState {
    /// All active positions in the system.
    pub positions: Vec<Position>,
    /// Mapping of users to their margin accounts.
    pub accounts: BTreeMap<User, MarginAccount>,
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
    /// Tracked execution IDs to prevent double-processing of trades.
    pub executed_trades: BTreeMap<TradeId, u64>,
    /// Positions currently frozen for transfer.
    pub frozen_transfers: BTreeMap<TransferId, PositionProof>,
    /// Status of position transfers (accepted or not).
    pub accepted_transfers: BTreeMap<TransferId, bool>,
    /// Active plans for series settlement.
    pub settlement_plans: BTreeMap<SeriesId, SettlementPlan>,
}
