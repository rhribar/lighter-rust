import logging
import os
from typing import Dict, List, Any
from datetime import datetime
from fetchers.extended import ExtendedFetcher
from operators.extended import ExtendedOperator
from dotenv import load_dotenv

load_dotenv()

# Extended uses API key authentication instead of wallet address
EXTENDED_API_KEY = os.getenv("EXTENDED_API_KEY")
EXTENDED_STARK_KEY = os.getenv("EXTENDED_STARK_KEY")  # Required for trading
TRADING_ENV = os.getenv("TRADING_ENV", "testing")

class PointsBot:
    def __init__(self, api_key: str = None, stark_key: str = None):
        self.api_key = api_key or EXTENDED_API_KEY
        self.stark_key = stark_key or EXTENDED_STARK_KEY
        self.fetcher = ExtendedFetcher(self.api_key)
        self.operator = ExtendedOperator(self.api_key, self.stark_key)
        
    def get_points_data(self):
        # Extended doesn't use wallet addresses, just API key
        return self.fetcher.get_account_data("extended_account")
    
    def get_positions(self):
        return self.fetcher.get_user_positions("extended_account")
    
    def get_account_balances(self):
        """Get detailed account balance information"""
        positions_data = self.get_positions()
        margin_summary = positions_data.get('margin_summary', {})
        
        return {
            "account_value": float(margin_summary.get('accountValue', '0')),
            "total_margin_used": float(margin_summary.get('totalMarginUsed', '0')),
            "total_ntl_pos": float(margin_summary.get('totalNtlPos', '0')),
            "total_raw_usd": float(margin_summary.get('totalRawUsd', '0')),
            "withdrawable": float(positions_data.get('withdrawable', '0')),
            "available_balance": float(margin_summary.get('accountValue', '0')) - float(margin_summary.get('totalMarginUsed', '0')),
            "positions_count": len(positions_data.get('positions', [])),
            "exchange": "extended",
            "timestamp": positions_data.get('timestamp', 0)
        }
        
    def get_supported_tokens(self):
        return self.fetcher.get_supported_tokens()
        
    def get_funding_rates(self):
        return self.fetcher.get_funding_rates()
        
    def check_funding_arbitrage(self):
        funding_data = self.get_funding_rates()
        
        if "error" in funding_data:
            return {"error": funding_data["error"]}
            
        opportunities = []
        funding_rates = funding_data["funding_rates"]
        
        for coin, data in funding_rates.items():
            funding_rate_8h = data["funding_rate_8h"]
            
            if abs(funding_rate_8h) > 0.01:  # 1% threshold
                opportunities.append({
                    "coin": coin,
                    "funding_rate_8h": funding_rate_8h,
                    "mark_price": data["mark_price"],
                    "direction": "short" if funding_rate_8h > 0 else "long",
                    "exchange": "extended"
                })
        
        return {
            "opportunities": opportunities,
            "total_opportunities": len(opportunities),
            "timestamp": funding_data["timestamp"]
        }
    
    def create_order(self, symbol: str, is_buy: bool, size: str, price: str = None):
        """Create order on Extended (symbol like 'BTC', size in base asset)"""
        return self.operator.create_order(symbol, is_buy, size, price)
    
    def close_position(self, symbol: str):
        return self.operator.close_position(symbol)

def main():
    bot = PointsBot()
    
    print("=== EXTENDED EXCHANGE ===")
    print(f"API Key: {'✅ Configured' if EXTENDED_API_KEY else '❌ Missing'}")
    print(f"Stark Key: {'✅ Configured' if EXTENDED_STARK_KEY else '❌ Missing'}")
    print(f"Trading Environment: {TRADING_ENV}")
    print()
    
    print("=== ACCOUNT BALANCES ===")
    balances = bot.get_account_balances()
    if "error" in balances:
        print(f"Error: {balances['error']}")
    else:
        print(f"Account Value: ${balances['account_value']:.2f}")
        print(f"Available Balance: ${balances['available_balance']:.2f}")
        print(f"Margin Used: ${balances['total_margin_used']:.2f}")
        print(f"Withdrawable: ${balances['withdrawable']:.2f}")
        print(f"Total Position Value: ${balances['total_ntl_pos']:.2f}")
        print(f"Open Positions: {balances['positions_count']}")
    print()
    
    print("=== SUPPORTED TOKENS ===")
    tokens = bot.get_supported_tokens()
    print(f"Found {len(tokens)} supported tokens")
    if tokens:
        print(f"First 10: {tokens[:10]}")
    print()
    
    print("=== FUNDING RATES ===")
    funding_data = bot.get_funding_rates()
    if "error" in funding_data:
        print(f"Error: {funding_data['error']}")
    else:
        funding_rates = funding_data["funding_rates"]
        print(f"Found funding rates for {len(funding_rates)} tokens")
        
        # Show top 5 highest funding rates
        sorted_rates = sorted(funding_rates.items(), key=lambda x: abs(x[1]['funding_rate_8h']), reverse=True)[:5]
        for coin, data in sorted_rates:
            print(f"{coin}: {data['funding_rate_8h']:.4f} (8h), Mark: ${data['mark_price']:.2f}")
    print()
    
    print("=== ARBITRAGE OPPORTUNITIES ===")
    opportunities = bot.check_funding_arbitrage()
    if "error" in opportunities:
        print(f"Error: {opportunities['error']}")
    else:
        print(f"Found {opportunities['total_opportunities']} opportunities")
        for opp in opportunities['opportunities'][:5]:
            print(f"{opp['coin']}: {opp['funding_rate_8h']:.4f} - {opp['direction']} at ${opp['mark_price']:.2f}")
    print()
    
    print("=== TRADING TEST ===")
    if EXTENDED_STARK_KEY:
        # Test small BTC order
        result = bot.create_order("BTC", True, "0.001", "100000")
        print(f"Order result: {result}")
    else:
        print("⚠️  No Stark key - trading disabled")

if __name__ == "__main__":
    main()
