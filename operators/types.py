"""
Operator Types

Types specific to trading operations and order management.
"""

from typing import Dict, List, Optional, TypedDict, Literal, Union
from decimal import Decimal
from enum import Enum
from bot_types import ExchangeName, Result

# ===== ORDER TYPES =====

class OrderSide(str, Enum):
    BUY = "buy"
    SELL = "sell"

class OrderType(str, Enum):
    MARKET = "market"
    LIMIT = "limit"
    STOP = "stop"
    STOP_LIMIT = "stop_limit"
    TAKE_PROFIT = "take_profit"
    TAKE_PROFIT_LIMIT = "take_profit_limit"

class TimeInForce(str, Enum):
    GTC = "gtc"  # Good Till Canceled
    IOC = "ioc"  # Immediate Or Cancel
    FOK = "fok"  # Fill Or Kill

class OrderStatus(str, Enum):
    PENDING = "pending"
    OPEN = "open"
    PARTIALLY_FILLED = "partially_filled"
    FILLED = "filled"
    CANCELED = "canceled"
    REJECTED = "rejected"
    EXPIRED = "expired"

# ===== ORDER REQUEST/RESPONSE TYPES =====

class OrderRequest(TypedDict):
    """Order creation request"""
    symbol: str
    side: OrderSide
    type: OrderType
    size: str  # Use string to avoid floating point precision issues
    price: Optional[str]
    stop_price: Optional[str]
    time_in_force: Optional[TimeInForce]
    reduce_only: Optional[bool]
    post_only: Optional[bool]  # Post-only (maker-only) orders
    client_order_id: Optional[str]

class OrderResponse(TypedDict):
    """Order creation response"""
    order_id: str
    client_order_id: Optional[str]
    symbol: str
    side: OrderSide
    type: OrderType
    size: str
    price: Optional[str]
    status: OrderStatus
    filled_size: Optional[str]
    remaining_size: Optional[str]
    average_price: Optional[str]
    created_at: int
    updated_at: int

class OrderFill(TypedDict):
    """Order execution/fill information"""
    fill_id: str
    order_id: str
    symbol: str
    side: OrderSide
    size: str
    price: str
    fee: str
    fee_currency: str
    timestamp: int
    is_maker: bool

# ===== POSITION MANAGEMENT TYPES =====

class PositionInfo(TypedDict):
    """Position information"""
    symbol: str
    size: str
    side: Literal["long", "short"]
    entry_price: str
    mark_price: str
    liquidation_price: Optional[str]
    unrealized_pnl: str
    margin_used: str
    percentage_pnl: str

class PositionUpdate(TypedDict):
    """Position update information"""
    symbol: str
    old_size: str
    new_size: str
    side: Literal["long", "short"]
    entry_price: str
    mark_price: str
    realized_pnl: str
    timestamp: int

# ===== RISK MANAGEMENT TYPES =====

class RiskLimits(TypedDict):
    """Risk management limits"""
    max_position_size: str
    max_order_size: str
    max_daily_loss: str
    max_open_orders: int
    allowed_symbols: List[str]

class RiskCheck(TypedDict):
    """Risk check result"""
    passed: bool
    violations: List[str]
    warnings: List[str]
    max_allowed_size: Optional[str]

# ===== PORTFOLIO MANAGEMENT TYPES =====

class PortfolioBalance(TypedDict):
    """Portfolio balance information"""
    total_equity: str
    available_balance: str
    used_margin: str
    unrealized_pnl: str
    total_margin_requirement: str
    maintenance_margin: str
    free_collateral: str

class PortfolioSummary(TypedDict):
    """Portfolio summary"""
    exchange: ExchangeName
    balances: PortfolioBalance
    open_positions: List[PositionInfo]
    open_orders: List[OrderResponse]
    daily_pnl: str
    total_fees_paid: str
    timestamp: int

# ===== EXECUTION STRATEGY TYPES =====

class ExecutionStrategy(str, Enum):
    MARKET = "market"
    LIMIT = "limit"
    TWAP = "twap"  # Time-Weighted Average Price
    VWAP = "vwap"  # Volume-Weighted Average Price
    ICEBERG = "iceberg"

class ExecutionParams(TypedDict):
    """Execution strategy parameters"""
    strategy: ExecutionStrategy
    total_size: str
    max_slice_size: Optional[str]
    time_interval: Optional[int]  # seconds
    price_limit: Optional[str]
    start_time: Optional[int]
    end_time: Optional[int]

# ===== RESULT TYPE ALIASES =====

# Specific result types for operator operations
OrderResult = Result           # Success[OrderResponse] | Failure[BotError]
ClosePositionResult = Result   # Success[OrderResponse] | Failure[BotError]
PositionInfoResult = Result    # Success[PositionInfo] | Failure[BotError]
PortfolioResult = Result       # Success[PortfolioSummary] | Failure[BotError]
RiskCheckResult = Result       # Success[RiskCheck] | Failure[BotError]
ExecutionResult = Result       # Success[List[OrderResponse]] | Failure[BotError] 