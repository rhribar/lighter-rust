"""
Hyperliquid Utilities

Utility functions for signing and authentication with Hyperliquid exchange.
"""

import msgpack
from eth_utils import keccak
from eth_account import Account
from typing import Optional, Dict, Any, Union
from eth_keys import keys

def address_to_bytes(address: str) -> bytes:
    """
    Convert hex address to bytes.
    
    Args:
        address: Hex address string (with or without 0x prefix)
        
    Returns:
        Address as bytes
    """
    return bytes.fromhex(address[2:])

def action_hash(
    action: Dict[str, Any], 
    vault_address: Optional[str], 
    nonce: int, 
    expires_after: Optional[int] = None
) -> bytes:
    """
    Create action hash using Hyperliquid's method.
    
    Args:
        action: Trading action dictionary
        vault_address: Optional vault address
        nonce: Transaction nonce
        expires_after: Optional expiration timestamp
        
    Returns:
        Keccak hash of the action data
    """
    data: bytes = msgpack.packb(action)
    data += nonce.to_bytes(8, "big")
    
    if vault_address is None:
        data += b"\x00"
    else:
        data += b"\x01"
        data += address_to_bytes(vault_address)
    
    if expires_after is not None:
        data += b"\x00"
        data += expires_after.to_bytes(8, "big")
    
    return keccak(data)

def construct_phantom_agent(hash: bytes, is_mainnet: bool = True) -> Dict[str, str]:
    """
    Create phantom agent for signing.
    
    Args:
        hash: Action hash bytes
        is_mainnet: Whether to use mainnet configuration
        
    Returns:
        Dictionary containing phantom agent data
    """
    return {
        "source": "a" if is_mainnet else "b", 
        "connectionId": "0x" + hash.hex()
    }

def sign_l1_action(
    private_key: str, 
    action: Dict[str, Any], 
    vault_address: Optional[str], 
    nonce: int, 
    expires_after: Optional[int] = None, 
    is_mainnet: bool = True
) -> Dict[str, Union[str, int]]:
    """
    Sign L1 action using Hyperliquid's official method.
    
    Args:
        private_key: Private key for signing (hex string)
        action: Trading action dictionary
        vault_address: Optional vault address
        nonce: Transaction nonce
        expires_after: Optional expiration timestamp
        is_mainnet: Whether to use mainnet configuration
        
    Returns:
        Dictionary containing signature components (r, s, v)
    """
    hash: bytes = action_hash(action, vault_address, nonce, expires_after)
    phantom_agent: Dict[str, str] = construct_phantom_agent(hash, is_mainnet)
    
    # EIP-712 domain and types for Hyperliquid
    domain_separator: bytes = keccak(
        keccak(b"EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)") +
        keccak(b"Exchange") +
        keccak(b"1") +
        (1337).to_bytes(32, 'big') +
        bytes.fromhex("0000000000000000000000000000000000000000000000000000000000000000")
    )
    
    # Agent type hash and message hash
    agent_type_hash: bytes = keccak(b"Agent(string source,bytes32 connectionId)")
    message_hash: bytes = keccak(
        agent_type_hash +
        keccak(phantom_agent["source"].encode()) +
        bytes.fromhex(phantom_agent["connectionId"][2:])
    )
    
    # Final EIP-712 hash
    final_hash: bytes = keccak(b"\x19\x01" + domain_separator + message_hash)
    
    # Sign using the private key directly
    private_key_obj = keys.PrivateKey(bytes.fromhex(private_key[2:]))
    signature = private_key_obj.sign_msg_hash(final_hash)
    
    return {
        "r": hex(signature.r),
        "s": hex(signature.s), 
        "v": signature.v
    } 