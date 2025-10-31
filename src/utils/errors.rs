use thiserror::Error;

#[derive(Error, Debug)]
pub enum PointsBotError {
    #[error("Network error: {msg}")]
    Network {
        msg: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("Parse error: {msg}")]
    Parse {
        msg: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("JSON parse error: {msg}")]
    Json {
        msg: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("Decimal parse error: {msg}")]
    Decimal {
        msg: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("Exchange error: {code} - {message}")]
    Exchange { code: String, message: String },

    #[error("Configuration error: {msg}")]
    Config {
        msg: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("Authentication error: {msg}")]
    Auth {
        msg: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("Invalid parameter: {msg}")]
    InvalidParameter { msg: String },

    #[error("Unknown error: {msg}")]
    Unknown {
        msg: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },
}

pub type PointsBotResult<T> = Result<T, PointsBotError>;

impl From<rust_decimal::Error> for PointsBotError {
    fn from(e: rust_decimal::Error) -> Self {
        PointsBotError::Decimal {
            msg: "Decimal error".to_string(),
            source: Some(Box::new(e)),
        }
    }
}

impl From<reqwest::Error> for PointsBotError {
    fn from(e: reqwest::Error) -> Self {
        PointsBotError::Unknown {
            msg: format!("Reqwest error: {}", e),
            source: Some(Box::new(e)),
        }
    }
}

impl From<serde_json::Error> for PointsBotError {
    fn from(e: serde_json::Error) -> Self {
        PointsBotError::Parse {
            msg: format!("Serde JSON error: {}", e),
            source: Some(Box::new(e)),
        }
    }
}
