from abc import ABC, abstractmethod
from typing import Dict, Any

class BaseOperator(ABC):
    
    @abstractmethod
    def create_order(self, asset_id: int, is_buy: bool, size: str, price: str, **kwargs) -> Dict[str, Any]:
        pass
