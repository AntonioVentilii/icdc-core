use candid::CandidType;
use serde::{Deserialize, Serialize};
use shared::types::{asset::errors::AssetError, SettlementInput};

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
    /// A second `settle_series` call arrived with a `SettlementInput` that does
    /// not match the one already locked in by the in-flight plan.
    ///
    /// Covers both `Price` and `Outcome` settlement types: the variant that's
    /// locked is stored in `existing`, and the one just attempted is in
    /// `requested`. Callers that need to surface a human-readable diff can
    /// pattern-match on both variants without losing information.
    InconsistentSettlement {
        existing: Box<SettlementInput>,
        requested: Box<SettlementInput>,
    },
}
