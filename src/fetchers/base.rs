/// Base fetcher functionality
/// 
/// This module provides common functionality for all exchange fetchers,
/// including the Fetcher trait definition and shared utility functions.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use crate::{PointsBotResult, PointsBotError, ExchangeName, Side};

// Re-export HttpClient so fetchers can access it
pub use crate::http_client::HttpClient;

// ===== FETCHER TRAIT =====

/// Trait that all exchange fetchers must implement
#[async_trait]
pub trait Fetcher: Send + Sync {
    /// Get account balance and summary data
    async fn get_account_data(&self, address: &str) -> PointsBotResult<AccountBalance>;

    /// Get user positions for all trading pairs
    async fn get_user_positions(&self, address: &str) -> PointsBotResult<Vec<Position>>;

    /// Get list of supported trading pairs/tokens
    async fn get_supported_tokens(&self) -> PointsBotResult<Vec<String>>;

    /// Get funding rates for all supported pairs
    async fn get_markets(&self) -> PointsBotResult<Vec<MarketInfo>>;

    /// Get the exchange name
    fn exchange_name(&self) -> ExchangeName;
}

// ===== FETCHER TYPES =====

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarginSummary {
    pub account_value: String,
    pub total_margin_used: String,
    pub total_ntl_pos: String,
    pub total_raw_usd: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountBalance {
    pub account_value: f64,
    pub total_margin_used: f64,
    pub total_ntl_pos: f64,
    pub total_raw_usd: f64,
    pub withdrawable: f64,
    pub available_balance: f64,
    pub positions_count: i32,
    pub exchange: ExchangeName,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PositionSide {
    #[serde(rename = "long")]
    Long,
    #[serde(rename = "short")]
    Short,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub symbol: String,
    pub size: String,
    pub side: PositionSide,
    pub entry_price: f64,
    pub mark_price: f64,
    pub unrealized_pnl: f64,
    pub margin_used: f64,
    pub liquidation_price: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionsData {
    pub exchange: ExchangeName,
    pub address: String,
    pub positions: Vec<Position>,
    pub margin_summary: MarginSummary,
    pub withdrawable: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FundingRateData {
    pub funding_rate: f64,
    pub funding_rate_8h: f64,
    pub mark_price: f64,
    pub index_price: Option<f64>,
    pub next_funding_time: Option<i64>,
    pub exchange: ExchangeName,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FundingRate {
    pub symbol: String,
    pub rate: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FundingRates {
    pub exchange: ExchangeName,
    pub funding_rates: HashMap<String, FundingRateData>,
    pub timestamp: i64,
}

// ===== MARKET DATA TYPES =====

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketInfo {
    pub symbol: String,
    pub base_asset: String,
    pub quote_asset: String,
    pub bid_price: Decimal,
    pub ask_price: Decimal,
    pub leverage: Decimal,
    pub funding_rate: Decimal,
    pub min_order_size: Option<Decimal>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarketStatus {
    #[serde(rename = "active")]
    Active,
    #[serde(rename = "inactive")]
    Inactive,
    #[serde(rename = "delisted")]
    Delisted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketStatistics {
    pub symbol: String,
    pub volume_24h: f64,
    pub price_change_24h: f64,
    pub high_24h: f64,
    pub low_24h: f64,
    pub last_price: f64,
    pub mark_price: f64,
    pub funding_rate: f64,
}

// ===== TOKEN/ASSET TYPES =====

pub type TokenList = Vec<String>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenInfo {
    pub symbol: String,
    pub name: String,
    pub decimals: i32,
    pub is_active: bool,
    pub min_trade_amount: Option<String>,
    pub withdrawal_fee: Option<String>,
}

// ===== UTILITY FUNCTIONS =====

/// Common utility functions for all fetchers
pub mod utils {
    use super::*;
    
    /// Parse a string to Decimal, handling common API response formats
    pub fn parse_decimal_field(value: &str) -> PointsBotResult<Decimal> {
        Decimal::from_str(value.trim())
            .map_err(|_| PointsBotError::Parse(format!("Invalid decimal value: {}", value)))
    }
    
    /// Validate ethereum-style address format
    pub fn validate_address(address: &str) -> PointsBotResult<()> {
        if address.len() != 42 || !address.starts_with("0x") {
            return Err(PointsBotError::InvalidInput(
                "Invalid address format. Expected 42-character hex string starting with 0x".to_string()
            ));
        }
        Ok(())
    }
    
    /// Handle common API error responses
    pub fn handle_api_error(status: u16, body: &str, exchange: ExchangeName) -> PointsBotError {
        match status {
            401 => PointsBotError::Exchange {
                code: "AUTHENTICATION_FAILED".to_string(),
                message: format!("Authentication failed for {}", exchange.as_str()),
            },
            404 => PointsBotError::Exchange {
                code: "NOT_FOUND".to_string(),
                message: "API endpoint not found".to_string(),
            },
            429 => PointsBotError::Exchange {
                code: "RATE_LIMIT_EXCEEDED".to_string(),
                message: "Rate limit exceeded, please try again later".to_string(),
            },
            500..=599 => PointsBotError::Exchange {
                code: "SERVER_ERROR".to_string(),
                message: format!("Server error: {}", body),
            },
            _ => PointsBotError::Exchange {
                code: "API_ERROR".to_string(),
                message: format!("HTTP {}: {}", status, body),
            },
        }
    }
}