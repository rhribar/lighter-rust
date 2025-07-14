use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use crate::{PointsBotError, PointsBotResult};

pub fn str_to_decimal(s: &str) -> PointsBotResult<Decimal> {
    s.parse::<Decimal>()
        .map_err(|e| PointsBotError::Parse(format!("Failed to parse '{}' as decimal: {}", s, e)))
}

pub fn current_timestamp() -> DateTime<Utc> {
    Utc::now()
}

pub fn format_decimal(d: &Decimal, precision: u32) -> String {
    format!("{:.1$}", d, precision as usize)
} 