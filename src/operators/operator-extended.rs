use std::str::FromStr;


use async_trait::async_trait;
use rust_decimal::Decimal;
use crate::operators::{validate_order, Operator, OrderRequest, OrderResponse, ClosePositionResponse};
use crate::{current_timestamp, AssetMapping, ExchangeName, PointsBotError, PointsBotResult};
use crate::operators::init_extended_markets::{init_extended_markets, extended_markets, sign_limit_ioc, Side, hex_to_felt};
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
            "https://api.extended.exchange/api/v1".to_string(),
            Some(1000),
        );
        Self {
            client,
            api_key: std::env::var("EXTENDED_API_KEY").ok(),
            stark_private_key: std::env::var("EXTENDED_STARK_KEY").ok(),
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
    async fn create_order(&self, mut order: OrderRequest) -> PointsBotResult<OrderResponse> {
        use serde_json::json;
        use chrono::{Utc, Duration};
        validate_order(&order)?;
        if !self.is_configured() {
            return Err(PointsBotError::Auth("Operator requires API key, STARK private key, and vault ID".to_string()));
        }
        let _ = init_extended_markets();
        order.symbol = AssetMapping::get_exchange_ticker(ExchangeName::Extended, &order.symbol)
            .unwrap_or_else(|| order.symbol.clone());
        let markets = extended_markets();
        let market_config = markets.get(&order.symbol)
            .ok_or_else(|| PointsBotError::InvalidParameter(format!("Market {} not found", order.symbol)))?;

        let side = match order.side.as_str().to_uppercase().as_str() {
            "BUY" => Side::Buy,
            "SELL" => Side::Sell,
            _ => return Err(PointsBotError::InvalidParameter("Invalid side".to_string())),
        };
        let qty_synthetic = order.quantity;
        let limit_price = order.price;
        let fee_rate = Decimal::from_str("0.0005").unwrap();
        let vault_id = self.vault_id.unwrap();
        let stark_priv_hex = self.stark_private_key.as_ref().unwrap();
        let stark_priv = hex_to_felt(stark_priv_hex);
        let expiry_ts_ms = Utc::now() + Duration::hours(8);
        let sig_expiry_ts_ms = expiry_ts_ms + Duration::days(14);
        let signature = sign_limit_ioc(
            market_config,
            side,
            qty_synthetic,
            limit_price.unwrap_or(Decimal::ZERO),
            fee_rate,
            vault_id,
            stark_priv,
            Some(sig_expiry_ts_ms.timestamp_millis()),
            None,
            false,
        ).map_err(|e| PointsBotError::Unknown(format!("Signing error: {}", e)))?;
        let api_key = self.api_key.as_ref().ok_or_else(|| PointsBotError::Auth("EXTENDED_API_KEY not set".to_string()))?;
        // For now, get public key from env (could be added to struct if needed)
        let stark_public_key = std::env::var("EXTENDED_STARK_PUBLIC_KEY").map_err(|_| PointsBotError::Auth("EXTENDED_STARK_PUBLIC_KEY not found in environment".to_string()))?;
        let order_payload = json!({
            "id": signature.order_hash.to_string(),
            "market": order.symbol,
            "type": "LIMIT",
            "side": order.side.as_str().to_uppercase(),
            "qty": qty_synthetic.to_string(),
            "price": limit_price.unwrap_or(Decimal::ZERO).to_string(),
            "timeInForce": "GTT",
            "expiryEpochMillis": expiry_ts_ms.timestamp_millis(),
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
            "reduceOnly": false,
            "postOnly": false,
            "debuggingAmounts": {
                "collateralAmount": "10000000",
                "feeAmount": "2500",
                "syntheticAmount": "100"
            }
        });
        let url = "/user/order";
        let mut headers = std::collections::HashMap::new();
        headers.insert("X-Api-Key".to_string(), api_key.clone());
        headers.insert("User-Agent".to_string(), "points-bot-rs/1.0".to_string());
        headers.insert("Content-Type".to_string(), "application/json".to_string());
        let response = self.client.post(url, &order_payload.to_string(), Some(headers)).await.map_err(|e| PointsBotError::Unknown(format!("HTTP error: {}", e)))?;
        let status = response.status();
        let text = response.text().await.map_err(|e| PointsBotError::Unknown(format!("Response error: {}", e)))?;
        let order_id = if let Ok(json_response) = serde_json::from_str::<serde_json::Value>(&text) {
            json_response.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string()
        } else {
            "".to_string()
        };
        Ok(OrderResponse {
            order_id,
            symbol: order.symbol.clone(),
            side: order.side.clone(),
            status: "SUBMITTED".to_string(),
            filled_quantity: Decimal::ZERO,
            remaining_quantity: order.quantity,
            average_price: None,
            timestamp: current_timestamp(),
        })
    }

    async fn close_position(&self, _symbol: &str) -> PointsBotResult<ClosePositionResponse> {
        Err(PointsBotError::Unknown("close_position not implemented".to_string()))
    }

    fn exchange_name(&self) -> ExchangeName {
        ExchangeName::Extended
    }
}