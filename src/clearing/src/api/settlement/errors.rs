use candid::CandidType;
use serde::{Deserialize, Serialize};

use crate::types::errors::{CommonError, LedgerError};

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
}
