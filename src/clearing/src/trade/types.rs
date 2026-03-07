use shared::types::{Price, SeriesId};

use crate::types::{trade::TradeId, user::User};

pub(crate) struct ExecuteTradeParams {
    pub trade_id: TradeId,
    pub series_id: SeriesId,
    pub buyer: User,
    pub seller: User,
    pub qty: i128,
    pub price: Price,
    pub buyer_unblock_amount: Option<u128>,
    pub seller_unblock_amount: Option<u128>,
}
