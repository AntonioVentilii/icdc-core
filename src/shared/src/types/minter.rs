use candid::{CandidType, Deserialize, Nat, Principal};
use icrc_ledger_types::icrc1::transfer::{BlockIndex, TransferError};
use serde::Serialize;

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct Config {
    pub ledger_canister: Principal,
    pub authorized_callers: Vec<Principal>,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum ConfigResult {
    Ok(Config),
    Err(String),
}

impl From<Result<Config, String>> for ConfigResult {
    fn from(value: Result<Config, String>) -> Self {
        match value {
            Ok(v) => ConfigResult::Ok(v),
            Err(e) => ConfigResult::Err(e),
        }
    }
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct MintParams {
    pub to: Principal,
    pub amount: Nat,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum MintResult {
    Ok(BlockIndex),
    Err(String),
}

impl From<Result<BlockIndex, TransferError>> for MintResult {
    fn from(value: Result<BlockIndex, TransferError>) -> Self {
        match value {
            Ok(v) => MintResult::Ok(v),
            Err(e) => MintResult::Err(format!("Transfer error: {e:?}")),
        }
    }
}
