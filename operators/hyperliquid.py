"""
Hyperliquid Exchange Operator

Handles trading operations on Hyperliquid exchange.
"""

import requests
import time
import json
import os
from typing import Dict, List, Any, Optional, Union
from .base import BaseOperator
from .utils import sign_l1_action

# Import our comprehensive type system
from bot_types import ExchangeName, ErrorCode, Result, create_success, create_error
from .types import (
    OrderResult, OrderRequest, OrderResponse, OrderSide, OrderType, 
    TimeInForce, OrderStatus, ClosePositionResult
)

class HyperliquidOperator(BaseOperator):
    """
    Trading operator for Hyperliquid exchange.
    
    Handles order creation, position management, and trade execution
    using wallet address and private key authentication.
    """
    
    def __init__(self, wallet_address: str, private_key: str) -> None:
        """
        Initialize Hyperliquid operator.
        
        Args:
            wallet_address: Hyperliquid wallet address
            private_key: Private key for signing transactions
        """
        self.wallet_address: str = wallet_address
        self.private_key: str = private_key
        self.base_url: str = "https://api.hyperliquid.xyz"
        self.session: requests.Session = requests.Session()
        self.is_testing: bool = os.getenv("TRADING_ENV", "testing").lower() == "testing"
        
    def _get_nonce(self) -> int:
        """
        Generate a nonce for the transaction.
        
        Returns:
            Unix timestamp in milliseconds
        """
        return int(time.time() * 1000)
    
    def create_order(self, symbol: str, is_buy: bool, size: str, price: Optional[str] = None) -> OrderResult:
        """
        Create an order on Hyperliquid exchange.
        
        Args:
            symbol: Trading symbol (asset_id for Hyperliquid)
            is_buy: True for buy order, False for sell order
            size: Order size in base asset
            price: Optional limit price for limit orders
            
        Returns:
            Result containing order information or error
        """
        if not self.private_key:
            return create_error(
                ErrorCode.AUTHENTICATION_FAILED,
                "No private key configured",
                ExchangeName.HYPERLIQUID
            )
        
        try:
            # Convert symbol to asset_id for Hyperliquid
            asset_id: int = int(symbol) if symbol.isdigit() else 0  # Default to BTC (0)
            
            action: Dict[str, Any] = {
                "type": "order",
                "orders": [{
                    "a": asset_id,
                    "b": is_buy,
                    "p": price or "0",
                    "s": size,
                    "r": False,
                    "t": {"limit": {"tif": "Gtc"}}
                }],
                "grouping": "na"
            }
            
            nonce: int = self._get_nonce()
            
            # Sign the action
            signature: Dict[str, str] = sign_l1_action(
                self.private_key, 
                action, 
                None, 
                nonce
            )
            
            payload: Dict[str, Any] = {
                "action": action,
                "nonce": nonce,
                "signature": signature,
                "vaultAddress": None
            }
            
            # For testing mode, return mock response
            if self.is_testing:
                return create_success({
                    "status": "testing",
                    "exchange": ExchangeName.HYPERLIQUID.value,
                    "symbol": symbol,
                    "is_buy": is_buy,
                    "size": size,
                    "price": price,
                    "result": {"order_id": f"test_{nonce}", "status": "pending"},
                    "timestamp": int(time.time())
                })
            
            # Submit order to Hyperliquid
            response = self.session.post(f"{self.base_url}/exchange", json=payload)
            response.raise_for_status()
            result = response.json()
            
            if result.get("status") == "ok":
                return create_success({
                    "exchange": ExchangeName.HYPERLIQUID.value,
                    "symbol": symbol,
                    "is_buy": is_buy,
                    "size": size,
                    "price": price,
                    "result": result.get("response", {}),
                    "timestamp": int(time.time())
                })
            else:
                error_message: str = result.get("response", {}).get("error", "Unknown error")
                return create_error(
                    ErrorCode.ORDER_REJECTED,
                    f"Order rejected: {error_message}",
                    ExchangeName.HYPERLIQUID,
                    {"symbol": symbol, "is_buy": is_buy, "size": size, "price": price}
                )
            
        except requests.exceptions.HTTPError as e:
            if e.response.status_code == 401:
                return create_error(
                    ErrorCode.AUTHENTICATION_FAILED,
                    "Authentication failed - check private key",
                    ExchangeName.HYPERLIQUID
                )
            elif e.response.status_code == 400:
                return create_error(
                    ErrorCode.INVALID_ORDER_SIZE,
                    "Invalid order parameters",
                    ExchangeName.HYPERLIQUID,
                    {"symbol": symbol, "is_buy": is_buy, "size": size, "price": price}
                )
            else:
                return create_error(
                    ErrorCode.API_ERROR,
                    f"HTTP error {e.response.status_code}: {str(e)}",
                    ExchangeName.HYPERLIQUID
                )
                
        except Exception as e:
            return create_error(
                ErrorCode.UNKNOWN_ERROR,
                f"Order creation failed: {str(e)}",
                ExchangeName.HYPERLIQUID,
                {"symbol": symbol, "is_buy": is_buy, "size": size, "price": price}
            )
    
    def close_position(self, symbol: str) -> ClosePositionResult:
        """
        Close position for a specific symbol.
        
        Args:
            symbol: Trading symbol (asset_id for Hyperliquid)
            
        Returns:
            Result containing close position information or error
        """
        if not self.private_key:
            return create_error(
                ErrorCode.AUTHENTICATION_FAILED,
                "No private key configured",
                ExchangeName.HYPERLIQUID
            )
            
        try:
            # Get current positions first
            endpoint = "/info"
            payload = {
                "type": "clearinghouseState",
                "user": self.wallet_address
            }
            
            response = self.session.post(f"{self.base_url}{endpoint}", json=payload)
            response.raise_for_status()
            data = response.json()
            
            asset_positions: List[Dict[str, Any]] = data.get("assetPositions", [])
            asset_id: int = int(symbol) if symbol.isdigit() else 0
            
            # Find the position to close
            target_position: Optional[Dict[str, Any]] = None
            for position in asset_positions:
                if position.get("position", {}).get("coin") == asset_id:
                    target_position = position
                    break
            
            if not target_position:
                return create_error(
                    ErrorCode.API_ERROR,
                    f"No open position found for asset {symbol}",
                    ExchangeName.HYPERLIQUID,
                    {"symbol": symbol}
                )
            
            # Calculate close order parameters
            position_info: Dict[str, Any] = target_position.get("position", {})
            position_size: float = float(position_info.get("szi", 0))
            
            if position_size == 0:
                return create_error(
                    ErrorCode.API_ERROR,
                    f"Position size is zero for asset {symbol}",
                    ExchangeName.HYPERLIQUID,
                    {"symbol": symbol}
                )
            
            # Close position by creating opposite order
            is_buy: bool = position_size < 0  # If short position, buy to close
            close_size: str = str(abs(position_size))
            
            # Create market order to close position (use extreme price for market execution)
            market_price: str = "999999" if is_buy else "0.001"
            
            return self.create_order(symbol, is_buy, close_size, market_price)
            
        except Exception as e:
            return create_error(
                ErrorCode.UNKNOWN_ERROR,
                f"Position close failed: {str(e)}",
                ExchangeName.HYPERLIQUID,
                {"symbol": symbol}
            ) 