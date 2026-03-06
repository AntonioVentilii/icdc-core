use candid::{CandidType, Deserialize};
use serde::Serialize;

use crate::types::{
    errors::{
        BlockingError, DepositCollateralError, MarginAccountError, SettlementError, TradeError,
        WithdrawCollateralError,
    },
    margin::MarginAccount,
};


