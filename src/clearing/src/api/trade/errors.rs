use candid::CandidType;
use serde::{Deserialize, Serialize};
use shared::types::SeriesId;

use crate::types::{errors::CommonError, trade::OrderId, user::User};

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
