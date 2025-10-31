use super::base::{Operator, OrderRequest, OrderResponse};
use crate::{
    ExchangeName, OrderStatus, PointsBotError, PointsBotResult, PositionSide, TickerDirection,
    asset_mapping::AssetMapping,
};
use async_trait::async_trait;
use ethers::signers::LocalWallet;
use hyperliquid_rust_sdk::{
    ClientLimit, ClientOrder, ClientOrderRequest, ExchangeClient, ExchangeDataStatus,
    ExchangeResponseStatus,
};
use log::info;
use rust_decimal::{Decimal, prelude::ToPrimitive};
use std::str::FromStr;

pub struct OperatorHyperliquid {
    client: ExchangeClient,
}

impl OperatorHyperliquid {
    pub async fn new(wallet: LocalWallet) -> Self {
        let client = ExchangeClient::new(None, wallet, None, None, None)
            .await
            .expect("Failed to create ExchangeClient");
        Self { client }
    }
}

pub async fn create_operator_hyperliquid(wallet: LocalWallet) -> Box<dyn Operator> {
    Box::new(OperatorHyperliquid::new(wallet).await)
}

#[async_trait]
impl Operator for OperatorHyperliquid {
    fn get_exchange_info(&self) -> ExchangeName {
        ExchangeName::Hyperliquid
    }

    async fn create_order(&self, mut order: OrderRequest) -> PointsBotResult<OrderResponse> {
        order.symbol = AssetMapping::map_ticker(
            ExchangeName::Hyperliquid,
            &order.symbol,
            TickerDirection::ToExchange,
        ).unwrap_or_else(|| order.symbol.clone());

        let sdk_order = ClientOrderRequest {
            asset: order.symbol.clone(),
            is_buy: matches!(order.side, PositionSide::Long),
            limit_px: order.price.unwrap_or(Decimal::ZERO).to_f64().unwrap_or(0.0),
            sz: order.quantity.to_f64().unwrap_or(0.0),
            reduce_only: order.reduce_only.unwrap_or(false),
            order_type: ClientOrder::Limit(ClientLimit { tif: "Gtc".to_string() }),
            cloid: None,
        };

        let sdk_result = self.client.bulk_order(vec![sdk_order], Some(&self.client.wallet)).await;
        let response = match sdk_result {
            Ok(resp) => resp,
            Err(e) => return Err(PointsBotError::Exchange {
                code: "500".to_string(),
                message: format!("SDK bulk_order error: {e}"),
            }),
        };

        match response {
            ExchangeResponseStatus::Ok(exchange_response) => {
                let data_statuses = exchange_response.data.ok_or_else(|| PointsBotError::Exchange {
                    code: "500".to_string(),
                    message: "No order data in response".to_string(),
                })?;

                info!(target: "hyperliquid", "Order response: {:?}", data_statuses);
                for status in data_statuses.statuses {
                    match status {
                        ExchangeDataStatus::Resting(resting_order) => {
                            return Ok(OrderResponse {
                                id: order.id,
                                exchange_id: resting_order.oid.to_string(),
                                symbol: order.symbol.clone(),
                                side: order.side,
                                status: OrderStatus::Resting,
                                filled_quantity: Decimal::ZERO,
                                remaining_quantity: order.quantity,
                                average_price: None,
                                timestamp: chrono::Utc::now(),
                            });
                        }
                        ExchangeDataStatus::Filled(filled_order) => {
                            return Ok(OrderResponse {
                                id: order.id,
                                exchange_id: filled_order.oid.to_string(),
                                symbol: order.symbol.clone(),
                                side: order.side,
                                status: OrderStatus::Filled,
                                filled_quantity: Decimal::from_str(&filled_order.total_sz).unwrap_or(order.quantity),
                                remaining_quantity: Decimal::ZERO,
                                average_price: Decimal::from_str(&filled_order.avg_px).ok(),
                                timestamp: chrono::Utc::now(),
                            });
                        }
                        _ => {
                            info!("[DEBUG] Order status: {:?}", status);
                        }
                    }
                }
                Err(PointsBotError::Exchange {
                    code: "500".to_string(),
                    message: "No resting order found in response".to_string(),
                })
            }
            ExchangeResponseStatus::Err(err_message) => Err(PointsBotError::Exchange {
                code: "500".to_string(),
                message: format!("SDK bulk_order error: {err_message}"),
            }),
        }
    }

    async fn change_leverage(&self, mut symbol: String, leverage: Decimal) -> PointsBotResult<()> {
        symbol = AssetMapping::map_ticker(
            ExchangeName::Hyperliquid,
            &symbol,
            TickerDirection::ToExchange,
        ).unwrap_or_else(|| symbol.clone());

        let sdk_result = self.client.update_leverage(
            leverage.to_u32().unwrap_or(0),
            &symbol,
            false,
            Some(&self.client.wallet),
        ).await;

        let response = match sdk_result {
            Ok(resp) => resp,
            Err(e) => return Err(PointsBotError::Exchange {
                code: "500".to_string(),
                message: format!("Failed to update leverage: {e}"),
            }),
        };

        match response {
            ExchangeResponseStatus::Ok(_) => Ok(()),
            ExchangeResponseStatus::Err(err_message) => Err(PointsBotError::Exchange {
                code: "500".to_string(),
                message: format!("Failed to update leverage: {err_message}"),
            }),
        }
    }
}
