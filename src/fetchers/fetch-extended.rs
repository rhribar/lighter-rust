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
    str_to_decimal, AssetMapping, ExchangeName, PointsBotError, PointsBotResult
};
use super::base::{HttpClient, Fetcher, AccountData, Position, MarketInfo, PositionSide};

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
    market: String,
    side: String,
    size: String,
    #[serde(rename = "value")]
    position_value: String,
    margin: String,
    #[serde(rename = "openPrice")]
    entry_price: String,
    #[serde(rename = "markPrice")]
    mark_price: String,
    #[serde(rename = "unrealisedPnl")]
    unrealized_pnl: String,
    #[serde(rename = "liquidationPrice")]
    liquidation_price: String,
}

#[derive(Debug, Deserialize)]
struct ExtendedMarketData {
    #[serde(rename = "name")]
    name: String,
    #[serde(rename = "marketStats")]
    market_stats: ExtendedMarketStats,
    #[serde(rename = "tradingConfig")]
    trading_config: ExtendedTradingConfig,
}

#[derive(Debug, Deserialize)]
struct ExtendedMarketStats {
    #[serde(rename = "fundingRate")]
    funding_rate: String,
    #[serde(rename = "bidPrice")]
    bid_price: String,
    #[serde(rename = "askPrice")]
    ask_price: String,
}

#[derive(Debug, Deserialize)]
struct ExtendedTradingConfig {
    #[serde(rename = "maxLeverage")]
    max_leverage: String,
    #[serde(rename = "minOrderSize")]
    min_order_size: String, // TODO: should probably handle this better
    #[serde(rename = "maxLimitOrderValue")]
    max_limit_order_value: String,
}

/// Extended Exchange Fetcher
pub struct FetcherExtended {
    client: HttpClient,
    api_key: Option<String>,
}

impl FetcherExtended {
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
impl Fetcher for FetcherExtended {
    async fn get_account_data(&self, address: &str) -> PointsBotResult<AccountData> {
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
        
        // Store the response body in a variable for reuse
        let raw_positions_response = positions_response.text().await?;
        // print!("[DEBUG] Raw positions response body: {:?}", raw_positions_response);

        let positions_data: ExtendedResponse<Vec<ExtendedPositionData>> = 
            serde_json::from_str(&raw_positions_response)?;
        
        let positions = if positions_data.status == "OK" {
            positions_data.data.unwrap_or_default()
        } else {
            Vec::new()
        };
        
        // Calculate total position value and margin
        let total_position_value = positions.iter()
            .map(|pos| pos.position_value.parse::<Decimal>().unwrap_or(Decimal::ZERO))
            .sum::<Decimal>();
        
        let total_margin = positions.iter()
            .map(|pos| pos.margin.parse::<Decimal>().unwrap_or(Decimal::ZERO))
            .sum::<Decimal>();
        
        let account_value = balance.equity.parse::<Decimal>().unwrap_or(Decimal::ZERO);
        let available_balance = balance.available_for_withdrawal.parse::<Decimal>().unwrap_or(Decimal::ZERO);
        let total_raw_usd = balance.balance.parse::<Decimal>().unwrap_or(Decimal::ZERO);
        
        let positions = positions
            .into_iter()
            .filter_map(|pos| {
                let side = match pos.side.to_uppercase().as_str() {
                    "LONG" => PositionSide::Long,
                    "SHORT" => PositionSide::Short,
                    _ => return None,
                };

                Some(Position {
                    symbol: AssetMapping::get_canonical_ticker(ExchangeName::Extended, &pos.market).unwrap_or_else(|| pos.market.clone()),
                    side,
                    size: str_to_decimal(&pos.size).ok()?.abs(),
                    entry_price: str_to_decimal(&pos.entry_price).ok()?,
                    unrealized_pnl: str_to_decimal(&pos.unrealized_pnl).ok()?,
                    margin_used: str_to_decimal(&pos.margin).ok()?,
                    liquidation_price: str_to_decimal(&pos.liquidation_price).ok(),
                })
            })
            .collect();

        Ok(AccountData {
            account_value,
            total_margin_used: total_margin,
            total_ntl_pos: total_position_value,
            total_raw_usd,
            withdrawable: available_balance,
            available_balance,
            positions,
            exchange: ExchangeName::Extended,
            timestamp: balance.updated_time,
        })
    }
    
    async fn get_markets(&self) -> PointsBotResult<Vec<MarketInfo>> {
        let headers = Some(HashMap::from([(
            "User-Agent".to_string(), "rust-client".to_string()
        )]));
        let response = self.client.get("/info/markets", headers).await?;
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

        let mut market_infos = Vec::new();
        for market in markets {
            let symbol = AssetMapping::get_canonical_ticker(ExchangeName::Extended, &market.name)
                .unwrap_or_else(|| market.name.clone());

            let funding_rate = Decimal::from_str(&market.market_stats.funding_rate)?;
            let bid_price = Decimal::from_str(&market.market_stats.bid_price)?;
            let ask_price = Decimal::from_str(&market.market_stats.ask_price)?;

            market_infos.push(MarketInfo {
                symbol: symbol.clone(),
                base_asset: symbol,
                quote_asset: "USD".to_string(),
                bid_price,
                ask_price,
                leverage: Decimal::from_str(&market.trading_config.max_leverage)?,
                funding_rate,
                min_order_size: Some(Decimal::from_str(&market.trading_config.min_order_size)?),
            });
        }

        Ok(market_infos)
    }
}
