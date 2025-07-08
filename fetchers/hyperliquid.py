"""
Hyperliquid Exchange Fetcher

Fetches account and trading data from Hyperliquid exchange.
"""

from typing import Dict, List, Any, Optional
from .base import BaseFetcher

# Import our comprehensive type system
from bot_types import ExchangeName, ErrorCode, Result, create_success, create_error
from .types import (
    AccountBalanceResult, PositionsResult, TokenListResult, FundingRatesResult
)

class HyperliquidFetcher(BaseFetcher):
    """
    Fetcher for Hyperliquid exchange.
    
    Handles data fetching from Hyperliquid's API including account data,
    positions, supported tokens, and funding rates.
    """
    
    def __init__(self) -> None:
        """Initialize Hyperliquid fetcher."""
        super().__init__(
            name=ExchangeName.HYPERLIQUID,
            base_url="https://api.hyperliquid.xyz",
            rate_limit=0.1  # 10 requests per second
        )
        
    def get_account_data(self, address: str) -> AccountBalanceResult:
        """
        Fetch account data for a given address from Hyperliquid.
        
        Args:
            address: Wallet address to fetch account data for
            
        Returns:
            Result containing account data or error
        """
        try:
            endpoint = "/info"
            payload = {
                "type": "clearinghouseState",
                "user": address
            }
            
            response = self.session.post(f"{self.base_url}{endpoint}", json=payload)
            response.raise_for_status()
            data = response.json()
            
            margin_summary: Dict[str, Any] = data.get("marginSummary", {})
            asset_positions: List[Dict[str, Any]] = data.get("assetPositions", [])
            
            account_data = {
                "exchange": self.name.value,
                "address": address,
                "account_value": margin_summary.get("accountValue", "0"),
                "total_ntl_pos": margin_summary.get("totalNtlPos", "0"),
                "withdrawable": data.get("withdrawable", "0"),
                "positions": len(asset_positions),
                "timestamp": data.get("time", 0)
            }
            
            return account_data
            
        except Exception as e:
            self.logger.error(f"Failed to fetch account for {address}: {e}")
            return {
                "exchange": self.name.value,
                "address": address,
                "error": str(e),
                "timestamp": 0
            }
    
    def get_user_positions(self, address: str) -> PositionsResult:
        """
        Get user positions from Hyperliquid.
        
        Args:
            address: Wallet address to fetch positions for
            
        Returns:
            Result containing user positions or error
        """
        try:
            endpoint = "/info"
            payload = {
                "type": "clearinghouseState",
                "user": address
            }
            
            response = self.session.post(f"{self.base_url}{endpoint}", json=payload)
            response.raise_for_status()
            data = response.json()
            
            positions: List[Dict[str, Any]] = data.get("assetPositions", [])
            margin_summary: Dict[str, Any] = data.get("marginSummary", {})
            
            return {
                "exchange": self.name.value,
                "address": address,
                "positions": positions,
                "margin_summary": margin_summary,
                "withdrawable": data.get("withdrawable", "0"),
                "timestamp": data.get("time", 0)
            }
            
        except Exception as e:
            self.logger.error(f"Failed to fetch positions for {address}: {e}")
            return {
                "exchange": self.name.value,
                "address": address,
                "error": str(e),
                "timestamp": 0
            }
    
    def get_supported_tokens(self) -> TokenListResult:
        """
        Get list of supported tokens from Hyperliquid.
        
        Returns:
            Result containing list of supported tokens or error
        """
        try:
            endpoint = "/info"
            payload = {"type": "meta"}
            
            response = self.session.post(f"{self.base_url}{endpoint}", json=payload)
            response.raise_for_status()
            data = response.json()
            
            tokens: List[str] = []
            universe: List[Dict[str, Any]] = data.get("universe", [])
            
            for token_info in universe:
                token_name: Optional[str] = token_info.get("name")
                if token_name:
                    tokens.append(token_name)
            
            return tokens
            
        except Exception as e:
            self.logger.error(f"Failed to fetch supported tokens: {e}")
            return []
    
    def get_funding_rates(self) -> FundingRatesResult:
        """
        Get funding rates for all tokens from Hyperliquid.
        
        Returns:
            Result containing funding rates data or error
        """
        try:
            endpoint = "/info"
            payload = {"type": "metaAndAssetCtxs"}
            
            response = self.session.post(f"{self.base_url}{endpoint}", json=payload)
            response.raise_for_status()
            data = response.json()
            
            funding_rates: Dict[str, Dict[str, Any]] = {}
            
            # data is a list [meta, assetCtxs]
            if isinstance(data, list) and len(data) >= 2:
                meta_data = data[0]
                asset_ctxs = data[1]
                
                universe: List[Dict[str, Any]] = meta_data.get("universe", [])
                
                for i, token_info in enumerate(universe):
                    token_name: str = token_info.get("name", "")
                    if token_name and i < len(asset_ctxs):
                        ctx = asset_ctxs[i]
                        
                        # Get funding rate and mark price from asset context
                        funding_rate: float = float(ctx.get("funding", 0))
                        mark_price: float = float(ctx.get("markPx", 0))
                        
                        funding_rates[token_name] = {
                            "funding_rate": funding_rate,
                            "funding_rate_8h": funding_rate * 8,  # 8-hour projection
                            "mark_price": mark_price,
                            "exchange": self.name.value
                        }
            
            return {
                "exchange": self.name.value,
                "funding_rates": funding_rates,
                "timestamp": data[0].get("time", 0) if isinstance(data, list) and len(data) > 0 else 0
            }
            
        except Exception as e:
            self.logger.error(f"Failed to fetch funding rates: {e}")
            return {
                "exchange": self.name.value,
                "error": str(e),
                "timestamp": 0
            } 