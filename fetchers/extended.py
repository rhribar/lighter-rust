"""
Extended Exchange Fetcher

Fetches funding rates and trading data from Extended exchange.
"""

import os
from typing import Dict, List, Any, Optional
from .base import BaseFetcher

# Import our comprehensive type system
from bot_types import ExchangeName, ErrorCode, Result, create_success, create_error
from .types import (
    AccountBalanceResult, PositionsResult, TokenListResult, FundingRatesResult
)

class ExtendedFetcher(BaseFetcher):
    """
    Fetcher for Extended exchange.
    
    Handles authentication via API key and provides methods for
    fetching account data, positions, and funding rates.
    """
    
    def __init__(self, api_key: Optional[str] = None) -> None:
        """
        Initialize Extended fetcher.
        
        Args:
            api_key: Extended API key for authentication
        """
        super().__init__(
            name=ExchangeName.EXTENDED,
            base_url="https://api.extended.exchange/api/v1",
            rate_limit=0.1  # 1000 requests per minute = ~16 per second
        )
        self.api_key: Optional[str] = api_key or os.getenv("EXTENDED_API_KEY")
        
        # Set required headers
        if self.api_key:
            self.session.headers.update({
                "X-Api-Key": self.api_key,
                "User-Agent": "points-bot/1.0"
            })
        
    def get_account_data(self, address: str) -> AccountBalanceResult:
        """
        Get account balance and position data.
        
        Args:
            address: Account identifier (not used by Extended, kept for interface compatibility)
            
        Returns:
            Result containing account balance information or error
        """
        try:
            balance_response = self.session.get(f"{self.base_url}/user/balance") 
            balance_response.raise_for_status()
            balance_data = balance_response.json()
            
            # Get positions
            positions_response = self.session.get(f"{self.base_url}/user/positions")
            positions_response.raise_for_status()
            positions_data = positions_response.json()
            
            positions: List[Dict[str, Any]] = positions_data.get("data", []) if positions_data.get("status") == "OK" else []
            balance: Dict[str, Any] = balance_data.get("data", {}) if balance_data.get("status") == "OK" else {}
            
            return {
                "exchange": self.name.value,
                "address": address,
                "account_value": balance.get("equity", "0"),
                "total_ntl_pos": str(sum(float(pos.get("notional", 0)) for pos in positions)),
                "withdrawable": balance.get("availableForWithdrawal", "0"),
                "positions": len(positions),
                "timestamp": balance.get("updatedTime", 0)
            }
            
        except Exception as e:
            self.logger.error(f"Failed to fetch points data: {e}")
            return {
                "exchange": self.name.value,
                "address": address,
                "error": str(e),
                "timestamp": 0
            }
    
    def get_supported_tokens(self) -> TokenListResult:
        """
        Get list of supported markets.
        
        Returns:
            Result containing list of supported tokens or error
        """
        try:
            response = self.session.get(f"{self.base_url}/info/markets")
            response.raise_for_status()
            data = response.json()

            tokens: List[str] = []
            if data.get("status") == "OK":  # Extended uses "OK" not "ok"
                markets: List[Dict[str, Any]] = data.get("data", [])
                for market in markets:
                    asset_name: Optional[str] = market.get("assetName")
                    if asset_name:  # Use assetName field
                        tokens.append(asset_name)
                        
            return tokens
            
        except Exception as e:
            self.logger.error(f"Failed to fetch supported tokens: {e}")
            return []
    
    def get_funding_rates(self) -> FundingRatesResult:
        """
        Get funding rates for all markets.
        
        Returns:
            Result containing funding rates data or error
        """
        try:
            response = self.session.get(f"{self.base_url}/info/markets")
            response.raise_for_status()
            data = response.json()
            
            funding_rates: Dict[str, Dict[str, Any]] = {}
            if data.get("status") == "OK":
                markets: List[Dict[str, Any]] = data.get("data", [])
                
                for market in markets:
                    asset_name: str = market.get("assetName", "")
                    market_stats: Dict[str, Any] = market.get("marketStats", {})
                    
                    # Extract funding rate and mark price from marketStats
                    funding_rate: float = float(market_stats.get("fundingRate", 0))
                    mark_price: float = float(market_stats.get("markPrice", 0))
                    
                    if asset_name:
                        funding_rates[asset_name] = {
                            "funding_rate": funding_rate,
                            "funding_rate_8h": funding_rate * 8,  # Assuming hourly rate
                            "mark_price": mark_price,
                            "exchange": self.name.value
                        }
                    
            return {
                "exchange": self.name.value,
                "funding_rates": funding_rates,
                "timestamp": int(data.get("timestamp", 0)) if data.get("timestamp") else 0
            }
            
        except Exception as e:
            self.logger.error(f"Failed to fetch funding rates: {e}")
            return {
                "exchange": self.name.value,
                "error": str(e),
                "timestamp": 0
            }
    
    def get_user_positions(self, address: str) -> PositionsResult:
        """
        Get user positions.
        
        Args:
            address: Account identifier (not used by Extended, kept for interface compatibility)
            
        Returns:
            Result containing user positions or error
        """
        try:
            response = self.session.get(f"{self.base_url}/user/positions")
            response.raise_for_status()
            data = response.json()
            
            if data.get("status") == "OK":
                positions: List[Dict[str, Any]] = data.get("data", [])
                
                # Calculate margin summary
                total_notional: float = sum(float(pos.get("notional", 0)) for pos in positions)
                total_margin: float = sum(float(pos.get("margin", 0)) for pos in positions)
                
                # Get balance data for account value
                balance_response = self.session.get(f"{self.base_url}/user/balance")
                balance_response.raise_for_status()
                balance_data = balance_response.json()
                balance: Dict[str, Any] = balance_data.get("data", {}) if balance_data.get("status") == "OK" else {}
                
                return {
                    "exchange": self.name.value,
                    "address": address,
                    "positions": positions,
                    "margin_summary": {
                        "accountValue": balance.get("equity", "0"),
                        "totalMarginUsed": str(total_margin),
                        "totalNtlPos": str(total_notional),
                        "totalRawUsd": balance.get("balance", "0")
                    },
                    "withdrawable": balance.get("availableForWithdrawal", "0"),
                    "timestamp": data.get("updatedTime", 0)
                }
            else:
                error_message: str = data.get("error", {}).get("message", "Unknown error")
                return {
                    "exchange": self.name.value,
                    "address": address,
                    "error": error_message,
                    "timestamp": 0
                }
                
        except Exception as e:
            self.logger.error(f"Failed to fetch positions: {e}")
            return {
                "exchange": self.name.value,
                "address": address,
                "error": str(e),
                "timestamp": 0
            } 