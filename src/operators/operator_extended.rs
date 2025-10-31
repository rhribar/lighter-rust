use std::str::FromStr;

use crate::{
    AssetMapping, ChangeLeverageRequest, ExchangeName, OrderStatus, PointsBotError,
    PointsBotResult, PositionSide, TickerDirection,
    operators::{
        Operator, OrderRequest, OrderResponse,
        init_extended_markets::{
            Side, extended_markets, hex_to_felt, init_extended_markets, sign_limit_ioc,
        },
    },
};
use async_trait::async_trait;
use chrono::{Duration, Utc};
use rust_decimal::Decimal;
use serde_json::json;

pub struct OperatorExtended {
    client: crate::operators::base::HttpClient,
    api_key: Option<String>,
    stark_private_key: Option<String>,
    vault_id: Option<u64>,
}

impl OperatorExtended {
    pub async fn new() -> Self {
        let _ = init_extended_markets().await;
        let client = crate::operators::base::HttpClient::new(
            "https://api.starknet.extended.exchange/api/v1".to_string(),
            Some(1000),
        );
        Self {
            client,
            api_key: std::env::var("EXTENDED_API_KEY").ok(),
            stark_private_key: std::env::var("EXTENDED_STARK_PRIVATE_KEY").ok(),
            vault_id: std::env::var("EXTENDED_VAULT_KEY")
                .ok()
                .and_then(|v| v.parse().ok()),
        }
    }

    fn is_configured(&self) -> bool {
        self.api_key.is_some() && self.stark_private_key.is_some() && self.vault_id.is_some()
    }
}

#[async_trait]
impl Operator for OperatorExtended {
    fn get_exchange_info(&self) -> ExchangeName {
        ExchangeName::Extended
    }

    async fn create_order(&self, mut order: OrderRequest) -> PointsBotResult<OrderResponse> {
        if !self.is_configured() {
            return Err(PointsBotError::Auth {
                msg: "Operator requires API key, STARK private key, and vault ID".to_string(),
                source: None,
            });
        }
        let _ = init_extended_markets();
        order.symbol = AssetMapping::map_ticker(
            ExchangeName::Extended,
            &order.symbol,
            TickerDirection::ToExchange,
        ).unwrap_or_else(|| order.symbol.clone());
        let market_config = extended_markets()
            .get(&order.symbol)
            .ok_or_else(|| PointsBotError::InvalidParameter {
                msg: format!("Market {} not found", order.symbol),
            })?;

        let side = match order.side {
            PositionSide::Long => Side::Buy,
            PositionSide::Short => Side::Sell,
        };
        let qty_synthetic = order.quantity;
        let limit_price = order.price;
        let fee_rate = Decimal::from_str("0.0005").unwrap();
        let vault_id = self.vault_id.unwrap();
        let stark_priv = hex_to_felt(self.stark_private_key.as_ref().unwrap());

        let expiry_ts_ms = Utc::now() + Duration::hours(8);
        let correct_expiry_hours = ((expiry_ts_ms.timestamp_millis() as f64) / 1000.0 / 3600.0).ceil();
        let hours_in_14_days = 14.0 * 24.0;
        let correct_expiry_hours_plus_14_days = correct_expiry_hours + hours_in_14_days;

        let user_public_key_hex = format!("{:x}", starknet_crypto::get_public_key(&stark_priv));
        let signature = sign_limit_ioc(
            market_config,
            side,
            qty_synthetic,
            limit_price.unwrap_or(Decimal::ZERO),
            fee_rate,
            vault_id,
            stark_priv,
            user_public_key_hex,
            Some((correct_expiry_hours_plus_14_days * 3600.0).round() as i64),
            None,
            order.reduce_only.unwrap_or(false),
            None, None, None, None,
        ).map_err(|e| PointsBotError::Unknown {
            msg: format!("Signing error: {e}"),
            source: Some(e.into()),
        })?;

        let api_key = self.api_key.as_ref().ok_or_else(|| PointsBotError::Auth {
            msg: "EXTENDED_API_KEY not set".to_string(),
            source: None,
        })?;
        let stark_public_key = std::env::var("EXTENDED_STARK_PUBLIC_KEY").map_err(|e| PointsBotError::Auth {
            msg: "EXTENDED_STARK_PUBLIC_KEY not found in environment".to_string(),
            source: Some(Box::new(e)),
        })?;

        let order_payload = json!({
            "id": signature.order_hash.to_string(),
            "market": order.symbol,
            "type": order.order_type.as_str().to_uppercase(),
            "side": side.as_str().to_uppercase(),
            "qty": qty_synthetic.to_string(),
            "price": limit_price.unwrap_or(Decimal::ZERO).to_string(),
            "timeInForce": "GTT",
            "expiryEpochMillis": (correct_expiry_hours * 1000.0 * 3600.0).round() as i64,
            "fee": "0.0005",
            "nonce": signature.nonce.to_string(),
            "settlement": {
                "signature": {
                    "r": format!("0x{:x}", signature.r),
                    "s": format!("0x{:x}", signature.s)
                },
                "starkKey": stark_public_key,
                "collateralPosition": vault_id.to_string()
            },
            "selfTradeProtectionLevel": "ACCOUNT",
            "reduceOnly": order.reduce_only.unwrap_or(false),
            "postOnly": false,
        });
        let mut headers = std::collections::HashMap::new();
        headers.insert("X-Api-Key".to_string(), api_key.clone());
        headers.insert("User-Agent".to_string(), "bot-rs/1.0".to_string());
        headers.insert("Content-Type".to_string(), "application/json".to_string());

        let response = self.client.post("/user/order", &order_payload.to_string(), Some(headers)).await;
        match response {
            Ok(resp) => {
                let response_text = resp.text().await.map_err(|e| PointsBotError::Unknown {
                    msg: format!("Response error: {e}"),
                    source: Some(Box::new(e)),
                })?;
                let json_response: serde_json::Value = serde_json::from_str(&response_text)
                    .map_err(|e| PointsBotError::Parse {
                        msg: format!("Failed to parse JSON: {e}"),
                        source: Some(Box::new(e)),
                    })?;
                if json_response["status"] == "OK" {
                    return Ok(OrderResponse {
                        id: order.id,
                        exchange_id: json_response["id"].as_str().unwrap_or_default().to_string(),
                        symbol: order.symbol.clone(),
                        side: order.side.clone(),
                        status: OrderStatus::Resting,
                        filled_quantity: Decimal::ZERO,
                        remaining_quantity: order.quantity,
                        average_price: None,
                        timestamp: Utc::now(),
                    });
                }
                Err(PointsBotError::Exchange {
                    code: "500".to_string(),
                    message: format!("Failed to create order: {}", response_text),
                })
            }
            Err(e) => Err(e),
        }
    }

    async fn change_leverage(&self, symbol: String, leverage: Decimal) -> PointsBotResult<()> {
        let api_key = self.api_key.as_ref().ok_or_else(|| PointsBotError::Auth {
            msg: "EXTENDED_API_KEY not set".to_string(),
            source: None,
        })?;
        let body = serde_json::to_string(&ChangeLeverageRequest {
            market: AssetMapping::map_ticker(
                ExchangeName::Extended,
                &symbol,
                TickerDirection::ToExchange,
            ).unwrap_or(symbol),
            leverage: leverage.to_string(),
        }).map_err(|e| PointsBotError::Parse {
            msg: format!("Failed to serialize payload: {e}"),
            source: Some(Box::new(e)),
        })?;
        let headers = [
            ("X-Api-Key".to_string(), api_key.clone()),
            ("User-Agent".to_string(), "bot-rs/1.0".to_string()),
            ("Content-Type".to_string(), "application/json".to_string()),
        ].iter().cloned().collect();

        let response_text = self.client.patch("/user/leverage", &body, Some(headers)).await
            .map_err(|e| PointsBotError::Unknown {
                msg: format!("HTTP error: {e}"),
                source: Some(Box::new(e)),
            })?
            .text().await.map_err(|e| PointsBotError::Unknown {
                msg: format!("Response error: {e}"),
                source: Some(Box::new(e)),
            })?;

        match serde_json::from_str::<serde_json::Value>(&response_text)
            .map_err(|e| PointsBotError::Parse {
                msg: format!("Failed to parse JSON: {e}"),
                source: Some(Box::new(e)),
            })?
            ["status"].as_str() {
            Some("OK") => Ok(()),
            _ => Err(PointsBotError::Exchange {
                code: "500".to_string(),
                message: "Failed to update leverage".to_string(),
            }),
        }
    }
}
