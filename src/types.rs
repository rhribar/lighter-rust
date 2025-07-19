use std::fmt::Display;

/// Shared types used across the Points Bot
/// 
/// This module contains core types that are used by both fetchers and operators.

use serde::{Deserialize, Serialize};

// ===== SHARED ENUMS =====

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ExchangeName {
    #[serde(rename = "hyperliquid")]
    Hyperliquid,
    #[serde(rename = "extended")]
    Extended,
    #[serde(rename = "kraken")]
    Kraken,
}

impl ExchangeName {
    pub fn as_str(&self) -> &'static str {
        match self {
            ExchangeName::Hyperliquid => "hyperliquid",
            ExchangeName::Extended => "extended",
            ExchangeName::Kraken => "kraken",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side {
    #[serde(rename = "buy")]
    Buy,
    #[serde(rename = "sell")]
    Sell,
}

impl Side {
    pub fn as_str(&self) -> &'static str {
        match self {
            Side::Buy => "buy",
            Side::Sell => "sell",
        }
    }
} 
