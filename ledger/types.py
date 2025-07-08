"""
Ledger Types

Types for tracking balances, transactions, and portfolio across exchanges.
"""

from typing import Dict, List, Optional, TypedDict, Literal, Union
from decimal import Decimal
from enum import Enum
from bot_types import ExchangeName, Result

# ===== LEDGER ACCOUNT TYPES =====

class AccountType(str, Enum):
    SPOT = "spot"
    MARGIN = "margin"
    FUTURES = "futures"
    OPTIONS = "options"
    SAVINGS = "savings"

class LedgerAccount(TypedDict):
    """Individual account on an exchange"""
    exchange: ExchangeName
    account_type: AccountType
    account_id: str
    display_name: str
    is_active: bool
    created_at: int

# ===== BALANCE TYPES =====

class BalanceType(str, Enum):
    AVAILABLE = "available"    # Free to trade
    LOCKED = "locked"         # In open orders
    MARGIN = "margin"         # Used as margin
    COLLATERAL = "collateral"  # Used as collateral
    STAKED = "staked"         # Staked/earning rewards
    PENDING = "pending"       # Pending deposits/withdrawals

class AssetBalance(TypedDict):
    """Balance for a single asset in an account"""
    asset: str
    total: str               # Total balance
    available: str           # Available for trading
    locked: str             # Locked in orders
    margin: str             # Used as margin/collateral
    staked: str             # Staked amount
    pending_deposit: str    # Pending deposits
    pending_withdrawal: str # Pending withdrawals
    last_updated: int

class AccountBalance(TypedDict):
    """All balances for a single account"""
    account: LedgerAccount
    balances: Dict[str, AssetBalance]  # asset -> balance
    total_value_usd: float
    total_value_btc: float
    margin_level: Optional[str]
    last_updated: int

# ===== PORTFOLIO AGGREGATION TYPES =====

class AggregatedBalance(TypedDict):
    """Aggregated balance across all exchanges"""
    asset: str
    total_amount: str
    total_value_usd: float
    exchange_breakdown: Dict[ExchangeName, AssetBalance]
    percentage_of_portfolio: float

class PortfolioSummary(TypedDict):
    """Complete portfolio summary"""
    total_value_usd: float
    total_value_btc: float
    total_unrealized_pnl: float
    total_realized_pnl: float
    total_fees: float
    asset_allocation: Dict[str, AggregatedBalance]
    exchange_allocation: Dict[ExchangeName, float]  # Percentage per exchange
    account_count: int
    last_updated: int

class PortfolioSnapshot(TypedDict):
    """Point-in-time portfolio snapshot"""
    timestamp: int
    total_value_usd: float
    total_value_btc: float
    accounts: List[AccountBalance]
    summary: PortfolioSummary

# ===== TRANSACTION TYPES =====

class TransactionType(str, Enum):
    DEPOSIT = "deposit"
    WITHDRAWAL = "withdrawal"
    TRADE = "trade"
    TRANSFER = "transfer"      # Between accounts
    FEE = "fee"
    FUNDING = "funding"        # Funding payments
    INTEREST = "interest"      # Interest earned
    STAKING_REWARD = "staking_reward"
    LIQUIDATION = "liquidation"

class TransactionStatus(str, Enum):
    PENDING = "pending"
    CONFIRMED = "confirmed"
    FAILED = "failed"
    CANCELED = "canceled"

class Transaction(TypedDict):
    """Individual transaction record"""
    id: str
    type: TransactionType
    status: TransactionStatus
    exchange: ExchangeName
    account: LedgerAccount
    asset: str
    amount: str                # Positive = credit, Negative = debit
    balance_after: str
    fee: Optional[str]
    fee_asset: Optional[str]
    reference_id: Optional[str]  # Exchange transaction ID
    description: str
    timestamp: int

class TransactionHistory(TypedDict):
    """Transaction history for an account"""
    account: LedgerAccount
    transactions: List[Transaction]
    total_count: int
    from_timestamp: int
    to_timestamp: int

# ===== FUNDING TRACKING TYPES =====

class FundingPayment(TypedDict):
    """Funding payment record"""
    exchange: ExchangeName
    symbol: str
    position_size: str
    funding_rate: float
    payment_amount: str      # Positive = received, Negative = paid
    timestamp: int

class FundingHistory(TypedDict):
    """Funding payment history"""
    payments: List[FundingPayment]
    total_received: str
    total_paid: str
    net_funding: str
    from_timestamp: int
    to_timestamp: int

# ===== PNL TRACKING TYPES =====

class PnLEntry(TypedDict):
    """Individual P&L entry"""
    timestamp: int
    exchange: ExchangeName
    symbol: str
    side: Literal["long", "short"]
    entry_price: str
    exit_price: str
    size: str
    realized_pnl: str
    fees: str
    holding_period_hours: int

class PnLSummary(TypedDict):
    """P&L summary"""
    total_realized_pnl: str
    total_unrealized_pnl: str
    total_fees: str
    win_count: int
    loss_count: int
    win_rate: float
    average_win: str
    average_loss: str
    largest_win: str
    largest_loss: str
    profit_factor: float      # Total wins / Total losses

# ===== ARBITRAGE TRACKING TYPES =====

class ArbitrageTrade(TypedDict):
    """Individual arbitrage trade"""
    id: str
    start_time: int
    end_time: Optional[int]
    symbol: str
    funding_rate: float
    position_size: str
    expected_profit: str
    actual_profit: Optional[str]
    exchange: ExchangeName
    status: Literal["open", "closed", "failed"]

class ArbitrageMetrics(TypedDict):
    """Arbitrage trading metrics"""
    total_trades: int
    active_trades: int
    total_profit: str
    average_profit_per_trade: str
    success_rate: float
    average_holding_hours: float
    best_trade_profit: str
    worst_trade_profit: str

# ===== LEDGER OPERATIONS =====

class LedgerUpdate(TypedDict):
    """Ledger update operation"""
    account: LedgerAccount
    updates: List[Transaction]
    new_balances: Dict[str, AssetBalance]
    timestamp: int

class BalanceSync(TypedDict):
    """Balance synchronization record"""
    exchange: ExchangeName
    sync_timestamp: int
    accounts_synced: int
    balances_updated: int
    errors: List[str]
    next_sync_time: int

# ===== RESULT TYPE ALIASES =====

AccountBalanceResult = Result      # Success[AccountBalance] | Failure[BotError]
PortfolioResult = Result          # Success[PortfolioSummary] | Failure[BotError]
TransactionResult = Result        # Success[Transaction] | Failure[BotError]
TransactionHistoryResult = Result # Success[TransactionHistory] | Failure[BotError]
FundingHistoryResult = Result     # Success[FundingHistory] | Failure[BotError]
PnLResult = Result               # Success[PnLSummary] | Failure[BotError]
ArbitrageMetricsResult = Result  # Success[ArbitrageMetrics] | Failure[BotError]
LedgerUpdateResult = Result      # Success[LedgerUpdate] | Failure[BotError] 