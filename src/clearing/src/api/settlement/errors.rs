use candid::CandidType;
use serde::{Deserialize, Serialize};
use shared::types::{asset::errors::AssetError, Price};

use crate::types::errors::CommonError;

/// Errors occurring during derivative series settlement.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum SettlementError {
    /// A common error occurred.
    Common(CommonError),
    /// An error occurred while interacting with the asset.
    Asset(AssetError),
    /// Overflow during settlement calculation.
    MathOverflow,
    /// Total net payoffs (post-fee) exceed global system equity (system insolvency).
    SolvencyViolation {
        total_net_payoff: u128,
        total_collateral_usd: u128,
    },
    /// Settlement price inconsistent with already executing plan.
    InconsistentSettlementPrice {
        existing: Box<Price>,
        requested: Box<Price>,
    },
}
