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
    /// The input provided for the request is invalid.
    InvalidInput(String),
}
