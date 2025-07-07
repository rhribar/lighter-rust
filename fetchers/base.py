"""
Base Fetcher Interface

Defines the common interface for all exchange fetchers with proper typing.
"""

import logging
import time
import requests
from abc import ABC, abstractmethod
from typing import Protocol, runtime_checkable

# Import our type system
from bot_types import (
    ExchangeName, ErrorCode, Result,
    create_success, create_error
)
from .types import (
    AccountBalanceResult, FundingRatesResult, PositionsResult, 
    TokenListResult
)

@runtime_checkable
class FetcherProtocol(Protocol):
    """Protocol defining the interface all fetchers must implement"""
    
    def get_account_data(self, address: str) -> AccountBalanceResult:
        """Get account balance and summary data"""
        ...
    
    def get_user_positions(self, address: str) -> PositionsResult:
        """Get user positions"""
        ...
    
    def get_supported_tokens(self) -> TokenListResult:
        """Get list of supported trading pairs"""
        ...
    
    def get_funding_rates(self) -> FundingRatesResult:
        """Get funding rates for all supported pairs"""
        ...

class BaseFetcher(ABC):
    """
    Base class for all exchange fetchers
    
    Provides common functionality like rate limiting, session management,
    and error handling with proper typing.
    """
    
    def __init__(self, name: ExchangeName, base_url: str, rate_limit: float = 0.1):
        self.name = name
        self.base_url = base_url
        self.rate_limit = rate_limit
        self.last_request_time = 0
        
        # Set up session and logging
        self.session = requests.Session()
        self.logger = logging.getLogger(f"fetcher.{name.value}")
        
        # Default headers
        self.session.headers.update({
            'Content-Type': 'application/json',
            'Accept': 'application/json'
        })
    
    def _rate_limit(self) -> None:
        """Enforce rate limiting between requests"""
        current_time = time.time()
        time_since_last = current_time - self.last_request_time
        
        if time_since_last < self.rate_limit:
            sleep_time = self.rate_limit - time_since_last
            time.sleep(sleep_time)
        
        self.last_request_time = time.time()
    
    def _safe_request(self, method: str, url: str, **kwargs) -> Result:
        """Make a safe HTTP request with error handling"""
        try:
            self._rate_limit()
            response = self.session.request(method, url, **kwargs)
            response.raise_for_status()
            return create_success(response.json())
            
        except requests.exceptions.HTTPError as e:
            if e.response.status_code == 401:
                return create_error(
                    ErrorCode.AUTHENTICATION_FAILED,
                    f"Authentication failed for {self.name.value}",
                    self.name,
                    {"status_code": e.response.status_code}
                )
            elif e.response.status_code == 404:
                return create_error(
                    ErrorCode.API_ERROR,
                    f"API endpoint not found: {url}",
                    self.name,
                    {"status_code": e.response.status_code}
                )
            else:
                return create_error(
                    ErrorCode.API_ERROR,
                    f"HTTP error: {str(e)}",
                    self.name,
                    {"status_code": e.response.status_code}
                )
                
        except requests.exceptions.ConnectionError as e:
            return create_error(
                ErrorCode.NETWORK_ERROR,
                f"Network connection failed: {str(e)}",
                self.name
            )
            
        except requests.exceptions.Timeout as e:
            return create_error(
                ErrorCode.NETWORK_ERROR,
                f"Request timeout: {str(e)}",
                self.name
            )
            
        except Exception as e:
            return create_error(
                ErrorCode.UNKNOWN_ERROR,
                f"Unexpected error: {str(e)}",
                self.name
            )
    
    @abstractmethod
    def get_account_data(self, address: str) -> AccountBalanceResult:
        """Get account balance and summary data"""
        pass
    
    @abstractmethod
    def get_user_positions(self, address: str) -> PositionsResult:
        """Get user positions"""
        pass
    
    @abstractmethod
    def get_supported_tokens(self) -> TokenListResult:
        """Get list of supported trading pairs"""
        pass
    
    @abstractmethod
    def get_funding_rates(self) -> FundingRatesResult:
        """Get funding rates for all supported pairs"""
        pass

    def __str__(self):
        return f"{self.name.value}Fetcher"
        
    def __repr__(self):
        return f"{self.__class__.__name__}(name='{self.name.value}')" 