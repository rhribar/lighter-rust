use thiserror::Error;

#[derive(Error, Debug)]
pub enum PointsBotError {
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),
    
    #[error("Parse error: {0}")]
    Parse(String),
    
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
    
    #[error("Decimal parse error: {0}")]
    Decimal(#[from] rust_decimal::Error),
    
    #[error("Exchange error: {code} - {message}")]
    Exchange { code: String, message: String },
    
    #[error("Configuration error: {0}")]
    Config(String),
    
    #[error("Authentication error: {0}")]
    Auth(String),
    
    #[error("Crypto error: {message}")]
    Crypto { message: String },
    
    #[error("Rate limit exceeded")]
    RateLimit,
    
    #[error("Invalid parameter: {0}")]
    InvalidParameter(String),
    
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    
    #[error("Unknown error: {0}")]
    Unknown(String),
}

pub type PointsBotResult<T> = Result<T, PointsBotError>; 