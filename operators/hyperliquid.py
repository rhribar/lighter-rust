import requests
import time
import json
import os
from typing import Dict, Any
from .base import BaseOperator
from .utils import sign_l1_action

class HyperliquidOperator(BaseOperator):
    def __init__(self, wallet_address: str, private_key: str):
        self.wallet_address = wallet_address
        self.private_key = private_key
        self.base_url = "https://api.hyperliquid.xyz"
        self.session = requests.Session()
        self.is_testing = os.getenv("TRADING_ENV", "testing").lower() == "testing"
        
    def _get_nonce(self) -> int:
        return int(time.time() * 1000)
    
    def create_order(self, asset_id: int, is_buy: bool, size: str, price: str) -> Dict[str, Any]:
        if not self.private_key:
            return {"error": "No private key configured"}
            
        action = {
            "type": "order",
            "orders": [{
                "a": asset_id,
                "b": is_buy,
                "p": price,
                "s": size,
                "r": False,
                "t": {"limit": {"tif": "Gtc"}}
            }],
            "grouping": "na"
        }
        
        nonce = self._get_nonce()
        
        try:
            signature = sign_l1_action(
                private_key=self.private_key,
                action=action,
                vault_address=None,
                nonce=nonce,
                expires_after=None,
                is_mainnet=True
            )
            
            payload = {
                "action": action,
                "nonce": nonce,
                "signature": signature
            }

            response = self.session.post(f"{self.base_url}/exchange", json=payload)
            result = response.json()
            
            return {
                "status": "real",
                "exchange": "hyperliquid",
                "asset_id": asset_id,
                "is_buy": is_buy,
                "size": size,
                "price": price,
                "result": result,
                "timestamp": int(time.time()),
            }
        except Exception as e:
            return {"error": str(e)}
    
    def close_position(self, asset_id: int) -> Dict[str, Any]:
        return {"error": "Close position not implemented yet"} 