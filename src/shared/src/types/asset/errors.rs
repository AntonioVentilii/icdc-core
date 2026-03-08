use candid::{CandidType, Deserialize};
use serde::Serialize;

/// Errors related to Asset interactions.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum AssetError {
    /// Failed to transfer tokens/assets.
    TransferError(String),
    /// The asset variant is incompatible with the current handler.
    InvalidAssetForHandler,
    /// The specified asset is not supported.
    UnsupportedAsset,
    /// A cross-canister or RPC call failed.
    CallError {
        /// The name of the method that failed.
        method: String,
        /// The rejection code from the IC or error code from RPC.
        code: i32,
        /// The rejection/error message.
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
