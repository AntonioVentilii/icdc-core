use candid::{CandidType, Deserialize, Nat};
use serde::Serialize;
use shared::types::{Asset, SeriesId};

use crate::types::{
    trade::{OrderId, Side, TradeId, TransferId},
    user::{DepositId, User, WithdrawalId},
};

/// Input parameters for submitting a limit order.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct SubmitLimitOrderParams {
    /// Unique identifier for the order.
    pub order_id: OrderId,
    /// The derivative series identifier.
    pub series_id: SeriesId,
    /// The side of the order (Buy or Sell).
    pub side: Side,
    /// The quantity of the order.
    pub qty: i128,
    /// The limit price of the order.
    pub price: u64,
}

/// Input parameters for submitting a market order (taking an existing limit order).
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct SubmitMarketOrderParams {
    /// Unique identifier for the trade resulting from this match.
    pub trade_id: TradeId,
    /// The identifier of the limit order to be matched.
    pub matching_order_id: OrderId,
}

/// Input parameters for depositing collateral.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct DepositCollateralParams {
    /// The amount of the asset to deposit.
    pub amount: Nat,
    /// The asset to be deposited.
    pub asset: Asset,
    /// Unique identifier for the deposit operation.
    pub deposit_id: DepositId,
}

/// Input parameters for withdrawing collateral.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct WithdrawCollateralParams {
    /// The amount of the asset to withdraw.
    pub amount: Nat,
    /// The asset to be withdrawn.
    pub asset: Asset,
    /// Unique identifier for the withdrawal operation.
    pub withdrawal_id: WithdrawalId,
}

/// Input parameters for submitting a matched trade from an exchange.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct SubmitMatchedTradeParams {
    /// Unique identifier for the trade provided by the exchange.
    pub trade_id: TradeId,
    /// The derivative series identifier.
    pub series_id: SeriesId,
    /// The user opening or increasing a Long position (buyer).
    pub buyer: User,
    /// The user opening or increasing a Short position (seller).
    pub seller: User,
    /// The quantity of the trade.
    pub qty: i128,
    /// The execution price of the trade.
    pub price: u64,
    /// Optional amount to atomically unblock for the buyer.
    pub buyer_unblock_amount: Option<Nat>,
    /// Optional amount to atomically unblock for the seller.
    pub seller_unblock_amount: Option<Nat>,
}

/// Input parameters for retrieving a margin account.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct GetMarginAccountParams {
    /// Whether to force a recalculation of the margin status.
    pub refresh: Option<bool>,
}

/// Input parameters for retrieving a user's position in a series.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct GetPositionParams {
    /// The derivative series identifier.
    pub series_id: SeriesId,
}

/// Input parameters for initiating a series settlement.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct SettleSeriesParams {
    /// The derivative series identifier.
    pub series_id: SeriesId,
    /// The final settlement price from the oracle.
    pub settlement_price: u64,
}

/// Input parameters for freezing a position to prepare for a cross-canister transfer.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct FreezePositionForTransferParams {
    /// Unique identifier for the transfer operation.
    pub transfer_id: TransferId,
    /// The user whose position is being frozen.
    pub user: User,
    /// The derivative series identifier.
    pub series_id: SeriesId,
}

/// Input parameters for blocking (reserving) collateral.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct BlockCollateralParams {
    /// The amount of the asset to block.
    pub amount: Nat,
    /// The asset to be blocked.
    pub asset: Asset,
}

/// Input parameters for unblocking (releasing) collateral.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct UnblockCollateralParams {
    /// The amount of the asset to unblock.
    pub amount: Nat,
    /// The asset to be unblocked.
    pub asset: Asset,
}

/// Input parameters for cancelling a limit order.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct CancelLimitOrderParams {
    /// Unique identifier for the order to cancel.
    pub order_id: OrderId,
}
