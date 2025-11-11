use crate::{
    fetchers::MarketInfo,
    operators::{Operator, OrderRequest, OrderResponse},
    AssetMapping, ExchangeName, OrderStatus, PointsBotError, PointsBotResult, PositionSide, TickerDirection,
};
use api_client::{CreateOrderRequest, LighterClient};
use async_trait::async_trait;
use log::info;
use rust_decimal::{prelude::ToPrimitive, Decimal, MathematicalOps};

pub struct OperatorLighter {
    client: LighterClient,
    account_index: i64,
}

impl OperatorLighter {
    pub async fn new() -> Self {
        let lighter_api_key = std::env::var("LIGHTER_API_KEY").ok();
        let account_index = std::env::var("LIGHTER_ACCOUNT_INDEX")
            .ok()
            .and_then(|v| v.parse::<i64>().ok());
        let api_key_index = std::env::var("LIGHTER_API_KEY_INDEX")
            .ok()
            .and_then(|v| v.parse::<u8>().ok());

        let client = LighterClient::new(
            "https://mainnet.zklighter.elliot.ai".to_string(),
            &lighter_api_key.unwrap(),
            account_index.unwrap(),
            api_key_index.unwrap(),
        )
        .expect("Failed to create LighterClient");

        Self {
            client,
            account_index: account_index.unwrap(),
        }
    }
}

#[async_trait]
impl Operator for OperatorLighter {
    fn get_exchange_info(&self) -> ExchangeName {
        ExchangeName::Lighter
    }

    async fn create_order(&self, order: OrderRequest) -> PointsBotResult<OrderResponse> {
        info!("📝 Preparing to create order on Lighter... {:?}", order);
        let mut last_err: Option<PointsBotError> = None;
        for attempt in 1..=20 {
            match self.submit_lighter_order(&order).await {
                Ok(resp) => return Ok(resp),
                Err(e) => {
                    let msg = format!("{}", e);
                    if msg.contains("invalid signature") {
                        info!("Attempt {}: Invalid signature, retrying...", attempt);
                        last_err = Some(e);
                        continue;
                    } else {
                        return Err(e);
                    }
                }
            }
        }
        Err(last_err.unwrap_or_else(|| PointsBotError::Unknown {
            msg: "Order submission failed after retries".to_string(),
            source: None,
        }))
    }

    async fn change_leverage(&self, market: MarketInfo, leverage: Decimal) -> PointsBotResult<()> {
        let lev = leverage.to_u16().unwrap_or(1);

        let exchange_id_u8 = market.exchange_id.unwrap_or(0) as u8;
        let res = self.client.update_leverage(exchange_id_u8, lev, 0).await;
        match res {
            Ok(val) => {
                info!("Leverage updated: {}", val);
                Ok(())
            }
            Err(e) => Err(PointsBotError::Unknown {
                msg: format!("SDK update_leverage error: {e}"),
                source: None,
            }),
        }
    }
}

// Helper function for order submission with error handling
impl OperatorLighter {
    async fn submit_lighter_order(&self, order: &OrderRequest) -> PointsBotResult<OrderResponse> {
        let bytes = order.id.as_bytes();
        let client_order_index = format!(
            "{}{}",
            u64::from_be_bytes(bytes[0..8].try_into().unwrap()),
            u64::from_be_bytes(bytes[8..16].try_into().unwrap())
        );

        let create_order = CreateOrderRequest {
            account_index: self.account_index,
            order_book_index: order.market.exchange_id.unwrap_or(0) as u8,
            client_order_index: client_order_index.parse().unwrap_or(0),
            base_amount: (order.quantity * Decimal::from(10).powd(order.market.sz_decimals))
                .to_i64()
                .unwrap_or(0),
            price: (order.price.unwrap_or(Decimal::ZERO) * Decimal::from(10).powd(order.market.px_decimals))
                .to_i64()
                .unwrap_or(0),
            is_ask: matches!(order.side, PositionSide::Short),
            order_type: 0,    // 0 = LIMIT
            time_in_force: 1, // 1 = GOOD_TILL_TIME
            reduce_only: order.reduce_only.unwrap_or(false),
            trigger_price: 0,
        };

        info!("Creating Lighter order: {:?}", create_order);

        let response = match self.client.create_order(create_order).await {
            Ok(res) => res,
            Err(e) => {
                println!("Full SDK error: {:?}", e);
                return Err(PointsBotError::Unknown {
                    msg: format!("SDK create_order error: {e}"),
                    source: None,
                });
            }
        };

        info!("✅ Limit order submitted!");
        info!("📥 Response:");
        info!("{}", serde_json::to_string_pretty(&response)?);

        let code = response["code"].as_i64().unwrap_or_default();
        let tx_hash = response["tx_hash"].as_str().unwrap_or("").to_string();
        let message = response["message"].as_str().unwrap_or("").to_string();

        if code != 200 {
            info!("\n⚠️  Order submission returned code: {}", code);
            if !message.is_empty() {
                info!("  Message: {}", message);
            }
            return Err(PointsBotError::Unknown {
                msg: format!("Order submission failed: code {} - {}", code, message),
                source: None,
            });
        }

        let order_response = OrderResponse {
            id: order.id.clone(),
            status: OrderStatus::Resting,
            exchange_id: tx_hash,
            symbol: order.market.symbol.clone(),
            side: order.side,
            filled_quantity: Decimal::ZERO,
            remaining_quantity: order.quantity,
            average_price: order.price,
            timestamp: chrono::Utc::now(),
        };
        Ok(order_response)
    }
}
