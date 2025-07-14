/// Hyperliquid Exchange Fetcher
/// 
/// Fetches account and trading data from Hyperliquid exchange.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;
use rust_decimal::Decimal;
use rust_decimal::prelude::*;

use crate::{
    ExchangeName, PointsBotResult, PointsBotError, str_to_decimal, current_timestamp
};
use super::base::{HttpClient, Fetcher, AccountBalance, Position, FundingRate, PositionSide};

/// Hyperliquid API response structures
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HyperliquidAccountData {
    margin_summary: HyperliquidMarginSummary,
    asset_positions: Vec<HyperliquidPosition>,
    withdrawable: String,
    time: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HyperliquidMarginSummary {
    account_value: String,
    total_margin_used: String,
    total_ntl_pos: String,
    total_raw_usd: String,
}

#[derive(Debug, Deserialize)]
struct HyperliquidPosition {
    coin: String,
    #[serde(rename = "entryPx")]
    entry_px: String,
    #[serde(rename = "liquidationPx")]
    liquidation_px: String,
    #[serde(rename = "marginUsed")]
    margin_used: String,
    #[serde(rename = "markPx")]
    mark_px: String,
    szi: String,
    #[serde(rename = "unrealizedPnl")]
    unrealized_pnl: String,
}

#[derive(Debug, Deserialize)]
struct HyperliquidMeta {
    universe: Vec<AssetCtx>,
    time: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct AssetCtx {
    name: String,
}

#[derive(Debug, Deserialize)]
struct HyperliquidAssetCtx {
    funding: String,
    #[serde(rename = "markPx")]
    mark_px: String,
}

pub struct HyperliquidFetcher {
    client: HttpClient,
}

impl HyperliquidFetcher {
    pub fn new() -> Self {
        let client = HttpClient::new(
            "https://api.hyperliquid.xyz".to_string(),
            Some(100),
        );
        
        Self { client }
    }
}

#[async_trait]
impl Fetcher for HyperliquidFetcher {
    async fn get_account_data(&self, address: &str) -> PointsBotResult<AccountBalance> {
        let payload = json!({
            "type": "clearinghouseState",
            "user": address
        });
        
        let response = self.client.post("/info", &payload.to_string(), None).await?;
        let account_data: HyperliquidAccountData = self.client.parse_json(response).await?;
        
        let account_value = str_to_decimal(&account_data.margin_summary.account_value)?.to_f64().unwrap_or(0.0);
        let total_margin_used = str_to_decimal(&account_data.margin_summary.total_margin_used)?.to_f64().unwrap_or(0.0);
        let total_ntl_pos = str_to_decimal(&account_data.margin_summary.total_ntl_pos)?.to_f64().unwrap_or(0.0);
        let total_raw_usd = str_to_decimal(&account_data.margin_summary.total_raw_usd)?.to_f64().unwrap_or(0.0);
        let withdrawable = str_to_decimal(&account_data.withdrawable)?.to_f64().unwrap_or(0.0);
        
        let available_balance = withdrawable;
        let positions_count = account_data.asset_positions.len() as i32;
        
        Ok(AccountBalance {
            account_value,
            total_margin_used,
            total_ntl_pos,
            total_raw_usd,
            withdrawable,
            available_balance,
            positions_count,
            exchange: ExchangeName::Hyperliquid,
            timestamp: account_data.time.unwrap_or(current_timestamp().timestamp_millis()),
        })
    }
    
    async fn get_user_positions(&self, address: &str) -> PointsBotResult<Vec<Position>> {
        let payload = json!({
            "type": "clearinghouseState",
            "user": address
        });
        
        let response = self.client.post("/info", &payload.to_string(), None).await?;
        let account_data: HyperliquidAccountData = self.client.parse_json(response).await?;
        
        let mut positions = Vec::new();
        for position in account_data.asset_positions {
            let size_decimal = str_to_decimal(&position.szi)?;
            if size_decimal == Decimal::ZERO {
                continue;
            }
            
            let side = if size_decimal > Decimal::ZERO { 
                PositionSide::Long 
            } else { 
                PositionSide::Short 
            };
            
            let entry_price = str_to_decimal(&position.entry_px)?.to_f64().unwrap_or(0.0);
            let mark_price = str_to_decimal(&position.mark_px)?.to_f64().unwrap_or(0.0);
            let unrealized_pnl = str_to_decimal(&position.unrealized_pnl)?.to_f64().unwrap_or(0.0);
            let margin_used = str_to_decimal(&position.margin_used)?.to_f64().unwrap_or(0.0);
            let liquidation_price = str_to_decimal(&position.liquidation_px).ok().and_then(|d| d.to_f64());
            
            positions.push(Position {
                symbol: position.coin,
                size: size_decimal.abs().to_string(),
                side,
                entry_price,
                mark_price,
                unrealized_pnl,
                margin_used,
                liquidation_price,
            });
        }
        
        Ok(positions)
    }
    
    async fn get_supported_tokens(&self) -> PointsBotResult<Vec<String>> {
        let payload = json!({
            "type": "meta"
        });
        
        let response = self.client.post("/info", &payload.to_string(), None).await?;
        let meta_data: HyperliquidMeta = self.client.parse_json(response).await?;
        
        let tokens = meta_data.universe.into_iter()
            .map(|asset| asset.name)
            .collect();
        
        Ok(tokens)
    }
    
    async fn get_funding_rates(&self) -> PointsBotResult<Vec<FundingRate>> {
        let payload = json!({
            "type": "metaAndAssetCtxs"
        });
        
        let response = self.client.post("/info", &payload.to_string(), None).await?;
        let data: serde_json::Value = self.client.parse_json(response).await?;
        
        let data_array = data.as_array()
            .ok_or_else(|| PointsBotError::Parse("Response is not an array".to_string()))?;
        
        if data_array.len() < 2 {
            return Err(PointsBotError::Parse("Response array too short".to_string()));
        }
        
        let meta_data: HyperliquidMeta = serde_json::from_value(data_array[0].clone())
            .map_err(|e| PointsBotError::Parse(format!("Failed to parse meta: {}", e)))?;
        
        let asset_ctxs: Vec<HyperliquidAssetCtx> = serde_json::from_value(data_array[1].clone())
            .map_err(|e| PointsBotError::Parse(format!("Failed to parse asset contexts: {}", e)))?;
        
        let mut funding_rates = Vec::new();
        for (i, token_info) in meta_data.universe.iter().enumerate() {
            if i < asset_ctxs.len() {
                let ctx = &asset_ctxs[i];
                let funding_rate = str_to_decimal(&ctx.funding)?;
                
                funding_rates.push(FundingRate {
                    symbol: token_info.name.clone(),
                    rate: funding_rate * Decimal::from(8), // 8-hour projection as per Python
                    next_funding_time: current_timestamp() + chrono::Duration::hours(1),
                });
            }
        }
        
        Ok(funding_rates)
    }
    
    fn exchange_name(&self) -> ExchangeName {
        ExchangeName::Hyperliquid
    }
} 