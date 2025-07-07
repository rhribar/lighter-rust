# Points Bot - Exchange Fetchers

A modular Python application for fetching points and trading data from various cryptocurrency exchanges.

## Structure

```
points-bot/
├── fetchers/
│   ├── __init__.py          # Package initialization with imports
│   ├── base.py              # Base fetcher class
│   ├── hyperliquid.py       # Hyperliquid exchange fetcher
│   ├── binance.py           # Binance exchange fetcher
│   └── bybit.py             # Bybit exchange fetcher
├── PointsBot.py             # Main bot application
├── example_usage.py         # Usage examples
├── requirements.txt         # Dependencies
└── README.md               # This file
```

## Installation

1. Install dependencies:
```bash
pip install -r requirements.txt
```

## Usage

### Import Individual Fetchers

```python
# Method 1: Import from package
from fetchers import HyperliquidFetcher, BinanceFetcher, BybitFetcher

# Method 2: Import from specific modules
from fetchers.hyperliquid import HyperliquidFetcher
from fetchers.binance import BinanceFetcher
from fetchers.bybit import BybitFetcher
```

### Basic Usage

```python
# Initialize a specific fetcher
fetcher = HyperliquidFetcher()

# Get points data
wallet_address = "0x1234567890abcdef1234567890abcdef12345678"
points_data = fetcher.get_account_data(wallet_address)
print(points_data)

# Get supported tokens
tokens = fetcher.get_supported_tokens()
print(f"Supported tokens: {len(tokens)}")
```

### Using the Main Bot

```python
from PointsBot import PointsBot

# Initialize bot
bot = PointsBot("0x1234567890abcdef1234567890abcdef12345678")

# Get points from specific exchange
hyperliquid_points = bot.get_points_from_exchange('hyperliquid')

# Get points from all exchanges
all_points = bot.get_all_points()

# Monitor points
bot.monitor_points()
```

## Adding New Exchanges

To add a new exchange fetcher:

1. Create a new file in `fetchers/` (e.g., `okx.py`)
2. Inherit from `BaseFetcher` class
3. Implement required methods:
   - `get_account_data(address: str)`
   - `get_supported_tokens()`
4. Add import to `fetchers/__init__.py`

Example:

```python
# fetchers/okx.py
from .base import BaseFetcher
from typing import Dict, List, Any

class OkxFetcher(BaseFetcher):
    def __init__(self):
        super().__init__(
            name="okx",
            base_url="https://www.okx.com",
            rate_limit=0.1
        )
    
    def get_account_data(self, address: str) -> Dict[str, Any]:
        # Implementation here
        pass
    
    def get_supported_tokens(self) -> List[str]:
        # Implementation here
        pass
```

Then update `fetchers/__init__.py`:

```python
from .okx import OkxFetcher

__all__ = [
    'HyperliquidFetcher',
    'BinanceFetcher', 
    'BybitFetcher',
    'OkxFetcher'
]
```

## Features

- **Modular Design**: Each exchange is a separate module
- **Base Class**: Common functionality shared across all fetchers
- **Rate Limiting**: Built-in rate limiting to respect API limits
- **Error Handling**: Comprehensive error handling and logging
- **Type Hints**: Full type annotation support
- **Extensible**: Easy to add new exchanges

## Examples

Run the examples:

```bash
# Run the main bot
python PointsBot.py

# Run usage examples
python example_usage.py
```

## API Reference

### BaseFetcher

Base class for all exchange fetchers.

- `get_account_data(address: str)` - Get points data for an address
- `get_supported_tokens()` - Get list of supported tokens
- `_make_request(endpoint: str, params: dict)` - Make API request

### Exchange-Specific Methods

Each fetcher may have additional methods:

- **HyperliquidFetcher**: `get_user_positions(address: str)`
- **BinanceFetcher**: `get_user_balances(address: str)`
- **BybitFetcher**: `get_user_positions(address: str)`

## Configuration

You can configure each fetcher by modifying the initialization parameters:

```python
fetcher = HyperliquidFetcher()
fetcher.rate_limit = 0.5  # Adjust rate limit
fetcher.base_url = "https://api.hyperliquid.xyz"  # Change base URL
``` 