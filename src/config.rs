use std::env;
use serde::{Deserialize, Serialize};
use crate::PointsBotResult;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotConfig {
    pub trading_mode: String,
    pub wallet_address: String,
    pub hyperliquid_private_key: Option<String>,
    pub extended_api_key: Option<String>,
    pub extended_stark_key: Option<String>,
    pub extended_stark_public_key: Option<String>,
    pub extended_vault_key: Option<String>,
}

impl BotConfig {
    pub fn from_env() -> PointsBotResult<Self> {
        dotenv::dotenv().ok();
        
        let trading_mode = env::var("TRADING_ENV")
            .unwrap_or_else(|_| "testing".to_string());
        
        let wallet_address = env::var("WALLET_ADDRESS")
            .unwrap_or_else(|_| panic!("WALLET_ADDRESS is not set"));
        
        Ok(BotConfig {
            trading_mode,
            wallet_address,
            hyperliquid_private_key: env::var("HYPERLIQUID_PRIVATE_KEY").ok(),
            extended_api_key: env::var("EXTENDED_API_KEY").ok(),
            extended_stark_key: env::var("EXTENDED_STARK_KEY").ok(),
            extended_stark_public_key: env::var("EXTENDED_STARK_PUBLIC_KEY").ok(),
            extended_vault_key: env::var("EXTENDED_VAULT_KEY").ok(),
        })
    }
} 