use std::collections::HashMap;

use anyhow::{Result, anyhow};
use chrono::{Duration, Utc};
use once_cell::sync::OnceCell;
use primitive_types::U256;
use rand::{Rng, thread_rng};
use reqwest::Client;
use rust_decimal::Decimal;
use rust_decimal::prelude::*; // precise decimals
use serde::Deserialize;
use starknet_crypto::{Felt, pedersen_hash, rfc6979_generate_k, sign};

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
        .get("https://api.extended.exchange/api/v1/info/markets")
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
    EXTENDED_MARKETS
        .get()
        .expect("call init_extended_markets() first")
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
    expiry_ts_ms: Option<i64>,
    nonce: Option<u32>,
    is_reduce_only: bool,
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
        if is_buy { q.ceil() } else { q.floor() }
    }
    .to_u64()
    .unwrap();

    let collateral_value = qty_synthetic * limit_price;
    let collateral_internal = {
        let c = collateral_value * Decimal::from(cfg.collateral_resolution);
        if is_buy { c.ceil() } else { c.floor() }
    }
    .to_u64()
    .unwrap();

    let fee_internal = (Decimal::from(collateral_internal) * fee_rate)
        .ceil()
        .to_u64()
        .unwrap();

    // ───── expiry & nonce ─────
    let expiry_hours: u32 = match expiry_ts_ms {
        Some(ts_ms) => {
            // convert millis → seconds → hours
            ((ts_ms as f64) / 1000.0 / 3600.0).ceil() as u32
        }
        None => {
            let expiry = Utc::now() + Duration::hours(8) + Duration::days(14);
            (expiry.timestamp() as f64 / 3600.0).ceil() as u32
        }
    };

    let nonce: u32 = match nonce {
        Some(n) => n,
        None => thread_rng().gen_range(0..(1 << 31)),
    };

    // ───── hash ─────
    let hash = get_limit_order_msg(
        cfg.synthetic_id,
        cfg.collateral_id,
        is_buy,
        cfg.collateral_id,
        qty_internal,
        collateral_internal,
        fee_internal,
        nonce,
        vault_id,
        expiry_hours,
    );

    // ───── signature ─────
    let k = rfc6979_generate_k(&hash, &stark_priv, None);
    let sig = sign(&stark_priv, &hash, &k).map_err(|e| anyhow!("sign failed: {e:?}"))?;

    Ok(Signature {
        order_hash: hash,
        r: sig.r,
        s: sig.s,
        nonce,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// internals
// ─────────────────────────────────────────────────────────────────────────────

fn get_limit_order_msg(
    asset_id_synthetic: Felt,
    asset_id_collateral: Felt,
    is_buying_synthetic: bool,
    asset_id_fee: Felt,
    amount_synthetic: u64,
    amount_collateral: u64,
    max_amount_fee: u64,
    nonce: u32,
    position_id: u64,
    expiration_timestamp_hours: u32,
) -> Felt {
    let (asset_id_sell, asset_id_buy, amount_sell, amount_buy) = if is_buying_synthetic {
        (
            asset_id_collateral,
            asset_id_synthetic,
            amount_collateral,
            amount_synthetic,
        )
    } else {
        (
            asset_id_synthetic,
            asset_id_collateral,
            amount_synthetic,
            amount_collateral,
        )
    };

    let mut msg = pedersen_hash(&asset_id_sell, &asset_id_buy);
    msg = pedersen_hash(&msg, &asset_id_fee);

    let mut packed0 = U256::from(amount_sell);
    packed0 = (packed0 << 64) + U256::from(amount_buy);
    packed0 = (packed0 << 64) + U256::from(max_amount_fee);
    packed0 = (packed0 << 32) + U256::from(nonce);
    msg = pedersen_hash(&msg, &u256_to_felt(packed0));

    const OP_LIMIT_ORDER_WITH_FEES: u64 = 3;
    let mut packed1 = U256::from(OP_LIMIT_ORDER_WITH_FEES);
    packed1 = (packed1 << 64) + U256::from(position_id);
    packed1 = (packed1 << 64) + U256::from(position_id);
    packed1 = (packed1 << 64) + U256::from(position_id);
    packed1 = (packed1 << 32) + U256::from(expiration_timestamp_hours);
    packed1 <<= 17; // padding

    pedersen_hash(&msg, &u256_to_felt(packed1))
}

fn u256_to_felt(x: U256) -> Felt {
    let mut buf = [0u8; 32];
    x.write_as_big_endian(&mut buf);
    Felt::from_bytes_be(&buf)
}

pub fn hex_to_felt(hex: &str) -> Felt {
    Felt::from_hex(hex).unwrap()
}