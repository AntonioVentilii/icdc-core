use candid::{CandidType, Deserialize, Nat};
use serde::Serialize;
use shared::types::{Asset, SeriesId};

use crate::types::{
    trade::{OrderId, Side, TradeId, TransferId},
    user::{DepositId, User, WithdrawalId},
};
