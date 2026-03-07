use candid::CandidType;
use serde::{Deserialize, Serialize};

use crate::types::{
    errors::{CommonError, LedgerError},
    user::User,
};

/// Errors occurring during derivative series settlement.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum SettlementError {
    /// A common error occurred.
    Common(CommonError),
    /// An error occurred while interacting with the ledger.
    Ledger(LedgerError),
    /// The series uses an unsupported settlement asset.
    UnsupportedSettlementAsset,
    /// Overflow during settlement calculation.
    MathOverflow,
    /// Total payoffs exceed locked collateral (system insolvency).
    SolvencyViolation {
        total_payoff: u128,
        total_collateral: u128,
    },
    /// A payer has insufficient internal balance to cover their loss.
    InsufficientInternalBalance {
        user: User,
        balance: u128,
        required: u128,
    },
}
