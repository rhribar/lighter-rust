use crate::PointsBotResult;
use serde::{Deserialize, Serialize};
use std::env;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum BotMode {
    Production,
    Testing,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotConfig {
    pub mode: BotMode,
    pub wallet_address: String,
    pub private_key: Option<String>,
    pub extended: Option<ExtendedConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtendedConfig {
    pub api_key: Option<String>,
    pub stark_private_key: Option<String>,
    pub stark_public_key: Option<String>,
    pub vault_key: Option<String>,
}

impl BotConfig {
    pub fn load_env() -> PointsBotResult<Self> {
        dotenv::dotenv().ok();

        let mode = match env::var("BOT_MODE").unwrap_or_default().to_lowercase().as_str() {
            "production" => BotMode::Production,
            _ => BotMode::Testing,
        };

        Ok(BotConfig {
            mode,
            wallet_address: env::var("WALLET_ADDRESS").unwrap_or_else(|_| panic!("WALLET_ADDRESS is not set")),
            private_key: env::var("PRIVATE_KEY").ok(),
            extended: Some(ExtendedConfig {
                api_key: env::var("EXTENDED_API_KEY").ok(),
                stark_private_key: env::var("EXTENDED_STARK_PRIVATE_KEY").ok(),
                stark_public_key: env::var("EXTENDED_STARK_PUBLIC_KEY").ok(),
                vault_key: env::var("EXTENDED_VAULT_KEY").ok(),
            }),
        })
    }
}
