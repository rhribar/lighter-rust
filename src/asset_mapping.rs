use crate::{ExchangeName};

pub struct AssetMapping;

impl AssetMapping {
    pub fn get_canonical_ticker(exchange: ExchangeName, symbol: &str) -> Option<String> {
        let stripped_symbol = match exchange {
            ExchangeName::Extended => symbol.strip_suffix("-USD").map(|s| s.to_string()),
            /* ExchangeName::Kraken => symbol.strip_suffix("-USDT").map(|s| s.to_string()), */
            _ => Some(symbol.to_string()),
        };

        stripped_symbol.map(|s| match s.as_str() {
            _ => match exchange {
                ExchangeName::Hyperliquid => match s.as_str() {
                    _ => s,
                },
                ExchangeName::Extended => match s.as_str() {
                    "1000PEPE" => "kPEPE".to_string(),
                    "1000SHIB" => "kSHIB".to_string(),
                    "1000BONK" => "kBONK".to_string(),
                    "XAUT" => "XAU".to_string(),
                    _ => s,
                },
                ExchangeName::Kraken => match s.as_str() {
                    "XBT" => "BTC".to_string(),
                    _ => s,
                },
                ExchangeName::Lighter => match s.as_str() {
                    _ => s,
                },
            },
        })
    }

    pub fn get_exchange_ticker(exchange: ExchangeName, src_asset: &str) -> Option<String> {
        let asset = match src_asset {
            _ => match exchange {
                ExchangeName::Hyperliquid => match src_asset {
                    _ => src_asset,
                },
                ExchangeName::Extended => match src_asset {
                    "kPEPE" => "1000PEPE",
                    "kSHIB" => "1000SHIB",
                    "kBONK" => "1000BONK",
                    "XAU" => "XAUT",
                    _ => src_asset,
                },
                ExchangeName::Kraken => match src_asset {
                    "XBT" => "BTC",
                    _ => src_asset,
                },
                ExchangeName::Lighter => match src_asset {
                    _ => src_asset,
                },
            },
        };

        match exchange {
            ExchangeName::Extended => Some(format!("{}-USD", asset)),
            ExchangeName::Kraken => Some(format!("{}-USD", asset)),
            _ => Some(asset.to_string()),
        }
    }
}
