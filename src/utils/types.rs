use serde::{Deserialize, Serialize};
use std::fmt::{self, Display};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ExchangeName {
    #[serde(rename = "hyperliquid")]
    Hyperliquid,
    #[serde(rename = "extended")]
    Extended,
    #[serde(rename = "lighter")]
    Lighter,
}

impl Display for ExchangeName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExchangeName::Hyperliquid => write!(f, "Hyperliquid"),
            ExchangeName::Extended => write!(f, "Extended"),
            ExchangeName::Lighter => write!(f, "Lighter"),
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
