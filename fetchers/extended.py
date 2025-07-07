"""
Extended Exchange Fetcher

Fetches funding rates and trading data from Extended exchange.
"""

import os
from typing import Dict, List, Any
from .base import BaseFetcher

class ExtendedFetcher(BaseFetcher):
    """Fetcher for Extended exchange"""
    
    def __init__(self, api_key: str = None):
        super().__init__(
            name="extended",
            base_url="https://api.extended.exchange/api/v1",
            rate_limit=0.1  # 1000 requests per minute = ~16 per second
        )
        self.api_key = api_key or os.getenv("EXTENDED_API_KEY")
        
        # Set required headers
        if self.api_key:
            self.session.headers.update({
                "X-Api-Key": self.api_key,
                "User-Agent": "points-bot/1.0"
            })
        
    def get_account_data(self, address: str) -> Dict[str, Any]:
        """Get account balance and position data"""
        try:
            # Get account balance
            balance_response = self.session.get(f"{self.base_url}/user/balance")
            balance_response.raise_for_status()
            balance_data = balance_response.json()
            
            # Get positions
            positions_response = self.session.get(f"{self.base_url}/user/positions")
            positions_response.raise_for_status()
            positions_data = positions_response.json()
            
            positions = positions_data.get("data", []) if positions_data.get("status") == "ok" else []
            balance = balance_data.get("data", {}) if balance_data.get("status") == "ok" else {}
            
            return {
                "exchange": self.name,
                "address": address,
                "account_value": balance.get("equity", "0"),
                "total_ntl_pos": str(sum(float(pos.get("notional", 0)) for pos in positions)),
                "withdrawable": balance.get("withdrawable", "0"),
                "positions": len(positions),
                "timestamp": balance.get("timestamp", 0)
            }
            
        except Exception as e:
            self.logger.error(f"Failed to fetch points data: {e}")
            return {
                "exchange": self.name,
                "address": address,
                "error": str(e),
                "timestamp": 0
            }
    
    def get_supported_tokens(self) -> List[str]:
        """Get list of supported markets"""
        try:
            response = self.session.get(f"{self.base_url}/info/markets")
            response.raise_for_status()
            data = response.json()

            tokens = []
            if data.get("status") == "OK":  # Extended uses "OK" not "ok"
                markets = data.get("data", [])
                for market in markets:
                    if market.get("assetName"):  # Use assetName field
                        tokens.append(market["assetName"])
                        
            return tokens
            
        except Exception as e:
            self.logger.error(f"Failed to fetch supported tokens: {e}")
            return []
    
    def get_funding_rates(self) -> Dict[str, Any]:
        """Get funding rates for all markets"""
        try:
            response = self.session.get(f"{self.base_url}/info/markets")
            response.raise_for_status()
            data = response.json()
            
            funding_rates = {}
            if data.get("status") == "OK":
                markets = data.get("data", [])
                
                for market in markets:
                    asset_name = market.get("assetName", "")
                    market_stats = market.get("marketStats", {})
                    
                    # Extract funding rate and mark price from marketStats
                    funding_rate = float(market_stats.get("fundingRate", 0))
                    mark_price = float(market_stats.get("markPrice", 0))
                    
                    if asset_name:
                        funding_rates[asset_name] = {
                            "funding_rate": funding_rate,
                            "funding_rate_8h": funding_rate * 8,  # Assuming hourly rate
                            "mark_price": mark_price,
                            "exchange": self.name
                        }
                    
            return {
                "exchange": self.name,
                "funding_rates": funding_rates,
                "timestamp": int(data.get("timestamp", 0)) if data.get("timestamp") else 0
            }
            
        except Exception as e:
            self.logger.error(f"Failed to fetch funding rates: {e}")
            return {
                "exchange": self.name,
                "error": str(e),
                "timestamp": 0
            }
    
    def get_user_positions(self, address: str) -> Dict[str, Any]:
        """Get user positions"""
        try:
            response = self.session.get(f"{self.base_url}/user/positions")
            response.raise_for_status()
            data = response.json()
            
            if data.get("status") == "ok":
                positions = data.get("data", [])
                
                # Calculate margin summary
                total_notional = sum(float(pos.get("notional", 0)) for pos in positions)
                total_margin = sum(float(pos.get("margin", 0)) for pos in positions)
                
                # Get balance data for account value
                balance_response = self.session.get(f"{self.base_url}/user/balance")
                balance_response.raise_for_status()
                balance_data = balance_response.json()
                balance = balance_data.get("data", {}) if balance_data.get("status") == "ok" else {}
                
                return {
                    "exchange": self.name,
                    "address": address,
                    "positions": positions,
                    "margin_summary": {
                        "accountValue": balance.get("equity", "0"),
                        "totalMarginUsed": str(total_margin),
                        "totalNtlPos": str(total_notional),
                        "totalRawUsd": balance.get("balance", "0")
                    },
                    "withdrawable": balance.get("withdrawable", "0"),
                    "timestamp": data.get("timestamp", 0)
                }
            else:
                return {
                    "exchange": self.name,
                    "address": address,
                    "error": data.get("error", {}).get("message", "Unknown error"),
                    "timestamp": 0
                }
                
        except Exception as e:
            self.logger.error(f"Failed to fetch positions: {e}")
            return {
                "exchange": self.name,
                "address": address,
                "error": str(e),
                "timestamp": 0
            } 