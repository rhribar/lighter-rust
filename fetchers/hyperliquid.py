"""
Hyperliquid Exchange Fetcher

Fetches account and trading data from Hyperliquid exchange.
"""

from typing import Dict, List, Any
from .base import BaseFetcher

class HyperliquidFetcher(BaseFetcher):
    """Fetcher for Hyperliquid exchange"""
    
    def __init__(self):
        super().__init__(
            name="hyperliquid",
            base_url="https://api.hyperliquid.xyz",
            rate_limit=0.1  # 10 requests per second
        )
        
    def get_account_data(self, address: str) -> Dict[str, Any]:
        """
        Fetch account for a given address from Hyperliquid
        
        Args:
            address: Wallet address to fetch account for
            
        Returns:
            Dictionary containing account data
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
            
            account_data = {
                "exchange": self.name,
                "address": address,
                "account_value": data.get("marginSummary", {}).get("accountValue", "0"),
                "total_ntl_pos": data.get("marginSummary", {}).get("totalNtlPos", "0"),
                "withdrawable": data.get("withdrawable", "0"),
                "positions": len(data.get("assetPositions", [])),
                "timestamp": data.get("time", 0)
            }
            
            return account_data
            
        except Exception as e:
            self.logger.error(f"Failed to fetch account for {address}: {e}")
            return {
                "exchange": self.name,
                "address": address,
                "error": str(e),
                "timestamp": 0
            }
            
    def get_supported_tokens(self) -> List[str]:
        """
        Get list of supported tokens/pairs from Hyperliquid
        
        Returns:
            List of supported token symbols
        """
        try:
            endpoint = "/info"
            payload = {"type": "meta"}
            
            response = self.session.post(f"{self.base_url}{endpoint}", json=payload)
            response.raise_for_status()
            data = response.json()
            
            tokens = []
            if "universe" in data:
                for token_info in data["universe"]:
                    if "name" in token_info:
                        tokens.append(token_info["name"])
                        
            return tokens
            
        except Exception as e:
            self.logger.error(f"Failed to fetch supported tokens: {e}")
            return []
            
    def get_funding_rates(self) -> Dict[str, Any]:
        try:
            endpoint = "/info"
            payload = {"type": "metaAndAssetCtxs"}
            
            response = self.session.post(f"{self.base_url}{endpoint}", json=payload)
            response.raise_for_status()
            data = response.json()
            
            funding_rates = {}
            meta = data[0]
            asset_ctxs = data[1]
            
            for i, asset_ctx in enumerate(asset_ctxs):
                coin = meta["universe"][i]["name"]
                funding_rate = float(asset_ctx.get("funding", 0))
                mark_price = float(asset_ctx.get("markPx", 0))
                
                funding_rates[coin] = {
                    "funding_rate": funding_rate,
                    "funding_rate_8h": funding_rate * 8,
                    "mark_price": mark_price,
                    "exchange": self.name
                }
                
            return {
                "exchange": self.name,
                "funding_rates": funding_rates,
                "timestamp": data[1][0].get("time", 0) if data[1] else 0
            }
            
        except Exception as e:
            return {
                "exchange": self.name,
                "error": str(e),
                "timestamp": 0
            }
            
    def get_user_positions(self, address: str) -> Dict[str, Any]:
        """
        Get user positions from Hyperliquid
        
        Args:
            address: Wallet address
            
        Returns:
            Dictionary containing position data
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
            
            return {
                "exchange": self.name,
                "address": address,
                "positions": data.get("assetPositions", []),
                "margin_summary": data.get("marginSummary", {}),
                "timestamp": data.get("time", 0)
            }
            
        except Exception as e:
            self.logger.error(f"Failed to fetch positions for {address}: {e}")
            return {
                "exchange": self.name,
                "address": address,
                "error": str(e),
                "timestamp": 0
            } 