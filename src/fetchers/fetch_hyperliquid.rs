use async_trait::async_trait;
use chrono::Utc;
use log::info;
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::json;

use super::base::{AccountData, Fetcher, HttpClient, MarketInfo, Position};
use crate::{parse_decimal, AssetMapping, ExchangeName, PointsBotError, PointsBotResult, PositionSide, TickerDirection};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HyperliquidAccountData {
    margin_summary: HyperliquidMarginSummary,
    withdrawable: String,
    asset_positions: Vec<HyperliquidAssetPosition>,
    time: Option<i64>,
    _cross_margin_summary: Option<HyperliquidMarginSummary>,
    _cross_maintenance_margin_used: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HyperliquidAssetPosition {
    position: HyperliquidPosition,
    #[serde(rename = "type")]
    _position_type: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HyperliquidPosition {
    coin: Option<String>,
    szi: String,
    entry_px: String,
    unrealized_pnl: String,
    liquidation_px: Option<String>,
    margin_used: Option<String>,
    cum_funding: HyperliquidCumFunding,
    _max_leverage: u32,
    _leverage: HyperliquidLeverage,
    _return_on_equity: String,
    _position_value: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HyperliquidLeverage {
    #[serde(rename = "type")]
    _leverage_type: String,
    _value: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HyperliquidCumFunding {
    since_open: String,
    _all_time: String,
    _since_change: String,
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
struct HyperliquidMeta {
    universe: Vec<AssetCtx>,
    _time: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct AssetCtx {
    name: String,
    #[serde(rename = "maxLeverage")]
    max_leverage: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct HyperliquidAssetCtx {
    funding: String,
    #[serde(rename = "markPx")]
    mark_px: String,
}

pub struct FetcherHyperliquid {
    client: HttpClient,
}

impl FetcherHyperliquid {
    pub fn new() -> Self {
        let client = HttpClient::new("https://api.hyperliquid.xyz".to_string(), Some(100));

        Self { client }
    }

    fn parse_position(asset_position: &HyperliquidAssetPosition) -> Option<Position> {
        let position = &asset_position.position;
        let size_decimal = parse_decimal(&position.szi).ok()?;
        if size_decimal == Decimal::ZERO {
            return None;
        }
        let side = if size_decimal > Decimal::ZERO {
            PositionSide::Long
        } else {
            PositionSide::Short
        };
        let entry_price = parse_decimal(&position.entry_px).ok()?;
        let unrealized_pnl = parse_decimal(&position.unrealized_pnl).ok()?;
        let margin_used = parse_decimal(position.margin_used.as_deref().unwrap_or("0")).ok()?;
        let liquidation_price = parse_decimal(position.liquidation_px.as_deref().unwrap_or("0")).ok()?;
        Some(Position {
            symbol: AssetMapping::map_ticker(
                ExchangeName::Hyperliquid,
                &position.coin.clone().unwrap_or_default(),
                TickerDirection::ToCanonical,
            )
            .unwrap_or_else(|| position.coin.clone().unwrap_or_default()),
            size: size_decimal.abs(),
            side,
            entry_price,
            unrealized_pnl,
            margin_used,
            liquidation_price: Some(liquidation_price),
            cum_funding: Some(parse_decimal(&position.cum_funding.since_open).ok().unwrap_or(Decimal::ZERO)),
        })
    }
}

#[async_trait]
impl Fetcher for FetcherHyperliquid {
    async fn get_account_data(&self, address: &str) -> PointsBotResult<AccountData> {
        let payload = json!({
            "type": "clearinghouseState",
            "user": address
        });

        let response = self.client.post("/info", &payload.to_string(), None).await?;
        let response_body = response.text().await?;
        info!("Response body: {}", response_body);

        let account_data: HyperliquidAccountData = serde_json::from_str(&response_body)?;

        let account_value = parse_decimal(&account_data.margin_summary.account_value)?;
        let total_margin_used = parse_decimal(&account_data.margin_summary.total_margin_used)?;
        let total_ntl_pos = parse_decimal(&account_data.margin_summary.total_ntl_pos)?;
        let total_raw_usd = parse_decimal(&account_data.margin_summary.total_raw_usd)?;
        let withdrawable = parse_decimal(&account_data.withdrawable)?;
        let available_balance = withdrawable;

        let positions: Vec<Position> = account_data.asset_positions.iter().filter_map(Self::parse_position).collect();

        Ok(AccountData {
            account_value,
            total_margin_used,
            total_ntl_pos,
            total_raw_usd,
            withdrawable,
            available_balance,
            positions,
            exchange: ExchangeName::Hyperliquid,
            timestamp: account_data.time.unwrap_or(Utc::now().timestamp_millis()),
        })
    }

    async fn get_markets(&self) -> PointsBotResult<Vec<MarketInfo>> {
        let payload = json!({
            "type": "metaAndAssetCtxs"
        });

        let response = self.client.post("/info", &payload.to_string(), None).await?;
        let data: serde_json::Value = self.client.parse_json(response).await?;

        let data_array = data.as_array().ok_or_else(|| PointsBotError::Parse {
            msg: "Response is not an array".to_string(),
            source: None,
        })?;

        if data_array.len() < 2 {
            return Err(PointsBotError::Parse {
                msg: "Response array too short".to_string(),
                source: None,
            });
        }

        let meta_data: HyperliquidMeta = serde_json::from_value(data_array[0].clone()).map_err(|e| PointsBotError::Parse {
            msg: "Failed to parse meta".to_string(),
            source: Some(Box::new(e)),
        })?;

        let asset_ctxs: Vec<HyperliquidAssetCtx> = serde_json::from_value(data_array[1].clone()).map_err(|e| PointsBotError::Parse {
            msg: "Failed to parse asset contexts".to_string(),
            source: Some(Box::new(e)),
        })?;

        let mut markets = Vec::new();
        for (i, token_info) in meta_data.universe.iter().enumerate() {
            if i < asset_ctxs.len() {
                let ctx = &asset_ctxs[i];
                let funding_rate = parse_decimal(&ctx.funding)?;
                let bid_price = parse_decimal(&ctx.mark_px)?;
                let ask_price = parse_decimal(&ctx.mark_px)?;
                let symbol = AssetMapping::map_ticker(ExchangeName::Hyperliquid, &token_info.name.clone(), TickerDirection::ToCanonical)
                    .unwrap_or_else(|| token_info.name.clone());

                markets.push(MarketInfo {
                    symbol: symbol.clone(),
                    base_asset: symbol.clone(),
                    quote_asset: "USD".to_string(),
                    bid_price,
                    ask_price,
                    leverage: Decimal::from(token_info.max_leverage.unwrap_or(5)),
                    funding_rate,
                    min_order_size: None,
                });
            }
        }

        Ok(markets)
    }
}
