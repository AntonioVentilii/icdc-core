use candid::CandidType;
use serde::{Deserialize, Serialize};
use shared::types::SeriesId;

use crate::types::{trade::OrderId, user::User};

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

/// Errors occurring during collateral deposit.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum DepositCollateralError {
    /// An error occurred while interacting with the ledger.
    Ledger(LedgerError),
    /// Overflow during collateral calculation.
    MathOverflow,
}

/// Errors occurring during collateral withdrawal.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum WithdrawCollateralError {
    /// An error occurred while interacting with the ledger.
    Ledger(LedgerError),
    /// The user does not have enough excess margin to withdraw the requested amount.
    InsufficientExcessMargin {
        /// Current excess margin available for withdrawal.
        available: u128,
        /// The amount requested to be withdrawn.
        requested: u128,
    },
    /// Overflow during collateral calculation.
    MathOverflow,
}

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

/// Errors occurring during trade submission or matching.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum TradeError {
    /// A common error occurred.
    Common(CommonError),
    /// The specified series was not found in the registry.
    SeriesNotFound(SeriesId),
    /// The user has insufficient margin to open or maintain the position.
    InsufficientMargin {
        /// The user whose margin is insufficient.
        user: User,
        /// Current margin balance.
        balance: u128,
        /// Required margin for the trade.
        required: u128,
    },
    /// Failed to communicate with the registry canister.
    RegistryError(String),
    /// The specified order was not found.
    OrderNotFound(OrderId),
    /// The caller is not the creator of the order.
    NotOrderCreator,
}

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

/// Errors occurring during collateral blocking or unblocking.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum BlockingError {
    /// The user does not have enough available balance to block the requested amount.
    InsufficientAvailableBalance {
        /// Current available balance.
        available: u128,
        /// The amount requested to be blocked.
        requested: u128,
    },
    /// The user does not have enough reserved balance to unblock the requested amount.
    InsufficientReservedBalance {
        /// Current reserved balance.
        reserved: u128,
        /// The amount requested to be unblocked.
        requested: u128,
    },
    /// Overflow during calculation.
    MathOverflow,
}
