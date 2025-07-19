use crate::{ExchangeName};

pub struct AssetMapping;

impl AssetMapping {
    pub fn get_exchange_ticker(exchange: ExchangeName, src_asset: &str) -> Option<String> {
        let asset = match src_asset {
            "FTM" => "S", // Example migration for all exchanges
            _ => match exchange {
                ExchangeName::Hyperliquid => match src_asset {
                    /* "PEPE" => "kPEPE", // basically this will be used in this project as well, for now as poc, for now important the appendix
                    "BONK" => "kBONK", */
                    _ => src_asset,
                },
                ExchangeName::Extended => match src_asset {
                    /* "PEPE" => "kPEPE",
                    "BONK" => "kBONK", */
                    _ => src_asset,
                },
                ExchangeName::Kraken => match src_asset {
                    "XBT" => "BTC",
                    _ => src_asset,
                },
            },
        };

        match exchange {
            ExchangeName::Hyperliquid => Some(format!("{}-USDT", asset)),
            ExchangeName::Extended => Some(format!("{}-USD", asset)),
            ExchangeName::Kraken => Some(format!("{}-USD", asset)),
        }
    }

    pub fn get_generic_ticker(exchange: ExchangeName, symbol: &str) -> Option<String> {
        let stripped_symbol = match exchange {
            ExchangeName::Extended => symbol.strip_suffix("-USD").map(|s| s.to_string()),
            ExchangeName::Kraken => symbol.strip_suffix("-USDT").map(|s| s.to_string()),
            _ => Some(symbol.to_string()),
        };

        stripped_symbol.map(|s| match s.as_str() {
            "FTM" => "S".to_string(), // Example migration for all exchanges
            _ => match exchange {
                ExchangeName::Hyperliquid => match s.as_str() {
                    /* "PEPE" => "kPEPE".to_string(),
                    "BONK" => "kBONK".to_string(), */
                    _ => s,
                },
                ExchangeName::Extended => match s.as_str() {
                    /* "PEPE" => "kPEPE".to_string(),
                    "BONK" => "kBONK".to_string(), */
                    _ => s,
                },
                ExchangeName::Kraken => match s.as_str() {
                    /* "PEPE" => "kPEPE".to_string(),
                    "BONK" => "kBONK".to_string(), */
                    _ => s,
                },
            },
        })
    }
}
