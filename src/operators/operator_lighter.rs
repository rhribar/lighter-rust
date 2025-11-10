use crate::{
    operators::{Operator, OrderRequest, OrderResponse},
    AssetMapping, ExchangeName, OrderStatus, PointsBotError, PointsBotResult, PositionSide, TickerDirection,
};
use api_client::{CreateOrderRequest, LighterClient};
use async_trait::async_trait;
use log::info;
use rust_decimal::{prelude::ToPrimitive, Decimal};
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;

pub struct OperatorLighter {
    client: LighterClient,
    account_index: i64,
}

impl OperatorLighter {
    pub async fn new() -> Self {
        let lighter_api_key = std::env::var("LIGHTER_API_KEY").ok();
        let account_index = std::env::var("LIGHTER_ACCOUNT_INDEX").ok().and_then(|v| v.parse::<i64>().ok());
        let api_key_index = std::env::var("LIGHTER_API_KEY_INDEX").ok().and_then(|v| v.parse::<u8>().ok());

        let client = LighterClient::new(
            "https://mainnet.zklighter.elliot.ai/api/v1/".to_string(),
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
        // Map OrderRequest to CreateOrderRequest
        let base_amount = (order.quantity * Decimal::from(1_000_00)).to_i64().unwrap_or(0);
        let create_order_req = CreateOrderRequest {
            account_index: self.account_index,
            order_book_index: 1,
            client_order_index: 123,
            base_amount,
            price: order.price.unwrap_or(Decimal::ZERO).to_i64().unwrap_or(0),
            is_ask: matches!(order.side, PositionSide::Short),
            order_type: 0,    // 0 = LIMIT
            time_in_force: 1, // 1 = GOOD_TILL_TIME
            reduce_only: order.reduce_only.unwrap_or(false),
            trigger_price: 0,
        };

        info!("Initial order: {:?}", order);

        info!("Creating Lighter order: {:?}", create_order_req);

        let sdk_result = match self.client.create_order(create_order_req).await {
            Ok(res) => res,
            Err(e) => {
                // Print the full error for debugging
                println!("Full SDK error: {:?}", e);
                return Err(PointsBotError::Unknown {
                    msg: format!("SDK create_order error: {e}"),
                    source: None,
                });
            }
        };

        let order_response = OrderResponse {
            id: order.id.clone(),
            status: OrderStatus::Resting,
            exchange_id: sdk_result["tx_hash"].as_str().unwrap_or_default().to_string(),
            symbol: order.symbol.clone(),
            side: order.side,
            filled_quantity: Decimal::ZERO, // Could parse from sdk_result if available
            remaining_quantity: order.quantity,
            average_price: order.price,
            timestamp: chrono::Utc::now(),
        };
        Ok(order_response)
    }

    async fn change_leverage(&self, symbol: String, leverage: Decimal) -> PointsBotResult<()> {
        // Map symbol to order_book_index
        /* let order_book_index = AssetMapping::map_ticker(ExchangeName::Lighter, &symbol, TickerDirection::ToExchange)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        // Call the SDK function to change leverage
        let sdk_result = self.client.change_leverage(order_book_index, leverage.to_f64().unwrap_or(1.0)).await.map_err(|e| PointsBotError::Unknown {
            msg: format!("SDK change_leverage error: {e}"),
            source: None,
        })?; */

        // Optionally check sdk_result for success
        Ok(())
    }
}


fn uuid_to_u64(uuid: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    uuid.hash(&mut hasher);
    hasher.finish()
}