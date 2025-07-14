use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use crate::{PointsBotResult, PointsBotError, ExchangeName, Side};

pub use crate::http_client::HttpClient;

#[async_trait]
pub trait Operator: Send + Sync {
    async fn create_order(&self, order: OrderRequest) -> PointsBotResult<OrderResponse>;

    async fn close_position(&self, symbol: &str) -> PointsBotResult<ClosePositionResponse>;

    fn exchange_name(&self) -> ExchangeName;
}

// ===== OPERATOR TYPES =====

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderRequest {
    pub symbol: String,
    pub side: Side,
    pub order_type: OrderType,
    pub quantity: Decimal,
    pub price: Option<Decimal>,
    pub stop_price: Option<Decimal>,
    pub time_in_force: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderResponse {
    pub order_id: String,
    pub symbol: String,
    pub side: Side,
    pub status: String,
    pub filled_quantity: Decimal,
    pub remaining_quantity: Decimal,
    pub average_price: Option<Decimal>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClosePositionResponse {
    pub symbol: String,
    pub closed_quantity: Decimal,
    pub average_price: Decimal,
    pub realized_pnl: Decimal,
    pub timestamp: DateTime<Utc>,
}

// ===== UTILITY FUNCTIONS =====

/// Validate order parameters
pub fn validate_order(order: &OrderRequest) -> PointsBotResult<()> {
    // Validate symbol
    if order.symbol.is_empty() {
        return Err(PointsBotError::InvalidParameter("Symbol cannot be empty".to_string()));
    }
    
    // Validate quantity
    if order.quantity <= Decimal::ZERO {
        return Err(PointsBotError::InvalidParameter("Quantity must be greater than zero".to_string()));
    }
    
    // Validate price for limit orders
    match order.order_type {
        OrderType::Limit | OrderType::StopLimit => {
            if order.price.is_none() || order.price.unwrap() <= Decimal::ZERO {
                return Err(PointsBotError::InvalidParameter("Price is required for limit orders".to_string()));
            }
        }
        _ => {}
    }
    
    // Validate stop price for stop orders
    match order.order_type {
        OrderType::Stop | OrderType::StopLimit => {
            if order.stop_price.is_none() || order.stop_price.unwrap() <= Decimal::ZERO {
                return Err(PointsBotError::InvalidParameter("Stop price is required for stop orders".to_string()));
            }
        }
        _ => {}
    }
    
    Ok(())
}

/// Calculate order value
pub fn calculate_order_value(order: &OrderRequest) -> PointsBotResult<Decimal> {
    match order.order_type {
        OrderType::Market => {
            // For market orders, we can't calculate exact value without current price
            // Return quantity as approximation
            Ok(order.quantity)
        }
        OrderType::Limit => {
            let price = order.price.ok_or_else(|| {
                PointsBotError::InvalidParameter("Price required for limit order".to_string())
            })?;
            Ok(order.quantity * price)
        }
        OrderType::Stop => {
            let stop_price = order.stop_price.ok_or_else(|| {
                PointsBotError::InvalidParameter("Stop price required for stop order".to_string())
            })?;
            Ok(order.quantity * stop_price)
        }
        OrderType::StopLimit => {
            let price = order.price.ok_or_else(|| {
                PointsBotError::InvalidParameter("Price required for stop limit order".to_string())
            })?;
            Ok(order.quantity * price)
        }
    }
}

/// Format symbol for exchange-specific requirements
pub fn format_symbol(symbol: &str, exchange_format: &str) -> String {
    match exchange_format {
        "hyperliquid" => {
            // Hyperliquid uses symbols like "BTC", "ETH" (no pairs)
            symbol.split('-').next().unwrap_or(symbol).to_string()
        }
        "extended" => {
            // Extended uses symbols like "BTC-USD", "ETH-USD"
            if symbol.contains('-') {
                symbol.to_string()
            } else {
                format!("{}-USD", symbol)
            }
        }
        _ => symbol.to_string(),
    }
}

/// Generate unique order ID
pub fn generate_order_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    
    format!("order_{}", timestamp)
} 