use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ExchangeName {
    #[serde(rename = "hyperliquid")]
    Hyperliquid,
    #[serde(rename = "extended")]
    Extended,
    #[serde(rename = "kraken")]
    Kraken,
    #[serde(rename = "lighter")]
    Lighter,
}

impl ExchangeName {
    pub fn as_str(&self) -> &'static str {
        match self {
            ExchangeName::Hyperliquid => "hyperliquid",
            ExchangeName::Extended => "extended",
            ExchangeName::Kraken => "kraken",
            ExchangeName::Lighter => "lighter",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PositionSide {
    #[serde(rename = "long")]
    Long,
    #[serde(rename = "short")]
    Short,
}

impl PositionSide {
    pub fn as_str(&self) -> &'static str {
        match self {
            PositionSide::Long => "long",
            PositionSide::Short => "short",
        }
    }
} 
