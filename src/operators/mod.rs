/// Operators module for Points Bot
/// 
/// This module contains all exchange-specific operators that implement
/// the Operator trait to execute trades on various cryptocurrency exchanges.

pub mod base;
#[path = "operator-hyperliquid.rs"]
pub mod hyperliquid;

#[path = "operator-extended.rs"]
pub mod extended;
pub mod init_extended_markets;

// Re-export everything from base and implementations
pub use base::*;
pub use hyperliquid::*;
pub use extended::*;
pub use init_extended_markets::*;