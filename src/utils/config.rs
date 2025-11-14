use crate::{
    create_operator_hyperliquid,
    fetchers::{Fetcher, FetcherExtended, FetcherHyperliquid, FetcherLighter},
    ExchangeName, Operator, OperatorExtended, OperatorLighter, PointsBotError, PointsBotResult,
};
use ethers::signers::LocalWallet;
use log::info;
use rust_decimal::{prelude::FromPrimitive, Decimal};
use serde::{Deserialize, Serialize};
use std::{env, str::FromStr};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum BotMode {
    Production,
    Testing,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotEnvConfig {
    pub mode: BotMode,
    pub config_file_path: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct BotJsonConfig {
    pub id: String,
    pub wallet_address: Option<String>,
    pub private_key: Option<String>,
    pub exchange_a: Option<ExchangeName>,
    pub exchange_b: Option<ExchangeName>,
    pub extended: Option<ExtendedConfig>,
    pub lighter: Option<LighterConfig>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct ExtendedConfig {
    pub api_key: String,
    pub stark_private_key: String,
    pub stark_public_key: String,
    pub vault_id: u64,
    pub entry_offset: Decimal,
    pub exit_offset: Decimal,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct LighterConfig {
    pub api_key: String,
    pub api_key_index: u32,
    pub account_index: u32,
    pub entry_offset: Decimal,
    pub exit_offset: Decimal,
}

pub struct BotConfig {
    pub id: String,
    pub wallet: LocalWallet,
    pub private_key: String,
    pub fetcher_a: Box<dyn Fetcher>,
    pub fetcher_b: Box<dyn Fetcher>,
    pub operator_a: Box<dyn Operator>,
    pub operator_b: Box<dyn Operator>,
    pub extended: Option<ExtendedConfig>,
    pub lighter: Option<LighterConfig>,
}

impl BotEnvConfig {
    pub fn load_env() -> PointsBotResult<Self> {
        dotenv::dotenv().ok();

        let mode = match env::var("BOT_MODE").unwrap_or_default().to_lowercase().as_str() {
            "production" => BotMode::Production,
            _ => BotMode::Testing,
        };

        Ok(BotEnvConfig {
            mode,
            config_file_path: env::var("CONFIG_FILE_PATH").ok(),
        })
    }
}

impl BotJsonConfig {
    pub fn load_config_file(env_config: &BotEnvConfig) -> PointsBotResult<Vec<BotJsonConfig>> {
        let config_str = std::fs::read_to_string(
            env_config.config_file_path.as_ref().expect("Missing config_file_path"),
        )
        .map_err(|e| PointsBotError::Unknown {
            msg: format!("Failed to read config file: {}", e),
            source: None,
        })?;
        let configs = serde_json::from_str(&config_str)?;
        Ok(configs)
    }

    pub async fn process_config(config_json: &BotJsonConfig) -> BotConfig {
        info!("Processing bot config: {:?}", config_json.id);

        let private_key = config_json
            .private_key
            .as_deref()
            .expect("Missing private_key in config");
        let wallet = LocalWallet::from_str(private_key).expect("Failed to create wallet from private key");

        let (fetcher_a, operator_a) =
            Self::create_fetcher_and_operator(config_json.exchange_a.unwrap(), wallet.clone(), config_json).await;

        let (fetcher_b, operator_b) =
            Self::create_fetcher_and_operator(config_json.exchange_b.unwrap(), wallet.clone(), config_json).await;

        BotConfig {
            id: config_json.id.clone(),
            private_key: config_json.private_key.clone().unwrap(),
            wallet: wallet.clone(),
            fetcher_a,
            operator_a,
            fetcher_b,
            operator_b,
            extended: config_json.extended.clone(),
            lighter: config_json.lighter.clone(),
        }
    }

    pub async fn create_fetcher_and_operator(
        exchange: ExchangeName,
        wallet: LocalWallet,
        config_json: &BotJsonConfig,
    ) -> (Box<dyn Fetcher>, Box<dyn Operator>) {
        match exchange {
            ExchangeName::Extended => {
                config_json.extended.as_ref().expect("Missing extended config");
                (
                    Box::new(FetcherExtended::new(&config_json)),
                    Box::new(OperatorExtended::new(&config_json).await),
                )
            }
            ExchangeName::Lighter => {
                config_json.lighter.as_ref().expect("Missing lighter config");
                (
                    Box::new(FetcherLighter::new(&config_json)),
                    Box::new(OperatorLighter::new(&config_json).await),
                )
            }
            ExchangeName::Hyperliquid => (
                Box::new(FetcherHyperliquid::new(&config_json)),
                create_operator_hyperliquid(wallet.clone()).await,
            ),
        }
    }

    pub fn get_taker_fee(exchange: ExchangeName) -> Decimal {
        match exchange {
            ExchangeName::Extended => Decimal::from_f64(0.000225).unwrap(), // 0.0225% fee
            ExchangeName::Lighter => Decimal::from_f64(0.0002).unwrap(),    // 0.02% fee
            ExchangeName::Hyperliquid => Decimal::from_f64(0.00045).unwrap(), // 0.045% fee
        }
    }

    pub fn get_entry_offset(&self, exchange: ExchangeName) -> Decimal {
        match exchange {
            ExchangeName::Extended => self.extended.as_ref().map_or(Decimal::ZERO, |c| c.entry_offset),
            ExchangeName::Lighter => self.lighter.as_ref().map_or(Decimal::ZERO, |c| c.entry_offset),
            ExchangeName::Hyperliquid => Decimal::ZERO,
        }
    }

    pub fn get_exit_offset(&self, exchange: ExchangeName) -> Decimal {
        match exchange {
            ExchangeName::Extended => self.extended.as_ref().map_or(Decimal::ZERO, |c| c.exit_offset),
            ExchangeName::Lighter => self.lighter.as_ref().map_or(Decimal::ZERO, |c| c.exit_offset),
            ExchangeName::Hyperliquid => Decimal::ZERO,
        }
    }
}
