use crate::ExchangeName;

#[derive(Debug, Clone, Copy)]
pub enum TickerDirection {
    ToCanonical,
    ToExchange,
}

pub struct AssetMapping;

impl AssetMapping {
    pub fn map_ticker(
        exchange: ExchangeName,
        symbol: &str,
        direction: TickerDirection,
    ) -> Option<String> {
        match direction {
            TickerDirection::ToCanonical => {
                let s = match exchange {
                    ExchangeName::Extended => {
                        symbol.strip_suffix("-USD").unwrap_or(symbol).to_string()
                    }
                    _ => symbol.to_string(),
                };
                match exchange {
                    ExchangeName::Extended => match s.as_str() {
                        "1000PEPE" => Some("kPEPE".to_string()),
                        "1000SHIB" => Some("kSHIB".to_string()),
                        "1000BONK" => Some("kBONK".to_string()),
                        "XAUT" => Some("XAU".to_string()),
                        _ => Some(s),
                    },
                    _ => Some(s),
                }
            }
            TickerDirection::ToExchange => {
                let asset = match exchange {
                    ExchangeName::Extended => match symbol {
                        "kPEPE" => "1000PEPE",
                        "kSHIB" => "1000SHIB",
                        "kBONK" => "1000BONK",
                        "XAU" => "XAUT",
                        _ => symbol,
                    },
                    _ => symbol,
                };
                match exchange {
                    ExchangeName::Extended => Some(format!("{}-USD", asset)),
                    _ => Some(asset.to_string()),
                }
            }
        }
    }
}
