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
    async fn get_account_data(&self, address: &str) -> PointsBotResult<AccountData>;

    /// Get markets
    async fn get_markets(&self) -> PointsBotResult<Vec<MarketInfo>>;
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
pub struct AccountData {
    pub account_value: Decimal,
    pub total_margin_used: Decimal,
    pub total_ntl_pos: Decimal,
    pub total_raw_usd: Decimal,
    pub withdrawable: Decimal,
    pub available_balance: Decimal,
    pub positions: Vec<Position>,
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
    pub size: Decimal,
    pub side: PositionSide,
    pub entry_price: Decimal,
    pub unrealized_pnl: Decimal,
    pub margin_used: Decimal,
    pub liquidation_price: Option<Decimal>,
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
    pub funding_rate: Decimal,
    pub funding_rate_8h: Decimal,
    pub mark_price: Decimal,
    pub index_price: Option<Decimal>,
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
    pub volume_24h: Decimal,
    pub price_change_24h: Decimal,
    pub high_24h: Decimal,
    pub low_24h: Decimal,
    pub last_price: Decimal,
    pub mark_price: Decimal,
    pub funding_rate: Decimal,
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