"""
Global Bot Types

Shared types used across all modules of the points bot.
"""

from typing import Dict, List, Any, Union, Optional, TypedDict, Literal
from dataclasses import dataclass
from enum import Enum
import time

# ===== GLOBAL ENUMS =====

class ExchangeName(str, Enum):
    HYPERLIQUID = "hyperliquid"
    EXTENDED = "extended"
    BINANCE = "binance"
    BYBIT = "bybit"

class TradingEnvironment(str, Enum):
    TESTING = "testing"
    LIVE = "live"

class AssetType(str, Enum):
    SPOT = "spot"
    PERP = "perpetual"
    FUTURE = "future"
    OPTION = "option"

# ===== ERROR HANDLING =====

@dataclass
class BotError:
    """Standard error format for all bot operations"""
    code: str
    message: str
    exchange: Optional[str] = None
    timestamp: Optional[int] = None
    details: Optional[Dict[str, Any]] = None
    
    def __post_init__(self):
        if self.timestamp is None:
            self.timestamp = int(time.time())

class ErrorCode(str, Enum):
    # Authentication errors
    AUTHENTICATION_FAILED = "AUTH_FAILED"
    API_KEY_INVALID = "API_KEY_INVALID"
    SIGNATURE_INVALID = "SIGNATURE_INVALID"
    
    # Trading errors
    INSUFFICIENT_BALANCE = "INSUFFICIENT_BALANCE"
    INVALID_SYMBOL = "INVALID_SYMBOL"
    INVALID_ORDER_SIZE = "INVALID_ORDER_SIZE"
    MARKET_CLOSED = "MARKET_CLOSED"
    ORDER_REJECTED = "ORDER_REJECTED"
    
    # Network/API errors
    API_ERROR = "API_ERROR"
    NETWORK_ERROR = "NETWORK_ERROR"
    RATE_LIMIT_EXCEEDED = "RATE_LIMIT_EXCEEDED"
    ENDPOINT_NOT_FOUND = "ENDPOINT_NOT_FOUND"
    
    # Data errors
    INVALID_DATA_FORMAT = "INVALID_DATA_FORMAT"
    MISSING_REQUIRED_FIELD = "MISSING_REQUIRED_FIELD"
    
    # Generic
    UNKNOWN_ERROR = "UNKNOWN_ERROR"

# ===== RESULT TYPES =====

@dataclass
class Success:
    """Success wrapper for Result type"""
    data: Any
    
@dataclass
class Failure:
    """Failure wrapper for Result type"""
    error: BotError

# Result type - Rust-inspired error handling
Result = Union[Success, Failure]

# ===== BASIC DATA TYPES =====

class Timestamp(TypedDict):
    """Timestamp information"""
    unix: int
    iso: str

class ExchangeInfo(TypedDict):
    """Basic exchange information"""
    name: ExchangeName
    display_name: str
    base_url: str
    is_active: bool

class AssetInfo(TypedDict):
    """Asset/token information"""
    symbol: str
    name: str
    type: AssetType
    decimals: int
    min_trade_size: Optional[str]
    max_trade_size: Optional[str]

# ===== UTILITY FUNCTIONS =====

def create_success(data: Any) -> Success:
    """Create a Success result"""
    return Success(data=data)

def create_error(
    code: ErrorCode, 
    message: str, 
    exchange: Optional[ExchangeName] = None,
    details: Optional[Dict[str, Any]] = None
) -> Failure:
    """Create a Failure result with proper error formatting"""
    error = BotError(
        code=code.value,
        message=message,
        exchange=exchange.value if exchange else None,
        details=details
    )
    return Failure(error=error)

def is_success(result: Result) -> bool:
    """Check if result is a success"""
    return isinstance(result, Success)

def is_failure(result: Result) -> bool:
    """Check if result is a failure"""
    return isinstance(result, Failure)

def unwrap_result(result: Result) -> Any:
    """Extract data from Success result or raise exception for Failure"""
    if is_success(result):
        return result.data
    else:
        raise Exception(f"[{result.error.code}] {result.error.message}")

def unwrap_or_default(result: Result, default: Any) -> Any:
    """Extract data from Success result or return default for Failure"""
    if is_success(result):
        return result.data
    else:
        return default

# ===== TYPE ALIASES =====

# Generic result types for different operations
BotResult = Result
DataResult = Result
APIResponse = Union[Dict[str, Any], List[Any]] 