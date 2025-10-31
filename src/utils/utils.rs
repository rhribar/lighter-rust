use crate::{PointsBotError, PointsBotResult};
use rust_decimal::Decimal;

pub fn parse_decimal(s: &str) -> PointsBotResult<Decimal> {
    s.parse::<Decimal>().map_err(|e| PointsBotError::Parse {
        msg: format!("Failed to parse string '{}' as decimal", s),
        source: Some(Box::new(e)),
    })
}
