use candid::CandidType;
use serde::{Deserialize, Serialize};

use crate::types::errors::LedgerError;

/// Errors related to margin account retrieval or state.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum MarginAccountError {
    /// An error occurred while interacting with the ledger.
    Ledger(LedgerError),
    /// No margin account exists for the specified user and asset.
    NoMarginAccountFound,
    /// Overflow during account state calculation.
    MathOverflow,
}
