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
    /// The trade would violate the no-arbitrage principle (e.g., sum of outcome prices > 1.0).
    ArbitrageLimitExceeded {
        /// The current sum of best bids across all outcomes.
        sum_usd: u128,
        /// The hard limit (usually 1.0 USD).
        limit_usd: u128,
    },
    /// A user tried to trade with themselves.
    SelfTradingNotAllowed,
    /// The caller is not authorized to trade on this restricted series.
    ///
    /// Returned when a series carries `TradingAccess::Restricted` policies
    /// and the caller is not a member of any of the referenced groups.
    /// The caller should check `is_trading_authorized` on the registry
    /// or contact the group creator to be added as a member.
    NotAuthorizedToTrade,
}
