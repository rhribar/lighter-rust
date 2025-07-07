"""
Operator Types

Types specific to trading operations and order management.
"""

from typing import Dict, List, Optional, TypedDict, Literal, Union
from decimal import Decimal
from bot_types import ExchangeName, Result

# ===== ORDER TYPES =====

class OrderSide(str, Literal):
    BUY = "buy"
    SELL = "sell"

class OrderType(str, Literal):
    MARKET = "market"
    LIMIT = "limit"
    STOP = "stop"
    STOP_LIMIT = "stop_limit"
    TAKE_PROFIT = "take_profit"
    TAKE_PROFIT_LIMIT = "take_profit_limit"

class TimeInForce(str, Literal):
    GTC = "gtc"  # Good Till Canceled
    IOC = "ioc"  # Immediate Or Cancel
    FOK = "fok"  # Fill Or Kill
    GTD = "gtd"  # Good Till Date

class OrderStatus(str, Literal):
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
    status: Literal["live", "testing", "pending", "filled", "rejected"]
    exchange: ExchangeName
    symbol: str
    side: OrderSide
    type: OrderType
    size: str
    price: Optional[str]
    order_id: Optional[str]
    client_order_id: Optional[str]
    result: Dict[str, any]  # Raw exchange response
    timestamp: int

class OrderFill(TypedDict):
    """Order fill information"""
    fill_id: str
    order_id: str
    symbol: str
    side: OrderSide
    size: str
    price: str
    fee: str
    fee_currency: str
    timestamp: int

class OrderDetails(TypedDict):
    """Detailed order information"""
    order_id: str
    client_order_id: Optional[str]
    symbol: str
    side: OrderSide
    type: OrderType
    status: OrderStatus
    size: str
    filled_size: str
    remaining_size: str
    price: Optional[str]
    average_fill_price: Optional[str]
    time_in_force: TimeInForce
    reduce_only: bool
    post_only: bool
    created_at: int
    updated_at: int
    fills: List[OrderFill]

# ===== POSITION MANAGEMENT TYPES =====

class PositionRequest(TypedDict):
    """Position modification request"""
    symbol: str
    action: Literal["open", "close", "reduce", "increase"]
    size: Optional[str]
    side: Optional[OrderSide]
    price: Optional[str]

class PositionResponse(TypedDict):
    """Position modification response"""
    success: bool
    position_id: Optional[str]
    symbol: str
    action: str
    result: Dict[str, any]
    timestamp: int

# ===== RISK MANAGEMENT TYPES =====

class RiskLimits(TypedDict):
    """Risk management limits"""
    max_position_size: str
    max_order_size: str
    max_daily_volume: str
    max_leverage: float
    stop_loss_percentage: Optional[float]
    take_profit_percentage: Optional[float]

class RiskMetrics(TypedDict):
    """Current risk metrics"""
    current_leverage: float
    portfolio_var: float  # Value at Risk
    max_drawdown: float
    sharpe_ratio: Optional[float]
    daily_pnl: float
    unrealized_pnl: float

# ===== PORTFOLIO MANAGEMENT TYPES =====

class PortfolioSummary(TypedDict):
    """Portfolio summary across exchanges"""
    total_equity: float
    total_unrealized_pnl: float
    total_margin_used: float
    available_balance: float
    positions_count: int
    exchanges: List[ExchangeName]
    last_updated: int

class CrossExchangePosition(TypedDict):
    """Position across multiple exchanges"""
    symbol: str
    total_size: float
    total_notional: float
    average_entry_price: float
    unrealized_pnl: float
    exchanges: Dict[ExchangeName, Dict[str, any]]

# ===== EXECUTION STRATEGY TYPES =====

class ExecutionStrategy(str, Literal):
    MARKET = "market"
    LIMIT = "limit"
    TWAP = "twap"  # Time-Weighted Average Price
    VWAP = "vwap"  # Volume-Weighted Average Price
    ICEBERG = "iceberg"

class TWAPConfig(TypedDict):
    """TWAP execution configuration"""
    duration_minutes: int
    slice_count: int
    randomize_timing: bool
    max_participation_rate: float

class IcebergConfig(TypedDict):
    """Iceberg order configuration"""
    total_size: str
    slice_size: str
    price_variance: float

class ExecutionRequest(TypedDict):
    """Advanced execution request"""
    symbol: str
    side: OrderSide
    total_size: str
    strategy: ExecutionStrategy
    target_price: Optional[str]
    max_slippage: Optional[float]
    twap_config: Optional[TWAPConfig]
    iceberg_config: Optional[IcebergConfig]
    risk_limits: Optional[RiskLimits]

# ===== RESULT TYPE ALIASES =====

# Specific result types for operator operations
OrderResult = Result          # Success[OrderResponse] | Failure[BotError]
OrderDetailsResult = Result   # Success[OrderDetails] | Failure[BotError]
PositionResult = Result       # Success[PositionResponse] | Failure[BotError]
ExecutionResult = Result      # Success[List[OrderResponse]] | Failure[BotError]
PortfolioResult = Result      # Success[PortfolioSummary] | Failure[BotError]
RiskMetricsResult = Result    # Success[RiskMetrics] | Failure[BotError] 