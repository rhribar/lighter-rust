import msgpack
from eth_utils import keccak
from eth_account import Account
from typing import Optional

def address_to_bytes(address: str) -> bytes:
    """Convert hex address to bytes"""
    return bytes.fromhex(address[2:])

def action_hash(action: dict, vault_address: Optional[str], nonce: int, expires_after: Optional[int]) -> bytes:
    """Create action hash using Hyperliquid's method"""
    data = msgpack.packb(action)
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

def construct_phantom_agent(hash: bytes, is_mainnet: bool = True) -> dict:
    """Create phantom agent for signing"""
    return {
        "source": "a" if is_mainnet else "b", 
        "connectionId": "0x" + hash.hex()
    }

def sign_l1_action(private_key: str, action: dict, vault_address: Optional[str], nonce: int, expires_after: Optional[int] = None, is_mainnet: bool = True) -> dict:
    """Sign L1 action using Hyperliquid's official method"""
    hash = action_hash(action, vault_address, nonce, expires_after)
    phantom_agent = construct_phantom_agent(hash, is_mainnet)
    
    # EIP-712 domain and types for Hyperliquid
    domain_separator = keccak(
        keccak(b"EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)") +
        keccak(b"Exchange") +
        keccak(b"1") +
        (1337).to_bytes(32, 'big') +
        bytes.fromhex("0000000000000000000000000000000000000000000000000000000000000000")
    )
    
    # Agent type hash and message hash
    agent_type_hash = keccak(b"Agent(string source,bytes32 connectionId)")
    message_hash = keccak(
        agent_type_hash +
        keccak(phantom_agent["source"].encode()) +
        bytes.fromhex(phantom_agent["connectionId"][2:])
    )
    
    # Final EIP-712 hash
    final_hash = keccak(b"\x19\x01" + domain_separator + message_hash)
    
    account = Account.from_key(private_key)
    
    # Sign using the private key directly
    from eth_keys import keys
    private_key_obj = keys.PrivateKey(bytes.fromhex(private_key[2:]))
    signature = private_key_obj.sign_msg_hash(final_hash)
    
    return {
        "r": hex(signature.r),
        "s": hex(signature.s), 
        "v": signature.v
    } 