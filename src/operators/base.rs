use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use crate::{ExchangeName, PointsBotResult, PositionSide};

pub use crate::http_client::HttpClient;

#[async_trait]
pub trait Operator: Send + Sync {
    async fn create_order(&self, order: OrderRequest) -> PointsBotResult<OrderResponse>;

    async fn change_leverage(&self, symbol: String, leverage: Decimal) -> PointsBotResult<()>;

    fn get_exchange_name(&self) -> ExchangeName;
}


#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderType {
    #[serde(rename = "market")]
    Market,
    #[serde(rename = "limit")]
    Limit,
    #[serde(rename = "stop")]
    Stop,
    #[serde(rename = "stop_limit")]
    StopLimit,
}

impl OrderType {
    pub fn as_str(&self) -> &'static str {
        match self {
            OrderType::Market => "market",
            OrderType::Limit => "limit",
            OrderType::Stop => "stop",
            OrderType::StopLimit => "stop_limit",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderRequest {
    pub symbol: String,
    pub side: PositionSide,
    pub order_type: OrderType,
    pub quantity: Decimal,
    pub price: Option<Decimal>,
    pub stop_price: Option<Decimal>,
    pub time_in_force: Option<String>,
    pub reduce_only: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderStatus {
    WaitingForFill,
    Resting,
    Filled,
    Cancelled,
    Unknown,
}

impl OrderStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            OrderStatus::WaitingForFill => "waiting_for_fill",
            OrderStatus::Resting => "resting",
            OrderStatus::Filled => "filled",
            OrderStatus::Cancelled => "cancelled",
            OrderStatus::Unknown => "unknown",
        }
    }
} 

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderResponse {
    pub order_id: String,
    pub symbol: String,
    pub side: PositionSide,
    pub status: OrderStatus,
    pub filled_quantity: Decimal,
    pub remaining_quantity: Decimal,
    pub average_price: Option<Decimal>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChangeLeverageRequest {
    pub market: String,
    pub leverage: String,
}
