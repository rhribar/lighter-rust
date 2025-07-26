use async_trait::async_trait;
use crate::{ExchangeName, OrderStatus, PointsBotError, PointsBotResult, PositionSide};
use super::base::{Operator, OrderRequest, OrderResponse};
use hyperliquid_rust_sdk::{ClientLimit, ClientOrder, ClientOrderRequest, ExchangeClient, ExchangeDataStatus, ExchangeResponseStatus};
use ethers::signers::LocalWallet;
use rust_decimal::Decimal;
use rust_decimal::prelude::{ToPrimitive};
use crate::asset_mapping::AssetMapping;

pub struct OperatorHyperliquid {
    client: ExchangeClient,
}

impl OperatorHyperliquid {
    pub async fn new(wallet: LocalWallet) -> Self {
        let client = ExchangeClient::new(
            None,
            wallet,
            None,
            None,
            None,
        ).await.expect("Failed to create ExchangeClient");
        Self { client }
    }
}

pub async fn create_operator_hyperliquid(wallet: LocalWallet) -> Box<dyn Operator> {
    Box::new(OperatorHyperliquid::new(wallet).await)
}

#[async_trait]
impl Operator for OperatorHyperliquid {    
    fn get_exchange_name(&self) -> ExchangeName {
        ExchangeName::Hyperliquid
    }

    async fn create_order(&self, mut order: OrderRequest) -> PointsBotResult<OrderResponse> {
        order.symbol = AssetMapping::get_exchange_ticker(ExchangeName::Hyperliquid, &order.symbol)
            .unwrap_or_else(|| order.symbol.clone());

        let is_buy = matches!(order.side, PositionSide::Long);
        let price = order.price.unwrap_or(Decimal::ZERO);
        let quantity = order.quantity;
        let sdk_order = ClientOrderRequest {
            asset: order.symbol.clone(),
            is_buy,
            limit_px: price.to_f64().unwrap_or(0.0),
            sz: quantity.to_f64().unwrap_or(0.0),
            reduce_only: order.reduce_only.unwrap_or(false),
            order_type: ClientOrder::Limit(ClientLimit { tif: "Gtc".to_string() }),
            cloid: None,
        };
        // Call the SDK bulk order method
        let sdk_result = self.client.bulk_order(vec![sdk_order], Some(&self.client.wallet)).await;
        match sdk_result {
            Ok(response) => {
                println!("[DEBUG] SDK bulk_order response: {:?}", response);
                match response {
                    ExchangeResponseStatus::Ok(exchange_response) => {
                        if let Some(data_statuses) = exchange_response.data {
                            for status in data_statuses.statuses {
                                match status {
                                    ExchangeDataStatus::Resting(resting_order) => {
                                        return Ok(OrderResponse {
                                            order_id: resting_order.oid.to_string(),
                                            symbol: order.symbol.clone(),
                                            side: order.side,
                                            status: OrderStatus::Resting,
                                            filled_quantity: Decimal::ZERO,
                                            remaining_quantity: order.quantity,
                                            average_price: None,
                                            timestamp: chrono::Utc::now(),
                                        });
                                    }
                                    _ => {
                                        print!("[DEBUG] Order status: {:?}", status);
                                    },
                                }
                            }
                        }
                        Err(PointsBotError::Exchange { code: 500.to_string(), message: "No waiting for fill order found in response".to_string() })
                    }
                    ExchangeResponseStatus::Err(err_message) => {
                        Err(PointsBotError::Exchange { code: 500.to_string(), message: format!("SDK bulk_order error: {err_message}") })
                    }
                }
            }
            Err(e) => {
                println!("[ERROR] SDK bulk_order failed: {:?}", e);
                Err(PointsBotError::Exchange { code: 500.to_string(), message: format!("SDK bulk_order error: {e}") })
            }
        }
    }

    async fn change_leverage(&self, mut symbol: String, leverage: Decimal) -> PointsBotResult<()> {
        symbol = AssetMapping::get_exchange_ticker(ExchangeName::Extended, &symbol).unwrap_or_else(|| symbol.clone());

        match self.client.update_leverage(
            leverage.to_u32().unwrap_or(0),
            &AssetMapping::get_exchange_ticker(ExchangeName::Extended, &symbol).unwrap_or_else(|| symbol.clone()),
            false,
            Some(&self.client.wallet)
        ).await {
            Ok(response) => {
                match response {
                    ExchangeResponseStatus::Ok(_) => {
                        Ok(())
                    }
                    ExchangeResponseStatus::Err(err_message) => {
                        Err(PointsBotError::Exchange {
                            code: "500".to_string(),
                            message: format!("Failed to update leverage: {err_message}"),
                        })
                    }
                }
            }
            Err(e) => {
                Err(PointsBotError::Exchange {
                    code: "500".to_string(),
                    message: format!("Failed to update leverage: {e}"),
                })
            }
        }
    }
}