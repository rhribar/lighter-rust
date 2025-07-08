"""
Base Operator Interface

Defines the common interface for all exchange operators with proper typing.
"""

from abc import ABC, abstractmethod
from typing import Dict, Any, Optional

# Import our comprehensive type system
from bot_types import ExchangeName, ErrorCode, Result
from .types import OrderResult, ClosePositionResult

class BaseOperator(ABC):
    """
    Base class for all exchange operators.
    
    Provides the interface that all trading operators must implement
    for creating orders and managing positions.
    """
    
    @abstractmethod
    def create_order(
        self, 
        symbol: str, 
        is_buy: bool, 
        size: str, 
        price: Optional[str] = None, 
        **kwargs: Any
    ) -> OrderResult:
        """
        Create an order on the exchange.
        
        Args:
            symbol: Trading symbol (e.g., 'BTC')
            is_buy: True for buy order, False for sell order
            size: Order size in base asset
            price: Optional limit price for limit orders
            **kwargs: Additional exchange-specific parameters
            
        Returns:
            Result containing order information or error
        """
        pass
    
    @abstractmethod
    def close_position(self, symbol: str) -> ClosePositionResult:
        """
        Close position for a specific symbol.
        
        Args:
            symbol: Trading symbol to close position for
            
        Returns:
            Result containing close position information or error
        """
        pass
