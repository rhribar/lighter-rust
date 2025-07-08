"""
Fetcher Types

Types specific to data fetching operations across exchanges.
"""

from typing import Dict, List, Optional, TypedDict, Literal
from enum import Enum
from bot_types import ExchangeName, Result

# ===== ACCOUNT & BALANCE TYPES =====

class MarginSummary(TypedDict):
    """Account margin summary from exchange"""
    accountValue: str
    totalMarginUsed: str
    totalNtlPos: str
    totalRawUsd: str

class AccountBalance(TypedDict):
    """Processed account balance information"""
    account_value: float
    total_margin_used: float
    total_ntl_pos: float
    total_raw_usd: float
    withdrawable: float
    available_balance: float
    positions_count: int
    exchange: ExchangeName
    timestamp: int

# ===== POSITION TYPES =====

class PositionSide(str, Enum):
    LONG = "long"
    SHORT = "short"

class Position(TypedDict):
    """Trading position information"""
    symbol: str
    size: str
    side: PositionSide
    entry_price: float
    mark_price: float
    unrealized_pnl: float
    margin_used: float
    liquidation_price: Optional[float]

class PositionsData(TypedDict):
    """User positions response"""
    exchange: ExchangeName
    address: str
    positions: List[Position]
    margin_summary: MarginSummary
    withdrawable: str
    timestamp: int

# ===== FUNDING RATE TYPES =====

class FundingRateData(TypedDict):
    """Funding rate information for a single asset"""
    funding_rate: float          # Current hourly rate
    funding_rate_8h: float       # 8-hour projected rate
    mark_price: float           # Current mark price
    index_price: Optional[float] # Index price
    next_funding_time: Optional[int] # Next funding timestamp
    exchange: ExchangeName

class FundingRates(TypedDict):
    """Funding rates response"""
    exchange: ExchangeName
    funding_rates: Dict[str, FundingRateData]
    timestamp: int

# ===== ARBITRAGE TYPES =====

class ArbitrageDirection(str, Enum):
    LONG = "long"
    SHORT = "short"

class ArbitrageOpportunity(TypedDict):
    """Single arbitrage opportunity"""
    coin: str
    funding_rate_8h: float
    mark_price: float
    direction: ArbitrageDirection
    exchange: ExchangeName
    expected_profit_bps: Optional[float]  # Expected profit in basis points

class ArbitrageOpportunities(TypedDict):
    """Arbitrage opportunities response"""
    opportunities: List[ArbitrageOpportunity]
    total_opportunities: int
    max_profit_bps: Optional[float]
    timestamp: int

# ===== MARKET DATA TYPES =====

class MarketInfo(TypedDict):
    """Market/trading pair information"""
    symbol: str
    base_asset: str
    quote_asset: str
    status: Literal["active", "inactive", "delisted"]
    min_order_size: str
    max_order_size: Optional[str]
    price_precision: int
    size_precision: int

class MarketStatistics(TypedDict):
    """Market statistics"""
    symbol: str
    volume_24h: float
    price_change_24h: float
    high_24h: float
    low_24h: float
    last_price: float
    mark_price: float
    funding_rate: float

# ===== TOKEN/ASSET TYPES =====

TokenList = List[str]

class TokenInfo(TypedDict):
    """Detailed token information"""
    symbol: str
    name: str
    decimals: int
    is_active: bool
    min_trade_amount: Optional[str]
    withdrawal_fee: Optional[str]

# ===== RESULT TYPE ALIASES =====

# Specific result types for fetcher operations
AccountBalanceResult = Result  # Success[AccountBalance] | Failure[BotError]
FundingRatesResult = Result    # Success[FundingRates] | Failure[BotError]
PositionsResult = Result       # Success[PositionsData] | Failure[BotError]
TokenListResult = Result       # Success[TokenList] | Failure[BotError]
ArbitrageResult = Result       # Success[ArbitrageOpportunities] | Failure[BotError]
MarketInfoResult = Result      # Success[List[MarketInfo]] | Failure[BotError]
MarketStatsResult = Result     # Success[List[MarketStatistics]] | Failure[BotError] 