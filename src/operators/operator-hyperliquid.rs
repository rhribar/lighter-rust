/// Async factory for OperatorHyperliquid
pub async fn create_operator_hyperliquid(wallet: LocalWallet) -> Box<dyn Operator> {
    Box::new(OperatorHyperliquid::new(wallet).await)
}
// Only import the SDK and the Operator trait
use async_trait::async_trait;
use crate::{PointsBotError, PointsBotResult, ExchangeName};
use super::base::{Operator, OrderRequest, OrderResponse};
use hyperliquid_rust_sdk::{ExchangeClient, ClientOrderRequest, ClientOrder, ClientLimit};
use ethers::signers::LocalWallet;
use rust_decimal::Decimal;
use rust_decimal::prelude::{ToPrimitive, FromPrimitive};
use crate::asset_mapping::AssetMapping;

pub struct OperatorHyperliquid {
    client: ExchangeClient,
}

impl OperatorHyperliquid {
    pub async fn new(wallet: LocalWallet) -> Self {
        // You may want to pass other args (Meta, vault_address, etc.)
        let client = ExchangeClient::new(
            None, // Option<reqwest::Client>
            wallet,
            None, // Option<BaseUrl>
            None, // Option<Meta>
            None, // Option<H160>
        ).await.expect("Failed to create ExchangeClient");
        Self { client }
    }
}

#[async_trait]
impl Operator for OperatorHyperliquid {
    async fn create_order(&self, mut order: OrderRequest) -> PointsBotResult<OrderResponse> {
        order.symbol = AssetMapping::get_exchange_ticker(ExchangeName::Hyperliquid, &order.symbol)
            .unwrap_or_else(|| order.symbol.clone());

        let is_buy = matches!(order.side, crate::Side::Buy);
        let price = order.price.unwrap_or(Decimal::ZERO).to_f64().unwrap_or(0.0);
        let quantity = order.quantity.to_f64().unwrap_or(0.0);
        let sdk_order = ClientOrderRequest {
            asset: order.symbol.clone(),
            is_buy,
            limit_px: price,
            sz: quantity,
            reduce_only: false,
            order_type: ClientOrder::Limit(ClientLimit { tif: "Gtc".to_string() }),
            cloid: None,
        };
        // Call the SDK bulk order method
        let sdk_result = self.client.bulk_order(vec![sdk_order], Some(&self.client.wallet)).await;
        match sdk_result {
            Ok(response) => {
                println!("[DEBUG] SDK bulk_order response: {:?}", response);
                // You may need to parse response to get order_id, status, etc.
                // For now, just return a basic response
                Ok(OrderResponse {
                    order_id: "sdk_id".to_string(), // TODO: parse from response
                    symbol: order.symbol.clone(),
                    side: order.side,
                    status: "ok".to_string(),
                    filled_quantity: order.quantity,
                    remaining_quantity: Decimal::ZERO,
                    average_price: Some(Decimal::from_f64(price).unwrap_or(Decimal::ZERO)),
                    timestamp: chrono::Utc::now(),
                })
            }
            Err(e) => {
                println!("[ERROR] SDK bulk_order failed: {:?}", e);
                Err(PointsBotError::Exchange { code: 500.to_string(), message: format!("SDK bulk_order error: {e}") })
            }
        }
    }

    async fn close_position(&self, _symbol: &str) -> PointsBotResult<super::base::ClosePositionResponse> {
        Err(PointsBotError::Unknown("close_position not supported in OperatorHyperliquid".to_string()))
    }

    fn exchange_name(&self) -> ExchangeName {
        ExchangeName::Hyperliquid
    }
}