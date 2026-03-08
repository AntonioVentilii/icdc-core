use crate::types::asset::{Chain, ErcToken, NativeEvmAsset};

pub fn native_evm_asset(chain: Chain) -> NativeEvmAsset {
    match chain {
        Chain::Ethereum | Chain::Base | Chain::Bsc | Chain::Polygon => NativeEvmAsset {
            chain_id: chain.id(),
            decimals: 18,
        },
    }
}

pub fn usdc_token(chain: Chain) -> ErcToken {
    match chain {
        Chain::Ethereum => ErcToken {
            token_address: "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".to_string(),
            chain_id: chain.id(),
            decimals: 6,
        },
        Chain::Base => ErcToken {
            token_address: "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913".to_string(),
            chain_id: chain.id(),
            decimals: 6,
        },
        Chain::Bsc => ErcToken {
            token_address: "0x8ac76a51cc950d9822d68b83fe1ad97b32cd580d".to_string(),
            chain_id: chain.id(),
            decimals: 18,
        },
        Chain::Polygon => ErcToken {
            token_address: "0x3c499c542cEF5E3811e1192ce70d8cC03d5c3359".to_string(),
            chain_id: chain.id(),
            decimals: 6,
        },
    }
}

pub fn usdt_token(chain: Chain) -> ErcToken {
    match chain {
        Chain::Ethereum => ErcToken {
            token_address: "0xdAC17F958D2ee523a2206206994597C13D831ec7".to_string(),
            chain_id: chain.id(),
            decimals: 6,
        },
        _ => panic!("Unsupported chain for USDT"),
    }
}
