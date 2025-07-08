import logging
import os
from typing import Dict, List, Any, Optional, Union, Tuple
from datetime import datetime
from fetchers.extended import ExtendedFetcher
from fetchers.hyperliquid import HyperliquidFetcher
from operators.extended import ExtendedOperator
from operators.hyperliquid import HyperliquidOperator
from dotenv import load_dotenv

# Import our comprehensive type system
from bot_types import Result, ExchangeName, create_success, create_error, ErrorCode
from fetchers.types import AccountBalanceResult, PositionsResult, TokenListResult, FundingRatesResult
from operators.types import OrderResult, OrderRequest, OrderSide, OrderType

load_dotenv()

# Exchange configuration - can be easily changed
EXCHANGE_NAME: ExchangeName = ExchangeName.EXTENDED  # Change this to ExchangeName.HYPERLIQUID to switch!

# Environment variables based on exchange
if EXCHANGE_NAME == ExchangeName.EXTENDED:
    API_KEY: Optional[str] = os.getenv("EXTENDED_API_KEY")
    STARK_KEY: Optional[str] = os.getenv("EXTENDED_STARK_KEY")
    WALLET_ADDRESS: Optional[str] = None  # Extended doesn't use wallet addresses
elif EXCHANGE_NAME == ExchangeName.HYPERLIQUID:
    API_KEY: Optional[str] = None  # Hyperliquid doesn't use API keys
    STARK_KEY: Optional[str] = None  # Hyperliquid doesn't use Stark keys
    WALLET_ADDRESS: Optional[str] = os.getenv("WALLET_ADDRESS")
    HYPERLIQUID_PRIVATE_KEY: Optional[str] = os.getenv("HYPERLIQUID_PRIVATE_KEY")

TRADING_ENV: str = os.getenv("TRADING_ENV", "testing")

# Type aliases for better readability
ArbitrageOpportunity = Dict[str, Union[str, float]]
AccountSummary = Dict[str, Union[float, int, str]]
ArbitrageResult = Dict[str, Union[List[ArbitrageOpportunity], int, str]]

class PointsBot:
    """
    Main Points Bot class for funding arbitrage detection and execution.
    
    Exchange-agnostic implementation that can work with any exchange
    by simply changing the EXCHANGE_NAME constant.
    """
    
    def __init__(self, exchange_name: ExchangeName = EXCHANGE_NAME) -> None:
        """
        Initialize the Points Bot for specified exchange.
        
        Args:
            exchange_name: Which exchange to use (Extended or Hyperliquid)
        """
        self.exchange_name: ExchangeName = exchange_name
        
        # Initialize fetcher and operator based on exchange
        if exchange_name == ExchangeName.EXTENDED:
            self.fetcher = ExtendedFetcher(API_KEY)
            self.operator = ExtendedOperator(API_KEY, STARK_KEY)
            self.account_identifier = "extended_account"
        elif exchange_name == ExchangeName.HYPERLIQUID:
            self.fetcher = HyperliquidFetcher()
            self.operator = HyperliquidOperator(WALLET_ADDRESS, HYPERLIQUID_PRIVATE_KEY)
            self.account_identifier = WALLET_ADDRESS
        else:
            raise ValueError(f"Unsupported exchange: {exchange_name}")
        
    def get_points_data(self) -> AccountBalanceResult:
        """
        Get account points data from the configured exchange.
        
        Returns:
            Result containing account balance information or error
        """
        return self.fetcher.get_account_data(self.account_identifier)
    
    def get_positions(self) -> PositionsResult:
        """
        Get user positions from the configured exchange.
        
        Returns:
            Result containing user positions or error
        """
        return self.fetcher.get_user_positions(self.account_identifier)
    
    def get_account_balances(self) -> AccountSummary:
        """
        Get detailed account balance information in a standardized format.
        
        Returns:
            Dictionary containing formatted account balance data
        """
        # Get raw account data using the typed interface
        account_data = self.get_points_data()
        
        if "error" in account_data:
            return {"error": account_data["error"]}
        
        # Extract standardized fields from the typed response
        return {
            "account_value": float(account_data.get("account_value", "0")),
            "total_margin_used": 0.0,  # Will be calculated if positions data available
            "total_ntl_pos": float(account_data.get("total_ntl_pos", "0")),
            "total_raw_usd": float(account_data.get("account_value", "0")),  # Fallback
            "withdrawable": float(account_data.get("withdrawable", "0")),
            "available_balance": float(account_data.get("account_value", "0")) - float(account_data.get("total_ntl_pos", "0")),
            "positions_count": int(account_data.get("positions", 0)),
            "exchange": self.exchange_name.value,
            "timestamp": account_data.get("timestamp", 0)
        }
        
    def get_supported_tokens(self) -> TokenListResult:
        """
        Get list of supported tokens from the configured exchange.
        
        Returns:
            Result containing list of supported tokens or error
        """
        return self.fetcher.get_supported_tokens()
        
    def get_funding_rates(self) -> FundingRatesResult:
        """
        Get funding rates for all tokens from the configured exchange.
        
        Returns:
            Result containing funding rates data or error
        """
        return self.fetcher.get_funding_rates()
        
    def check_funding_arbitrage(self, threshold: float = 0.01) -> ArbitrageResult:
        """
        Check for funding arbitrage opportunities.
        
        Args:
            threshold: Minimum funding rate threshold (default 1%)
            
        Returns:
            Dictionary containing arbitrage opportunities and metadata
        """
        funding_data = self.get_funding_rates()
        
        if "error" in funding_data:
            return {"error": funding_data["error"]}
            
        opportunities: List[ArbitrageOpportunity] = []
        funding_rates = funding_data["funding_rates"]
        
        for coin, data in funding_rates.items():
            funding_rate_8h: float = data["funding_rate_8h"]
            
            if abs(funding_rate_8h) > threshold:
                opportunities.append({
                    "coin": coin,
                    "funding_rate_8h": funding_rate_8h,
                    "mark_price": data["mark_price"],
                    "direction": "short" if funding_rate_8h > 0 else "long",
                    "exchange": self.exchange_name.value  # Dynamic exchange name!
                })
        
        return {
            "opportunities": opportunities,
            "total_opportunities": len(opportunities),
            "timestamp": funding_data["timestamp"]
        }
    
    def create_order(self, symbol: str, is_buy: bool, size: str, price: Optional[str] = None) -> OrderResult:
        """
        Create order on the configured exchange.
        
        Args:
            symbol: Trading symbol (e.g., 'BTC')
            is_buy: True for buy order, False for sell order
            size: Order size in base asset
            price: Optional limit price
            
        Returns:
            Result containing order information or error
        """
        return self.operator.create_order(symbol, is_buy, size, price)
    
    def close_position(self, symbol: str) -> OrderResult:
        """
        Close position for a specific symbol.
        
        Args:
            symbol: Trading symbol to close position for
            
        Returns:
            Result containing order information or error
        """
        return self.operator.close_position(symbol)

def main() -> None:
    """
    Main function to demonstrate bot functionality.
    
    Tests all major features including account data, funding rates,
    arbitrage detection, and trading operations.
    """
    bot = PointsBot()
    
    print(f"=== {bot.exchange_name.value.upper()} EXCHANGE ===")
    if bot.exchange_name == ExchangeName.EXTENDED:
        print(f"API Key: {'✅ Configured' if API_KEY else '❌ Missing'}")
        print(f"Stark Key: {'✅ Configured' if STARK_KEY else '❌ Missing'}")
    elif bot.exchange_name == ExchangeName.HYPERLIQUID:
        print(f"Wallet Address: {'✅ Configured' if WALLET_ADDRESS else '❌ Missing'}")
        print(f"Private Key: {'✅ Configured' if HYPERLIQUID_PRIVATE_KEY else '❌ Missing'}")
    print(f"Trading Environment: {TRADING_ENV}")
    print()
    
    print("=== ACCOUNT BALANCES ===")
    balances: AccountSummary = bot.get_account_balances()
    if "error" in balances:
        print(f"Error: {balances['error']}")
    else:
        print(f"Account Value: ${balances['account_value']:.2f}")
        print(f"Available Balance: ${balances['available_balance']:.2f}")
        print(f"Margin Used: ${balances['total_margin_used']:.2f}")
        print(f"Withdrawable: ${balances['withdrawable']:.2f}")
        print(f"Total Position Value: ${balances['total_ntl_pos']:.2f}")
        print(f"Open Positions: {balances['positions_count']}")
    print()
    """     
        print("=== SUPPORTED TOKENS ===")
        tokens: TokenListResult = bot.get_supported_tokens()
        print(f"Found {len(tokens)} supported tokens")
        if tokens:
            print(f"First 10: {tokens[:10]}")
        print()
        
        print("=== FUNDING RATES ===")
        funding_data: FundingRatesResult = bot.get_funding_rates()
        if "error" in funding_data:
            print(f"Error: {funding_data['error']}")
        else:
            funding_rates = funding_data["funding_rates"]
            print(f"Found funding rates for {len(funding_rates)} tokens")
            
            # Show top 5 highest funding rates
            sorted_rates = sorted(funding_rates.items(), key=lambda x: abs(x[1]['funding_rate_8h']), reverse=True)[:5]
            for coin, data in sorted_rates:
                print(f"{coin}: {data['funding_rate_8h']:.4f} (8h), Mark: ${data['mark_price']:.2f}")
        print()
        
        print("=== ARBITRAGE OPPORTUNITIES ===")
        opportunities: ArbitrageResult = bot.check_funding_arbitrage()
        if "error" in opportunities:
            print(f"Error: {opportunities['error']}")
        else:
            print(f"Found {opportunities['total_opportunities']} opportunities")
            for opp in opportunities['opportunities'][:5]:
                print(f"{opp['coin']}: {opp['funding_rate_8h']:.4f} - {opp['direction']} at ${opp['mark_price']:.2f}")
        print() """
    
    print("=== TRADING TEST ===")
    # Check if we have the required credentials for trading
    can_trade = False
    if bot.exchange_name == ExchangeName.EXTENDED and STARK_KEY:
        can_trade = True
    elif bot.exchange_name == ExchangeName.HYPERLIQUID and HYPERLIQUID_PRIVATE_KEY:
        can_trade = True
    
    if can_trade:
        # Test small BTC order
        result: OrderResult = bot.create_order("BTC", True, "0.0001", "100000")
        print(f"Order result: {result}")
    else:
        print(f"⚠️  Missing credentials for {bot.exchange_name.value} - trading disabled")

if __name__ == "__main__":
    main()
