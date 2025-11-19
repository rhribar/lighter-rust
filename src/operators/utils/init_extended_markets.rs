use std::collections::HashMap;

use crate::operators::utils::extended_signature::{get_order_hash, sign_message};
use anyhow::{anyhow, Result};
use log::info;
use once_cell::sync::OnceCell;
use rand::{thread_rng, Rng};
use reqwest::Client;
use rust_decimal::prelude::*; // precise decimals
use rust_decimal::Decimal;
use serde::Deserialize;
use starknet_crypto::Felt;

/// Global, lazily-initialised store for every market.
static EXTENDED_MARKETS: OnceCell<HashMap<String, MarketConfig>> = OnceCell::new();

pub use starknet_crypto;

/// Shape of `GET /info/markets` (only fields we use).
#[derive(Deserialize)]
struct MarketsResponse {
    data: Vec<MarketInfo>,
}

#[derive(Deserialize)]
struct MarketInfo {
    name: String,
    #[serde(rename = "tradingConfig")]
    trading_config: TradingCfg,
    #[serde(rename = "l2Config")]
    l2_config: L2Cfg,
}

#[derive(Deserialize)]
struct TradingCfg {
    #[serde(rename = "minOrderSize")]
    min_order_size: String,
    #[serde(rename = "maxLimitOrderValue")]
    max_limit_order_value: String,
}

#[derive(Deserialize)]
struct L2Cfg {
    #[serde(rename = "syntheticId")]
    synthetic_id: String,
    #[serde(rename = "collateralId")]
    collateral_id: String,
    #[serde(rename = "syntheticResolution")]
    synthetic_resolution: u64,
    #[serde(rename = "collateralResolution")]
    collateral_resolution: u64,
}

/// Call once during startup to populate [`EXTENDED_MARKETS`].
pub async fn init_extended_markets() -> Result<()> {
    let client = Client::new();
    let resp = client
        .get("https://api.starknet.extended.exchange/api/v1/info/markets")
        .header("User-Agent", "rust-client")
        .send()
        .await?
        .error_for_status()?
        .json::<MarketsResponse>()
        .await?;

    let mut map = HashMap::with_capacity(resp.data.len());

    for m in resp.data {
        let cfg = MarketConfig {
            synthetic_id: Felt::from_hex(&m.l2_config.synthetic_id)?,
            collateral_id: Felt::from_hex(&m.l2_config.collateral_id)?,
            synthetic_resolution: m.l2_config.synthetic_resolution,
            collateral_resolution: m.l2_config.collateral_resolution,
            min_qty_synthetic: m.trading_config.min_order_size.parse::<Decimal>()?,
            max_limit_value: m.trading_config.max_limit_order_value.parse::<Decimal>()?,
        };
        map.insert(m.name, cfg);
    }

    EXTENDED_MARKETS
        .set(map)
        .map_err(|_| anyhow!("extended markets already initialised"))
}

/// Immutable handle to the loaded market map.
pub fn extended_markets() -> &'static HashMap<String, MarketConfig> {
    EXTENDED_MARKETS.get().expect("call init_extended_markets() first")
}

/// All Extended-specific constants that influence hashing & conversions.
#[derive(Clone, Debug)]
pub struct MarketConfig {
    pub synthetic_id: Felt,
    pub collateral_id: Felt,
    pub synthetic_resolution: u64,
    pub collateral_resolution: u64,
    pub min_qty_synthetic: Decimal,
    pub max_limit_value: Decimal,
}

/// BUY or SELL side.
#[derive(Clone, Copy, Debug)]
pub enum Side {
    Buy,
    Sell,
}

impl Side {
    pub fn as_str(&self) -> &'static str {
        match self {
            Side::Buy => "BUY",
            Side::Sell => "SELL",
        }
    }
}

/// Result returned by `sign_limit_ioc()`.
#[derive(Debug)]
pub struct Signature {
    pub order_hash: Felt,
    pub r: Felt,
    pub s: Felt,
    pub nonce: u32,
}

/// Sign a **limit-IOC** order and return `(hash, r, s, nonce)`.
///
/// * `cfg` – market/global constants.
/// * `side` – Buy or Sell.
/// * `qty_synthetic` – human units (e.g. BTC).
/// * `limit_price` – human quote (e.g. USDC per BTC).
/// * `fee_rate` – 0.0005 = 0.05 %.
/// * `vault_id` – user collateral position.
/// * `stark_priv` – Stark private key.
/// * `expiry_ts_ms` – *optional* Unix‐epoch expiration time in
///   **milliseconds**.   If `None`, the default "now + 8 h + 14 d" window is
///   used.
pub fn sign_limit_ioc(
    cfg: &MarketConfig,
    side: Side,
    qty_synthetic: Decimal,
    limit_price: Decimal,
    fee_rate: Decimal,
    vault_id: u64,
    stark_priv: Felt,
    user_public_key_hex: String,
    expiry_ts_ms: Option<i64>,
    nonce: Option<u32>,
    is_reduce_only: bool,
    domain_name: Option<String>,
    domain_version: Option<String>,
    domain_chain_id: Option<String>,
    domain_revision: Option<String>,
) -> Result<Signature> {
    if qty_synthetic < cfg.min_qty_synthetic && !is_reduce_only {
        return Err(anyhow!(
            "qty below exchange minimum ({} synthetic)",
            cfg.min_qty_synthetic
        ));
    }
    if qty_synthetic * limit_price > cfg.max_limit_value {
        return Err(anyhow!("order value exceeds exchange cap"));
    }

    let is_buy = matches!(side, Side::Buy);

    // ───── conversions ─────
    let qty_internal = {
        let q = qty_synthetic * Decimal::from(cfg.synthetic_resolution);
        if is_buy {
            q.ceil()
        } else {
            q.floor()
        }
    }
    .to_u64()
    .unwrap();

    let collateral_value = qty_synthetic * limit_price;
    let collateral_internal = {
        let c = collateral_value * Decimal::from(cfg.collateral_resolution);
        if is_buy {
            c.ceil()
        } else {
            c.floor()
        }
    }
    .to_u64()
    .unwrap();

    let fee_internal = (Decimal::from(collateral_internal) * fee_rate).ceil().to_u64().unwrap();

    // ───── expiry & nonce ─────
    /* let expiry_hours: u32 = match expiry_ts_ms {
        Some(ts_ms) => {
            // convert millis → seconds → hours
            ((ts_ms as f64) / 1000.0 / 3600.0).ceil() as u32
        }
        None => {
            let expiry = Utc::now() + Duration::hours(8) + Duration::days(14);
            (expiry.timestamp() as f64 / 3600.0).ceil() as u32
        }
    }; */

    let nonce: u32 = match nonce {
        Some(n) => n,
        None => thread_rng().gen_range(0..(1 << 31)),
    };

    // ───── hash (using new signature logic) ─────
    let position_id = vault_id.to_string();
    let base_asset_id_hex = format!("0x{:x}", cfg.synthetic_id);

    let base_amount = if is_buy {
        qty_internal.to_string()
    } else {
        format!("-{}", qty_internal)
    };

    let quote_asset_id_hex = format!("0x{:x}", cfg.collateral_id);
    let quote_amount = if is_buy {
        format!("-{}", collateral_internal)
    } else {
        collateral_internal.to_string()
    };
    let fee_asset_id_hex = format!("0x{:x}", cfg.collateral_id);
    let fee_amount = fee_internal.to_string();
    let expiry = expiry_ts_ms.unwrap().to_string();
    let nonce_str = nonce.to_string();

    /* [DEBUG] get_order_hash args:
    position_id: 109221
    base_asset_id_hex: 0x4254432d3600000000000000000000
    base_amount: 900
    quote_asset_id_hex: 0x1
    quote_amount: 102060000
    fee_asset_id_hex: 0x1
    fee_amount: 51030
    expiry: 489670
    nonce_str: 1603086732
    user_public_key_hex: 0x43af293b0634f06ee1c593f9b9a0a4494c3fa68b5ee49b5e480e9051bb01c6
    domain_name: Perpetuals
    domain_version: v0
    domain_chain_id: SN_MAIN
    domain_revision: 1 */

    let user_public_key_hex = if user_public_key_hex.starts_with("0x") {
        user_public_key_hex
    } else {
        format!("0x{}", user_public_key_hex)
    };

    let domain_name_val = domain_name.clone().unwrap_or_else(|| "Perpetuals".to_string());
    let domain_version_val = domain_version.clone().unwrap_or_else(|| "v0".to_string());
    let domain_chain_id_val = domain_chain_id.clone().unwrap_or_else(|| "SN_MAIN".to_string());
    let domain_revision_val = domain_revision.clone().unwrap_or_else(|| "1".to_string());

    let hash = get_order_hash(
        position_id,
        base_asset_id_hex,
        base_amount,
        quote_asset_id_hex,
        quote_amount,
        fee_asset_id_hex,
        fee_amount,
        expiry,
        nonce_str,
        user_public_key_hex,
        domain_name.unwrap_or_else(|| "Perpetuals".to_string()),
        domain_version.unwrap_or_else(|| "v0".to_string()),
        domain_chain_id.unwrap_or_else(|| "SN_MAIN".to_string()),
        domain_revision.unwrap_or_else(|| "1".to_string()),
    )
    .map_err(|e| anyhow!("order hash error: {e}"))?;

    info!("[DEBUG] Generated order hash: hash={:x}", hash);
    // ───── signature ─────
    let sig = sign_message(&hash, &stark_priv).map_err(|e| anyhow!("sign failed: {e}"))?;
    info!("[DEBUG] Generated signature: r={:x}, s={:x}", sig.r, sig.s);
    Ok(Signature {
        order_hash: hash,
        r: sig.r,
        s: sig.s,
        nonce,
    })
}

pub fn hex_to_felt(hex: &str) -> Felt {
    Felt::from_hex(hex).unwrap()
}
