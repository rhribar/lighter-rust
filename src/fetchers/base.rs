pub use crate::http_client::HttpClient;
use crate::{ExchangeName, PointsBotResult, PositionSide};
use async_trait::async_trait;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[async_trait]
pub trait Fetcher: Send + Sync {
    fn get_exchange_info(&self) -> ExchangeName;

    async fn get_account_data(&self) -> PointsBotResult<AccountData>;

    async fn get_markets(&self) -> PointsBotResult<Vec<MarketInfo>>;
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub symbol: String,
    pub size: Decimal,
    pub side: PositionSide,
    pub entry_price: Decimal,
    pub unrealized_pnl: Decimal,
    pub margin_used: Decimal,
    pub liquidation_price: Option<Decimal>,
    pub cum_funding: Option<Decimal>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketInfo {
    pub exchange_id: Option<u64>,
    pub symbol: String,
    pub base_asset: String,
    pub quote_asset: String,
    pub bid_price: Decimal,
    pub ask_price: Decimal,
    pub leverage: Decimal,
    pub funding_rate: Decimal,
    pub sz_decimals: Decimal,
    pub px_decimals: Decimal,
    pub min_order_size_change: Decimal,
}
