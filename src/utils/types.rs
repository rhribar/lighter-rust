use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ExchangeName {
    #[serde(rename = "hyperliquid")]
    Hyperliquid,
    #[serde(rename = "extended")]
    Extended,
    #[serde(rename = "lighter")]
    Lighter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PositionSide {
    #[serde(rename = "long")]
    Long,
    #[serde(rename = "short")]
    Short,
}
