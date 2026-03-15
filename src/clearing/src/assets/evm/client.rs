use candid::{CandidType, Nat, Principal};
use hex::encode;
use ic_cdk::call;
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

pub struct ChainFusionSignerClient {
    pub canister_id: Principal,
}

impl ChainFusionSignerClient {
    pub fn new(canister_id: Principal) -> Self {
        Self { canister_id }
    }

    pub async fn eth_address(&self, principal: Principal) -> Result<String, String> {
        let args = EthAddressArgs { principal };
        let (res,): (Result<String, String>,) = call(self.canister_id, "eth_address", (args,))
            .await
            .map_err(|(code, msg)| format!("Call error: {}: {}", code as i32, msg))?;
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
        let (res,): (Result<Vec<u8>, String>,) =
            call(self.canister_id, "eth_sign_transaction", (args,))
                .await
                .map_err(|(code, msg)| format!("Call error: {}: {}", code as i32, msg))?;
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
        let (res,): (JsonRpcResult<Nat>,) =
            call(self.canister_id, "eth_getBalance", (address, block, config))
                .await
                .map_err(|(code, msg)| format!("Call error: {}: {}", code as i32, msg))?;
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
        let (res,): (JsonRpcResult<String>,) =
            call(self.canister_id, "eth_sendRawTransaction", (hex_tx, config))
                .await
                .map_err(|(code, msg)| format!("Call error: {}: {}", code as i32, msg))?;
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
        let (res,): (JsonRpcResult<String>,) =
            call(self.canister_id, "eth_call", (call_args, block, config))
                .await
                .map_err(|(code, msg)| format!("Call error: {}: {}", code as i32, msg))?;
        match res {
            JsonRpcResult::Ok(val) => Ok(val),
            JsonRpcResult::Err(e) => Err(e),
        }
    }
}
