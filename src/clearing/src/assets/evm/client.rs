use candid::{CandidType, Nat, Principal};
use hex::encode;
use ic_cdk::call::{Call, CallFailed};
use serde::{Deserialize, Serialize};

#[derive(CandidType, Serialize, Deserialize, Debug)]
pub struct EthAddressArgs {
    pub principal: Principal,
}

#[derive(CandidType, Serialize, Deserialize, Debug)]
pub struct SignTransactionArgs {
    pub transaction: Vec<u8>,
    pub principal: Principal,
}

fn format_call_failed(err: CallFailed) -> String {
    match err {
        CallFailed::CallRejected(r) => format!(
            "Call error: {}: {}",
            r.raw_reject_code(),
            r.reject_message()
        ),
        e => format!("Call error: {e}"),
    }
}

pub struct ChainFusionSignerClient {
    pub canister_id: Principal,
}

impl ChainFusionSignerClient {
    pub fn new(canister_id: Principal) -> Self {
        Self { canister_id }
    }

    pub async fn eth_address(&self, principal: Principal) -> Result<String, String> {
        let args = EthAddressArgs { principal };
        let response = Call::bounded_wait(self.canister_id, "eth_address")
            .with_args(&(args,))
            .await
            .map_err(format_call_failed)?;

        let (res,) = response
            .candid_tuple::<(Result<String, String>,)>()
            .map_err(|e| e.to_string())?;
        res
    }

    #[expect(dead_code)]
    pub async fn eth_sign_transaction(
        &self,
        transaction: Vec<u8>,
        principal: Principal,
    ) -> Result<Vec<u8>, String> {
        let args = SignTransactionArgs {
            transaction,
            principal,
        };
        let response = Call::bounded_wait(self.canister_id, "eth_sign_transaction")
            .with_args(&(args,))
            .await
            .map_err(format_call_failed)?;

        let (res,) = response
            .candid_tuple::<(Result<Vec<u8>, String>,)>()
            .map_err(|e| e.to_string())?;
        res
    }
}

#[derive(CandidType, Serialize, Deserialize, Debug)]
pub enum RpcService {
    EthereumMainnet,
    EthereumSepolia,
    BaseMainnet,
    BaseSepolia,
    BscMainnet,
    PolygonMainnet,
}

#[derive(CandidType, Serialize, Deserialize, Debug)]
pub struct RpcConfig {
    pub service: RpcService,
}

#[derive(CandidType, Serialize, Deserialize, Debug)]
pub enum JsonRpcResult<T> {
    Ok(T),
    Err(String),
}

pub struct EvmRpcClient {
    pub canister_id: Principal,
}

#[derive(CandidType, Serialize, Deserialize, Debug, Clone)]
pub struct TransactionRequest {
    pub to: String,
    pub data: Option<String>,
}

impl EvmRpcClient {
    pub fn new(canister_id: Principal) -> Self {
        Self { canister_id }
    }

    pub async fn get_balance(
        &self,
        address: String,
        block: String,
        config: RpcConfig,
    ) -> Result<Nat, String> {
        let response = Call::bounded_wait(self.canister_id, "eth_getBalance")
            .with_args(&(address, block, config))
            .await
            .map_err(format_call_failed)?;

        let (res,) = response
            .candid_tuple::<(JsonRpcResult<Nat>,)>()
            .map_err(|e| e.to_string())?;
        match res {
            JsonRpcResult::Ok(val) => Ok(val),
            JsonRpcResult::Err(e) => Err(e),
        }
    }

    #[expect(dead_code)]
    pub async fn send_raw_transaction(
        &self,
        raw_tx: Vec<u8>,
        config: RpcConfig,
    ) -> Result<String, String> {
        let hex_tx = format!("0x{}", encode(raw_tx));
        let response = Call::bounded_wait(self.canister_id, "eth_sendRawTransaction")
            .with_args(&(hex_tx, config))
            .await
            .map_err(format_call_failed)?;

        let (res,) = response
            .candid_tuple::<(JsonRpcResult<String>,)>()
            .map_err(|e| e.to_string())?;
        match res {
            JsonRpcResult::Ok(val) => Ok(val),
            JsonRpcResult::Err(e) => Err(e),
        }
    }

    pub async fn call(
        &self,
        to: String,
        data: String,
        block: String,
        config: RpcConfig,
    ) -> Result<String, String> {
        let call_args = TransactionRequest {
            to,
            data: Some(data),
        };
        let response = Call::bounded_wait(self.canister_id, "eth_call")
            .with_args(&(call_args, block, config))
            .await
            .map_err(format_call_failed)?;

        let (res,) = response
            .candid_tuple::<(JsonRpcResult<String>,)>()
            .map_err(|e| e.to_string())?;
        match res {
            JsonRpcResult::Ok(val) => Ok(val),
            JsonRpcResult::Err(e) => Err(e),
        }
    }
}
