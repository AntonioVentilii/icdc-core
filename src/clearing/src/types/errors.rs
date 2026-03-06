use candid::CandidType;
use serde::{Deserialize, Serialize};

/// Generic errors that can occur across multiple modules.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum CommonError {
    /// The caller does not have permission to perform this action.
    Unauthorized,
    /// The registry canister principal has not been configured.
    RegistryNotSet,
    /// An unexpected internal error occurred.
    Internal(String),
    /// A mathematical calculation resulted in an overflow or underflow.
    MathOverflow,
}

/// Errors related to Ledger canister interactions.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum LedgerError {
    /// Failed to transfer tokens.
    TransferError(String),
    /// The specified ledger is not supported.
    UnsupportedLedger,
    /// A cross-canister call failed.
    CallError {
        /// The name of the method that failed.
        method: String,
        /// The rejection code from the IC.
        code: i32,
        /// The rejection message from the IC.
        message: String,
    },
    /// The account has insufficient funds for the operation.
    InsufficientBalance {
        /// The current balance of the account.
        balance: u128,
        /// The amount required for the operation.
        required: u128,
    },
    /// Overflow during internal balance calculations.
    MathOverflow,
}
