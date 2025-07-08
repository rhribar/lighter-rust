"""
Extended Exchange Operator

Handles trading operations on Extended exchange using the official x10-python-trading SDK.
"""

import os
import time
from typing import Dict, Any, Optional
from decimal import Decimal
from .base import BaseOperator

# Import our comprehensive type system
from bot_types import ExchangeName, ErrorCode, Result, create_success, create_error
from .types import (
    OrderResult, OrderRequest, OrderResponse, OrderSide, OrderType, 
    TimeInForce, OrderStatus, ClosePositionResult
)

class ExtendedOperator(BaseOperator):
    """
    Trading operator for Extended exchange using the official SDK.
    
    Handles order creation, position management, and trade execution
    using the x10-python-trading SDK.
    """
    
    def __init__(self, api_key: Optional[str] = None, stark_private_key: Optional[str] = None, 
                 stark_public_key: Optional[str] = None, vault: Optional[int] = None) -> None:
        """
        Initialize Extended operator.
        
        Args:
            api_key: Extended API key from api-management
            stark_private_key: Stark private key from api-management  
            stark_public_key: Stark public key from api-management
            vault: Vault number from api-management
        """
        self.api_key: Optional[str] = api_key or os.getenv("EXTENDED_API_KEY")
        self.stark_private_key: Optional[str] = stark_private_key or os.getenv("EXTENDED_STARK_PRIVATE_KEY")
        self.stark_public_key: Optional[str] = stark_public_key or os.getenv("EXTENDED_STARK_PUBLIC_KEY")
        self.vault: Optional[int] = vault or int(os.getenv("EXTENDED_VAULT", "109221"))
        self.is_testing: bool = os.getenv("TRADING_ENV", "testing").lower() == "testing"
        
        # Initialize SDK client
        self.client = None
        self._initialize_sdk()
    
    def _initialize_sdk(self) -> None:
        """Initialize the Extended SDK client."""
        try:
            from x10.perpetual.trading_client import PerpetualTradingClient
            from x10.perpetual.accounts import StarkPerpetualAccount
            from x10.perpetual.configuration import MAINNET_CONFIG
            from x10.perpetual.simple_client.simple_trading_client import BlockingTradingClient
            
            if not self.api_key:
                print("❌ No API key configured for Extended")
                return
                
            if not self.stark_private_key:
                print("❌ No Stark private key configured for Extended")
                return
                
            if not self.stark_public_key:
                print("❌ No Stark public key configured for Extended")
                return
                
            if not self.vault:
                print("❌ No vault configured for Extended")
                return
            
            # Create StarkPerpetualAccount
            self.stark_account = StarkPerpetualAccount(
                vault=self.vault,
                private_key=self.stark_private_key,
                public_key=self.stark_public_key,
                api_key=self.api_key,
            )
            
            # Create BlockingTradingClient (synchronous)
            self.client = BlockingTradingClient(
                endpoint_config=MAINNET_CONFIG,
                account=self.stark_account,
            )
            
            print("✅ Extended SDK initialized successfully")
            
        except ImportError as e:
            print(f"❌ Failed to import Extended SDK: {e}")
            print("Please install the SDK: pip install x10-python-trading")
        except Exception as e:
            print(f"❌ Failed to initialize Extended SDK: {e}")
    

    def create_order(self, symbol: str, is_buy: bool, size: str, price: Optional[str] = None) -> OrderResult:
        """
        Create an order on Extended exchange using the SDK.
        
        Args:
            symbol: Trading symbol (e.g., 'BTC')
            is_buy: True for buy order, False for sell order
            size: Order size in base asset
            price: Optional limit price for limit orders
            
        Returns:
            Result containing order information or error
        """
        if not self.client:
            return create_error(
                ErrorCode.AUTHENTICATION_FAILED,
                "Extended SDK not initialized. Check API key and Stark key.",
                ExchangeName.EXTENDED
            )
        
        try:
            # For testing mode, return mock response
            if self.is_testing:
                return create_success({
                    "status": "testing",
                    "exchange": ExchangeName.EXTENDED.value,
                    "symbol": symbol,
                    "is_buy": is_buy,
                    "size": size,
                    "price": price,
                    "result": {"order_id": f"test_{int(time.time())}", "status": "pending"},
                    "timestamp": int(time.time())
                })
            
            import asyncio
            from x10.perpetual.orders import OrderSide as SDKOrderSide
            
            # Convert symbol to market format
            market_name = f"{symbol}-USD"
            
            # Convert parameters to SDK format
            side = SDKOrderSide.BUY if is_buy else SDKOrderSide.SELL
            amount_of_synthetic = Decimal(size)
            
            # Create order using SDK
            print(f"Creating {side.value} order for {amount_of_synthetic} {symbol} at {price or 'market'}")
            
            # Use BlockingTradingClient.create_and_place_order
            if price:
                # Limit order
                placed_order = asyncio.run(self.client.create_and_place_order(
                    market_name=market_name,
                    amount_of_synthetic=amount_of_synthetic,
                    price=Decimal(price),
                    side=side,
                    post_only=False,
                ))
            else:
                # Market order - need to get current market price
                # For now, raise an error as market orders need current price
                return create_error(
                    ErrorCode.INVALID_PARAMETERS,
                    "Market orders not supported yet - please provide a price",
                    ExchangeName.EXTENDED
                )
            
            print(f"✅ Order placed: {placed_order.id} - {side.value} {amount_of_synthetic} {symbol} at {price}")
            
            return create_success({
                "exchange": ExchangeName.EXTENDED.value,
                "symbol": symbol,
                "is_buy": is_buy,
                "size": size,
                "price": price,
                "result": {
                    "order_id": placed_order.id,
                    "status": "placed",
                    "market": market_name,
                    "side": side.value,
                    "amount_of_synthetic": str(amount_of_synthetic),
                    "price": str(placed_order.price) if hasattr(placed_order, 'price') else price,
                    "filled_amount": str(placed_order.filled_amount) if hasattr(placed_order, 'filled_amount') else "0",
                    "remaining_amount": str(placed_order.remaining_amount) if hasattr(placed_order, 'remaining_amount') else str(amount_of_synthetic),
                },
                "timestamp": int(time.time())
            })
                
        except Exception as e:
            error_msg = str(e)
            
            # Handle specific Extended API errors
            if "1101" in error_msg:
                error_code = ErrorCode.AUTHENTICATION_FAILED
                message = "Invalid StarkEx signature"
            elif "1100" in error_msg:
                error_code = ErrorCode.AUTHENTICATION_FAILED
                message = "Invalid StarkEx public key"
            elif "1102" in error_msg:
                error_code = ErrorCode.AUTHENTICATION_FAILED
                message = "Invalid StarkEx vault"
            elif "401" in error_msg:
                error_code = ErrorCode.AUTHENTICATION_FAILED
                message = "Authentication failed - check API key"
            elif "400" in error_msg:
                error_code = ErrorCode.INVALID_ORDER_SIZE
                message = "Invalid order parameters"
            else:
                error_code = ErrorCode.UNKNOWN_ERROR
                message = f"Order creation failed: {error_msg}"
            
            return create_error(
                error_code,
                message,
                ExchangeName.EXTENDED,
                {"symbol": symbol, "is_buy": is_buy, "size": size, "price": price}
            )
    
    def close_position(self, symbol: str) -> ClosePositionResult:
        """
        Close position for a specific symbol using the SDK.
        
        Args:
            symbol: Trading symbol to close position for
            
        Returns:
            Result containing close position information or error
        """
        if not self.client:
            return create_error(
                ErrorCode.AUTHENTICATION_FAILED,
                "Extended SDK not initialized",
                ExchangeName.EXTENDED
            )
            
        try:
            import asyncio
            from x10.perpetual.orders import OrderSide as SDKOrderSide
            
            # Get current positions
            positions_response = asyncio.run(self.client.get_positions())
            
            # Find position for the symbol
            market_name = f"{symbol}-USD"
            position = None
            
            for pos in positions_response.data:
                if pos.market == market_name and abs(pos.size) > 0:
                    position = pos
                    break
            
            if not position:
                return create_error(
                    ErrorCode.POSITION_NOT_FOUND,
                    f"No open position found for {symbol}",
                    ExchangeName.EXTENDED
                )
            
            # Create closing order (opposite side)
            side = SDKOrderSide.SELL if position.size > 0 else SDKOrderSide.BUY
            amount_of_synthetic = abs(position.size)
            
            # Use current mark price for closing (could also use best bid/ask)
            close_price = position.mark_price
            
            # Place closing order
            close_order = asyncio.run(self.client.create_and_place_order(
                market_name=market_name,
                amount_of_synthetic=amount_of_synthetic,
                price=close_price,
                side=side,
                post_only=False,
            ))
            
            print(f"✅ Position closed: {close_order.id} - {side.value} {amount_of_synthetic} {symbol} at {close_price}")
            
            return create_success({
                "exchange": ExchangeName.EXTENDED.value,
                "symbol": symbol,
                "result": {
                    "order_id": close_order.id,
                    "status": "placed",
                    "side": side.value,
                    "amount_of_synthetic": str(amount_of_synthetic),
                    "price": str(close_price),
                    "original_position_size": str(position.size)
                },
                "timestamp": int(time.time())
            })
            
        except Exception as e:
            return create_error(
                ErrorCode.UNKNOWN_ERROR,
                f"Failed to close position: {str(e)}",
                ExchangeName.EXTENDED
            )
    
    def get_account_info(self) -> Dict[str, Any]:
        """
        Get account information using the SDK.
        
        Returns:
            Dictionary containing account information
        """
        if not self.client:
            return {
                "error": "Extended SDK not initialized",
                "balances": {},
                "positions": [],
                "account_value": 0.0,
                "margin_used": 0.0,
                "withdrawable": 0.0
            }
        
        try:
            import asyncio
            
            # Get positions and balance data using SDK
            positions_response = asyncio.run(self.client.get_positions())
            balance_response = asyncio.run(self.client.get_balance())
            
            # Convert positions to our format
            position_data = []
            for position in positions_response.data:
                if abs(position.size) > 0:  # Only include open positions
                    position_data.append({
                        "market": position.market,
                        "side": "LONG" if position.size > 0 else "SHORT",
                        "size": float(abs(position.size)),
                        "mark_price": float(position.mark_price),
                        "leverage": float(position.leverage),
                        "pnl": float(position.pnl) if hasattr(position, 'pnl') else 0.0
                    })
            
            # Extract balance information
            balance_data = {
                "USDC": {
                    "available": float(balance_response.available) if hasattr(balance_response, 'available') else 0.0,
                    "locked": float(balance_response.locked) if hasattr(balance_response, 'locked') else 0.0,
                    "total": float(balance_response.total) if hasattr(balance_response, 'total') else 0.0
                }
            }
            
            return {
                "exchange": ExchangeName.EXTENDED.value,
                "balances": balance_data,
                "positions": position_data,
                "account_value": balance_data["USDC"]["total"],
                "margin_used": 0.0,  # Would need to calculate from positions
                "withdrawable": balance_data["USDC"]["available"],
                "total_position_value": sum(pos["size"] * pos["mark_price"] for pos in position_data),
                "open_positions": len(position_data)
            }
            
        except Exception as e:
            return {
                "error": f"Failed to get account info: {str(e)}",
                "balances": {},
                "positions": [],
                "account_value": 0.0,
                "margin_used": 0.0,
                "withdrawable": 0.0
            }
    
    def cancel_order(self, order_id: str) -> Result:
        """
        Cancel an order on Extended exchange.
        
        Args:
            order_id: Order ID to cancel
            
        Returns:
            Result containing cancel confirmation or error
        """
        if not self.client:
            return create_error(
                ErrorCode.AUTHENTICATION_FAILED,
                "Extended SDK not initialized",
                ExchangeName.EXTENDED
            )
            
        try:
            import asyncio
            
            # Cancel the order
            asyncio.run(self.client.cancel_order(order_id=order_id))
            
            print(f"✅ Order cancelled: {order_id}")
            
            return create_success({
                "exchange": ExchangeName.EXTENDED.value,
                "result": {
                    "order_id": order_id,
                    "status": "cancelled"
                },
                "timestamp": int(time.time())
            })
            
        except Exception as e:
            return create_error(
                ErrorCode.UNKNOWN_ERROR,
                f"Cancel order failed: {str(e)}",
                ExchangeName.EXTENDED
            )
    
    def get_positions(self) -> Result:
        """
        Get current positions on Extended exchange.
        
        Returns:
            Result containing positions data or error
        """
        if not self.client:
            return create_error(
                ErrorCode.AUTHENTICATION_FAILED,
                "Extended SDK not initialized",
                ExchangeName.EXTENDED
            )
            
        try:
            import asyncio
            
            # Get positions
            positions_response = asyncio.run(self.client.get_positions())
            
            positions_data = []
            for pos in positions_response.data:
                positions_data.append({
                    "market": pos.market,
                    "side": pos.side,
                    "size": str(pos.size),
                    "mark_price": str(pos.mark_price),
                    "leverage": str(pos.leverage),
                    "pnl": str(pos.pnl) if hasattr(pos, 'pnl') else "0"
                })
            
            return create_success({
                "exchange": ExchangeName.EXTENDED.value,
                "result": {
                    "positions": positions_data,
                    "count": len(positions_data)
                },
                "timestamp": int(time.time())
            })
            
        except Exception as e:
            return create_error(
                ErrorCode.UNKNOWN_ERROR,
                f"Get positions failed: {str(e)}",
                ExchangeName.EXTENDED
            ) 