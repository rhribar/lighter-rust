"""
Extended Exchange Operator

Handles trading operations on Extended exchange.
"""

import requests
import time
import json
import os
from typing import Dict, Any
from .base import BaseOperator

class ExtendedOperator(BaseOperator):
    def __init__(self, api_key: str = None, stark_key: str = None):
        self.api_key = api_key or os.getenv("EXTENDED_API_KEY")
        self.stark_key = stark_key or os.getenv("EXTENDED_STARK_KEY")
        self.base_url = "https://api.extended.exchange/api/v1"
        self.session = requests.Session()
        self.is_testing = os.getenv("TRADING_ENV", "testing").lower() == "testing"
        
        # Set required headers
        if self.api_key:
            self.session.headers.update({
                "X-Api-Key": self.api_key,
                "User-Agent": "points-bot/1.0",
                "Content-Type": "application/json"
            })
        
    def _get_market_id(self, symbol: str) -> str:
        """Get market ID for a symbol (e.g., 'BTC' -> 'BTCUSD')"""
        try:
            response = self.session.get(f"{self.base_url}/markets")
            response.raise_for_status()
            data = response.json()
            
            if data.get("status") == "ok":
                markets = data.get("data", [])
                for market in markets:
                    if market.get("symbol", "").replace("USD", "") == symbol:
                        return market.get("id", "")
            
            return f"{symbol}USD"  # Fallback
            
        except Exception as e:
            return f"{symbol}USD"  # Fallback
    
    def create_order(self, symbol: str, is_buy: bool, size: str, price: str = None) -> Dict[str, Any]:
        """Create an order on Extended"""
        if not self.api_key:
            return {"error": "No API key configured"}
        
        # Note: Extended requires Stark signatures for order management
        # This is a simplified implementation - full implementation would need Stark key signing
        if not self.stark_key:
            return {"error": "Stark key required for order management on Extended"}
            
        try:
            market_id = self._get_market_id(symbol)
            
            # Order payload
            order_data = {
                "market": market_id,
                "side": "buy" if is_buy else "sell",
                "type": "limit" if price else "market",
                "quantity": size,
                "price": price,
                "timeInForce": "GTC",  # Good Till Canceled
                "reduceOnly": False
            }
            
            # For testing mode, return mock response
            if self.is_testing:
                return {
                    "status": "testing",
                    "exchange": "extended",
                    "symbol": symbol,
                    "is_buy": is_buy,
                    "size": size,
                    "price": price,
                    "result": {"order_id": f"test_{int(time.time())}", "status": "pending"},
                    "timestamp": int(time.time())
                }
            
            # For live mode, would need proper Stark signature here
            # This is a placeholder - actual implementation needs Stark key signing
            response = self.session.post(f"{self.base_url}/user/orders", json=order_data)
            response.raise_for_status()
            result = response.json()
            
            return {
                "status": "live",
                "exchange": "extended",
                "symbol": symbol,
                "is_buy": is_buy,
                "size": size,
                "price": price,
                "result": result,
                "timestamp": int(time.time())
            }
            
        except Exception as e:
            return {"error": str(e)}
    
    def close_position(self, symbol: str) -> Dict[str, Any]:
        """Close a position on Extended"""
        if not self.api_key or not self.stark_key:
            return {"error": "API key and Stark key required"}
        
        try:
            # Get current position
            positions_response = self.session.get(f"{self.base_url}/user/positions")
            positions_response.raise_for_status()
            positions_data = positions_response.json()
            
            if positions_data.get("status") != "ok":
                return {"error": "Failed to fetch positions"}
            
            positions = positions_data.get("data", [])
            target_position = None
            
            for pos in positions:
                if pos.get("symbol", "").replace("USD", "") == symbol:
                    target_position = pos
                    break
            
            if not target_position:
                return {"error": f"No position found for {symbol}"}
            
            # Create closing order
            size = abs(float(target_position.get("size", 0)))
            is_buy = float(target_position.get("size", 0)) < 0  # If short, buy to close
            
            return self.create_order(symbol, is_buy, str(size))
            
        except Exception as e:
            return {"error": str(e)}
    
    def get_account_info(self) -> Dict[str, Any]:
        """Get account information"""
        if not self.api_key:
            return {"error": "No API key configured"}
        
        try:
            response = self.session.get(f"{self.base_url}/user/balance")
            response.raise_for_status()
            data = response.json()
            
            if data.get("status") == "ok":
                return {
                    "exchange": "extended",
                    "balance": data.get("data", {}),
                    "timestamp": int(time.time())
                }
            else:
                return {"error": data.get("error", {}).get("message", "Unknown error")}
                
        except Exception as e:
            return {"error": str(e)} 