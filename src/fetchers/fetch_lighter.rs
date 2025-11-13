// Helper to extract price and size from order book array
use super::base::{AccountData, Fetcher, MarketInfo, Position};
use crate::{
    AssetMapping, BotJsonConfig, ExchangeName, HttpClient, PointsBotError, PointsBotResult, PositionSide,
    TickerDirection,
};
use async_trait::async_trait;
use async_tungstenite::{tokio::connect_async, tungstenite::Message};
use futures::{SinkExt, StreamExt};
use log::info;
use rust_decimal::{prelude::FromPrimitive, Decimal};
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    str::FromStr,
};
use tokio::time::{timeout, Duration};

pub struct FetcherLighter {
    client: HttpClient,
    wallet: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ApiAccountResponse {
    code: u32,
    total: u32,
    accounts: Vec<ApiAccount>,
}

#[derive(Debug, Deserialize)]
struct ApiAccount {
    available_balance: String,
    collateral: String,
    positions: Vec<ApiPosition>,
    total_asset_value: String,
    cross_asset_value: String,
}

#[derive(Debug, Deserialize)]
struct ApiPosition {
    symbol: String,
    sign: i32,
    position: String,
    avg_entry_price: String,
    unrealized_pnl: String,
    liquidation_price: String,
}

#[derive(Debug)]
struct MarketInfoBuilder {
    exchange_id: Option<u64>,
    symbol: String,
    base_asset: String,
    quote_asset: String,
    bid_price: Option<Decimal>,
    ask_price: Option<Decimal>,
    leverage: Option<Decimal>,
    funding_rate: Option<Decimal>,
    sz_decimals: Option<Decimal>,
    px_decimals: Option<Decimal>,
    min_order_size_change: Option<Decimal>,
    last_trade_price: Option<Decimal>,
}

impl FetcherLighter {
    pub fn new(config: &BotJsonConfig) -> Self {
        let client = HttpClient::new("https://mainnet.zklighter.elliot.ai/api/v1/".to_string(), Some(1000));

        let wallet = config.wallet_address.clone();

        Self { client, wallet }
    }
}

#[async_trait]
impl Fetcher for FetcherLighter {
    fn get_exchange_info(&self) -> ExchangeName {
        ExchangeName::Lighter
    }

    async fn get_account_data(&self) -> PointsBotResult<AccountData> {
        let address = self.wallet.as_ref().ok_or_else(|| PointsBotError::Config {
            msg: "Wallet address not configured for Fetcher Lighter".to_string(),
            source: None,
        })?;
        let url = format!("account?by=l1_address&value={}", address);
        let resp = self.client.get(&url, None).await?;
        let resp_text = resp.text().await?;
        let api_resp: ApiAccountResponse = serde_json::from_str(&resp_text)?;
        let account = &api_resp.accounts[0];

        let positions = account
            .positions
            .iter()
            .filter_map(|p| {
                let size = Decimal::from_str(&p.position).unwrap_or(Decimal::ZERO);
                if size.is_zero() {
                    None
                } else {
                    Some(Position {
                        symbol: AssetMapping::map_ticker(
                            ExchangeName::Lighter,
                            &p.symbol,
                            TickerDirection::ToCanonical,
                        )
                        .unwrap_or_else(|| p.symbol.clone()),
                        size,
                        side: match p.sign {
                            1 => PositionSide::Long,
                            -1 => PositionSide::Short,
                            _ => PositionSide::Long,
                        },
                        entry_price: Decimal::from_str(&p.avg_entry_price).unwrap_or(Decimal::ZERO),
                        unrealized_pnl: Decimal::from_str(&p.unrealized_pnl).unwrap_or(Decimal::ZERO),
                        margin_used: Decimal::ZERO,
                        liquidation_price: Decimal::from_str(&p.liquidation_price).ok(),
                        cum_funding: None,
                    })
                }
            })
            .collect();

        Ok(AccountData {
            account_value: Decimal::from_str(&account.total_asset_value).unwrap_or(Decimal::ZERO)
                * Decimal::from_f64(0.99).unwrap(),
            total_margin_used: Decimal::ZERO,
            total_ntl_pos: Decimal::ZERO,
            total_raw_usd: Decimal::ZERO,
            withdrawable: Decimal::from_str(&account.collateral).unwrap_or(Decimal::ZERO),
            available_balance: Decimal::from_str(&account.available_balance).unwrap_or(Decimal::ZERO),
            positions,
            exchange: ExchangeName::Lighter,
            timestamp: 0,
        })
    }

    async fn get_markets(&self) -> PointsBotResult<Vec<MarketInfo>> {
        let mut builders = fetch_market_details(&self.client).await?;
        // let market_ids: Vec<u32> = builders.keys().copied().collect();

        let lighter_rates = fetch_lighter_funding_rates(&self.client).await?;
        for (market_id, _symbol, rate) in &lighter_rates {
            if let Some(b) = builders.get_mut(market_id) {
                b.funding_rate = Some(Decimal::from_f64(*rate).unwrap_or(Decimal::ZERO));
            }
        }

        /* let (ws_stream, _) = connect_async("wss://mainnet.zklighter.elliot.ai/stream").await?;
        let (mut write, mut read) = ws_stream.split();
        subscribe_and_process_order_books(&mut write, &mut read, &mut builders, &market_ids).await; */

        for b in builders.values_mut() {
            if let Some(lp) = b.last_trade_price {
                let offset = Decimal::from_f64(0.001).unwrap(); // 10 bips
                b.bid_price = Some(lp * (Decimal::ONE - offset));
                b.ask_price = Some(lp * (Decimal::ONE + offset));
            }
        }

        Ok(builders
            .into_iter()
            .map(|(_, b)| {
                let symbol = AssetMapping::map_ticker(ExchangeName::Lighter, &b.symbol, TickerDirection::ToCanonical)
                    .unwrap_or_else(|| b.symbol.clone());
                MarketInfo {
                    exchange: ExchangeName::Lighter,
                    exchange_id: b.exchange_id,
                    symbol: symbol.clone(),
                    base_asset: symbol.clone(),
                    quote_asset: "USD".to_string(),
                    bid_price: b.bid_price.unwrap_or(Decimal::ZERO),
                    ask_price: b.ask_price.unwrap_or(Decimal::ZERO),
                    leverage: b.leverage.unwrap_or(Decimal::ZERO),
                    funding_rate: b.funding_rate.unwrap_or(Decimal::ZERO),
                    sz_decimals: b.sz_decimals.unwrap_or(Decimal::ZERO),
                    px_decimals: b.px_decimals.unwrap_or(Decimal::ZERO),
                    min_order_size_change: b.min_order_size_change.unwrap_or(Decimal::ZERO),
                }
            })
            .collect())
    }
}

async fn fetch_market_details(http_client: &crate::HttpClient) -> PointsBotResult<HashMap<u32, MarketInfoBuilder>> {
    let rest_resp = http_client.get("orderBookDetails", None).await?;
    let rest_text = rest_resp.text().await?;
    let details: Value = serde_json::from_str(&rest_text)?;
    let mut builders: HashMap<u32, MarketInfoBuilder> = HashMap::new();

    for market in details["order_book_details"].as_array().unwrap_or(&vec![]) {
        let market_id = market["market_id"].as_u64().unwrap_or(0) as u32;
        builders.insert(
            market_id,
            MarketInfoBuilder {
                exchange_id: Some(market_id as u64),
                symbol: market["symbol"].as_str().unwrap_or("").to_string(),
                base_asset: market["symbol"].as_str().unwrap_or("").to_string(),
                quote_asset: "USD".to_string(),
                bid_price: None,
                ask_price: None,
                leverage: market["default_initial_margin_fraction"]
                    .as_i64()
                    .filter(|&v| v != 0)
                    .map(|v| {
                        let lev = Decimal::from_i64(10000).unwrap() / Decimal::from_i64(v).unwrap();
                        lev.round_dp(2)
                    }),
                funding_rate: None,
                sz_decimals: market["size_decimals"]
                    .as_i64()
                    .map(|v| Decimal::from_i64(v).unwrap_or(Decimal::ZERO)),
                px_decimals: market["price_decimals"]
                    .as_i64()
                    .map(|v| Decimal::from_i64(v).unwrap_or(Decimal::ZERO)),
                min_order_size_change: None,
                last_trade_price: market["last_trade_price"]
                    .as_f64()
                    .map(|f| Decimal::from_f64(f).unwrap_or(Decimal::ZERO)),
            },
        );
    }
    Ok(builders)
}

/* async fn subscribe_and_process_order_books(
    write: &mut (impl futures::Sink<Message, Error = async_tungstenite::tungstenite::Error> + Unpin),
    read: &mut (impl futures::Stream<Item = Result<Message, async_tungstenite::tungstenite::Error>> + Unpin),
    builders: &mut HashMap<u32, MarketInfoBuilder>,
    market_ids: &[u32],
) {
    // Subscribe to all order books up front
    info!("Subscribing to order_books/{:?}", market_ids);

    for &market_id in market_ids {
        let _ = write
            .send(Message::Text(
                serde_json::json!({"type": "subscribe", "channel": format!("order_book/{}", market_id)})
                    .to_string()
                    .into(),
            ))
            .await;
    }

    let mut processed: HashSet<u32> = HashSet::new();

    info!("Starting to process order book messages");
    while processed.len() < market_ids.len() {
        // Wait max 5 seconds for a message
        let msg = match timeout(Duration::from_secs(1), read.next()).await {
            Ok(Some(Ok(Message::Text(txt)))) => txt,
            Ok(_) | Err(_) => {
                // Mark one unprocessed market_id as processed to avoid infinite loop
                if let Some(&unprocessed_id) = market_ids.iter().find(|id| !processed.contains(id)) {
                    info!(
                        "Timeout waiting for order book message, skipping one market... {}",
                        unprocessed_id
                    );
                    processed.insert(unprocessed_id);
                }
                continue;
            }
        };
        let v: Value = serde_json::from_str(&msg).unwrap_or(Value::Null);
        let msg_type = v["type"].as_str().unwrap_or("");

        if msg_type == "ping" {
            info!("Received ping");
            let _ = write
                .send(Message::Text("{\"type\":\"pong\"}".to_string().into()))
                .await;
            continue;
        }

        if msg_type == "subscribed/order_book" {
            let channel = v["channel"].as_str().unwrap_or("");
            let id = channel.split(':').nth(1).and_then(|s| s.parse::<u32>().ok());
            info!("Subscribed to order_book for market_id {:?}", id);
            if let Some(market_id) = id {
                if !processed.contains(&market_id) {
                    if let Some(b) = builders.get_mut(&market_id) {
                        let ob = &v["order_book"];
                        b.bid_price = extract_price(&ob["bids"], market_id);
                        b.ask_price = extract_price(&ob["asks"], market_id);

                        let _ = write
                            .send(Message::Text(
                                serde_json::json!({
                                    "type": "unsubscribe",
                                    "channel": format!("order_book/{}", market_id)
                                })
                                .to_string()
                                .into(),
                            ))
                            .await;

                        processed.insert(market_id);
                    }
                }
            }
        }
    }
} */

async fn fetch_lighter_funding_rates(http_client: &crate::HttpClient) -> PointsBotResult<Vec<(u32, String, f64)>> {
    let rest_resp = http_client.get("funding-rates", None).await?;
    let rest_text = rest_resp.text().await?;
    let json: serde_json::Value = serde_json::from_str(&rest_text)?;
    let mut rates = Vec::new();
    if let Some(arr) = json["funding_rates"].as_array() {
        for entry in arr {
            if entry["exchange"].as_str() == Some("lighter") {
                let market_id = entry["market_id"].as_u64().unwrap_or(0) as u32;
                let symbol = entry["symbol"].as_str().unwrap_or("").to_string();
                let rate = entry["rate"].as_f64().unwrap_or(0.0) / 8.0;
                rates.push((market_id, symbol, rate));
            }
        }
    }
    Ok(rates)
}

fn extract_price(array: &serde_json::Value, market_id: u32) -> Option<Decimal> {
    let arr = array.as_array()?;
    let first = arr.get(0)?;
    let price = first.get("price")?.as_str().and_then(|s| Decimal::from_str(s).ok())?;
    if price.is_zero() {
        info!("Market {} has zero bid or ask price!", market_id);
    }
    Some(price)
}
