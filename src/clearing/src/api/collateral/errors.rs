use candid::CandidType;
use serde::{Deserialize, Serialize};
use shared::types::asset::errors::AssetError;

/// Errors occurring during collateral deposit.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum DepositCollateralError {
    /// An error occurred while interacting with the asset.
    Asset(AssetError),
    /// Overflow during collateral calculation.
    MathOverflow,
}

/// Errors occurring during collateral withdrawal.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum WithdrawCollateralError {
    /// An error occurred while interacting with the asset.
    Asset(AssetError),
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
