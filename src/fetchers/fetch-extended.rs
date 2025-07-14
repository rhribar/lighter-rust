/// Extended Exchange Fetcher
/// 
/// Fetches account and trading data from Extended exchange.
/// Supports StarkEx-based trading with proper authentication.

use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashMap;
use std::str::FromStr;
use rust_decimal::Decimal;
use log::{error, info};
use chrono::{DateTime, Utc};

use crate::{
    PointsBotResult, PointsBotError, ExchangeName, current_timestamp
};
use super::base::{HttpClient, Fetcher, AccountBalance, Position, FundingRate, PositionSide};

/// Extended API response wrapper
#[derive(Debug, Deserialize)]
struct ExtendedResponse<T> {
    status: String,
    data: Option<T>,
    error: Option<ExtendedError>,
}

#[derive(Debug, Deserialize)]
struct ExtendedError {
    message: String,
}

/// Extended balance response
#[derive(Debug, Deserialize)]
struct ExtendedBalanceData {
    equity: String,
    balance: String,
    #[serde(rename = "availableForWithdrawal")]
    available_for_withdrawal: String,
    #[serde(rename = "updatedTime")]
    updated_time: i64,
}

#[derive(Debug, Deserialize)]
struct ExtendedPositionData {
    #[serde(rename = "assetName")]
    asset_name: String,
    side: String,
    #[serde(rename = "positionValue")]
    position_value: String,
    notional: String,
    margin: String,
    #[serde(rename = "entryPrice")]
    entry_price: String,
    #[serde(rename = "markPrice")]
    mark_price: String,
    #[serde(rename = "unrealizedPnl")]
    unrealized_pnl: String,
}

#[derive(Debug, Deserialize)]
struct ExtendedMarketData {
    #[serde(rename = "assetName")]
    asset_name: String,
    #[serde(rename = "marketStats")]
    market_stats: ExtendedMarketStats,
}

#[derive(Debug, Deserialize)]
struct ExtendedMarketStats {
    #[serde(rename = "fundingRate")]
    funding_rate: String,
    #[serde(rename = "markPrice")]
    mark_price: String,
}

/// Extended Exchange Fetcher
pub struct ExtendedFetcher {
    client: HttpClient,
    api_key: Option<String>,
}

impl ExtendedFetcher {
    /// Create a new Extended fetcher
    pub fn new() -> Self {
        let client = HttpClient::new(
            "https://api.extended.exchange/api/v1".to_string(),
            Some(1000), // Rate limit: 1000 requests per minute (matching Python)
        );
        
        let api_key = std::env::var("EXTENDED_API_KEY").ok();
        
        Self {
            client,
            api_key,
        }
    }
    
    /// Create an Extended fetcher with specific API key
    pub fn with_api_key(api_key: String) -> Self {
        let client = HttpClient::new(
            "https://api.extended.exchange/api/v1".to_string(),
            Some(1000),
        );
        
        Self {
            client,
            api_key: Some(api_key),
        }
    }
    
    /// Check if authenticated
    fn is_authenticated(&self) -> bool {
        self.api_key.is_some()
    }
    
    /// Get headers for authenticated requests
    fn get_auth_headers(&self) -> PointsBotResult<HashMap<String, String>> {
        let api_key = self.api_key.as_ref()
            .ok_or_else(|| PointsBotError::Auth("No API key provided".to_string()))?;
        
        let mut headers = HashMap::new();
        headers.insert("X-Api-Key".to_string(), api_key.clone());
        headers.insert("User-Agent".to_string(), "points-bot-rs/1.0".to_string());
        
        Ok(headers)
    }
}

#[async_trait]
impl Fetcher for ExtendedFetcher {
    async fn get_account_data(&self, address: &str) -> PointsBotResult<AccountBalance> {
        if !self.is_authenticated() {
            return Err(PointsBotError::Auth("Extended API requires authentication".to_string()));
        }
        
        let headers = self.get_auth_headers()?;
        
        // Get balance data
        let balance_response = self.client.get("/user/balance", Some(headers.clone())).await?;
        let balance_data: ExtendedResponse<ExtendedBalanceData> = 
            self.client.parse_json(balance_response).await?;
        
        if balance_data.status != "OK" {
            let error_msg = balance_data.error
                .map(|e| e.message)
                .unwrap_or_else(|| "Unknown error".to_string());
            return Err(PointsBotError::Exchange {
                code: balance_data.status,
                message: format!("Extended API error: {}", error_msg),
            });
        }
        
        let balance = balance_data.data
            .ok_or_else(|| PointsBotError::Exchange {
                code: "NO_DATA".to_string(),
                message: "Missing balance data".to_string(),
            })?;
        
        // Get positions to calculate total position value
        let positions_response = self.client.get("/user/positions", Some(headers)).await?;
        let positions_data: ExtendedResponse<Vec<ExtendedPositionData>> = 
            self.client.parse_json(positions_response).await?;
        
        let positions = if positions_data.status == "OK" {
            positions_data.data.unwrap_or_default()
        } else {
            Vec::new()
        };
        
        // Calculate total position value and margin
        let total_position_value = positions.iter()
            .map(|pos| pos.notional.parse::<f64>().unwrap_or(0.0))
            .sum::<f64>();
        
        let total_margin = positions.iter()
            .map(|pos| pos.margin.parse::<f64>().unwrap_or(0.0))
            .sum::<f64>();
        
        let account_value = balance.equity.parse::<f64>().unwrap_or(0.0);
        let available_balance = balance.available_for_withdrawal.parse::<f64>().unwrap_or(0.0);
        let total_raw_usd = balance.balance.parse::<f64>().unwrap_or(0.0);
        
        Ok(AccountBalance {
            account_value,
            total_margin_used: total_margin,
            total_ntl_pos: total_position_value,
            total_raw_usd,
            withdrawable: available_balance,
            available_balance,
            positions_count: positions.len() as i32,
            exchange: ExchangeName::Extended,
            timestamp: balance.updated_time,
        })
    }
    
    async fn get_user_positions(&self, _address: &str) -> PointsBotResult<Vec<Position>> {
        if !self.is_authenticated() {
            return Err(PointsBotError::Auth("Extended API requires authentication".to_string()));
        }
        
        let headers = self.get_auth_headers()?;
        
        let response = self.client.get("/user/positions", Some(headers)).await?;
        let data: ExtendedResponse<Vec<ExtendedPositionData>> = 
            self.client.parse_json(response).await?;
        
        if data.status != "OK" {
            let error_msg = data.error
                .map(|e| e.message)
                .unwrap_or_else(|| "Unknown error".to_string());
            return Err(PointsBotError::Exchange {
                code: data.status,
                message: format!("Extended API error: {}", error_msg),
            });
        }
        
        let positions_data = data.data.unwrap_or_default();
        
        let mut positions = Vec::new();
        for pos_data in positions_data {
            let side = match pos_data.side.as_str() {
                "long" => PositionSide::Long,
                "short" => PositionSide::Short,
                _ => continue, // Skip unknown sides
            };
            
            positions.push(Position {
                symbol: pos_data.asset_name,
                size: pos_data.notional,
                side,
                entry_price: pos_data.entry_price.parse::<f64>().unwrap_or(0.0),
                mark_price: pos_data.mark_price.parse::<f64>().unwrap_or(0.0),
                unrealized_pnl: pos_data.unrealized_pnl.parse::<f64>().unwrap_or(0.0),
                margin_used: pos_data.margin.parse::<f64>().unwrap_or(0.0),
                liquidation_price: None, // Extended doesn't provide liquidation price
            });
        }
        
        Ok(positions)
    }
    
    async fn get_supported_tokens(&self) -> PointsBotResult<Vec<String>> {
        // Public endpoint, no authentication required
        let response = self.client.get("/info/markets", None).await?;
        let data: ExtendedResponse<Vec<ExtendedMarketData>> = 
            self.client.parse_json(response).await?;
        
        if data.status != "OK" {
            let error_msg = data.error
                .map(|e| e.message)
                .unwrap_or_else(|| "Unknown error".to_string());
            return Err(PointsBotError::Exchange {
                code: data.status,
                message: format!("Extended API error: {}", error_msg),
            });
        }
        
        let markets = data.data.unwrap_or_default();
        
        Ok(markets.into_iter()
            .map(|market| market.asset_name)
            .collect())
    }
    
    async fn get_funding_rates(&self) -> PointsBotResult<Vec<FundingRate>> {
        // Public endpoint, no authentication required
        let response = self.client.get("/info/markets", None).await?;
        let data: ExtendedResponse<Vec<ExtendedMarketData>> = 
            self.client.parse_json(response).await?;
        
        if data.status != "OK" {
            let error_msg = data.error
                .map(|e| e.message)
                .unwrap_or_else(|| "Unknown error".to_string());
            return Err(PointsBotError::Exchange {
                code: data.status,
                message: format!("Extended API error: {}", error_msg),
            });
        }
        
        let markets = data.data.unwrap_or_default();
        let now = current_timestamp();
        
        let mut funding_rates = Vec::new();
        for market in markets {
            let rate = Decimal::from_str(&market.market_stats.funding_rate)?;
            
            funding_rates.push(FundingRate {
                symbol: market.asset_name,
                rate,
                next_funding_time: now + chrono::Duration::hours(8), // Assuming 8-hour funding
            });
        }
        
        Ok(funding_rates)
    }
    
    fn exchange_name(&self) -> ExchangeName {
        ExchangeName::Extended
    }
} 