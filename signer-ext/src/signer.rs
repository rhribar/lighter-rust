use goldilocks_crypto::{schnorr::{sign_with_nonce},schnorr::verify_signature, ScalarField, Goldilocks};
use thiserror::Error;
use base64::Engine;
use serde_json::{json,Value};

#[derive(Error, Debug)]
pub enum SignerError {
    #[error("Crypto error: {0}")]
    Crypto(#[from] goldilocks_crypto::CryptoError),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("System time error: {0}")]
    SystemTime(#[from] std::time::SystemTimeError),
    #[error("Hex decode error: {0}")]
    HexDecode(#[from] hex::FromHexError),
    #[error("API error: {0}")]
    API(String),
}

pub type Result<T> = std::result::Result<T, SignerError>;

pub struct KeyManager {
    private_key: ScalarField,
}

impl KeyManager {
    pub fn new(private_key_bytes: &[u8]) -> Result<Self> {
        if private_key_bytes.len() != 40 {
            return Err(SignerError::Crypto(goldilocks_crypto::CryptoError::InvalidPrivateKeyLength(private_key_bytes.len())));
        }
        // Use all 40 bytes for 5-limb scalar
        let private_key = ScalarField::from_bytes_le(private_key_bytes)
            .map_err(|_| SignerError::Crypto(goldilocks_crypto::CryptoError::InvalidPrivateKeyLength(private_key_bytes.len())))?;
        Ok(Self { private_key })
    }
    
    pub fn from_hex(hex_str: &str) -> Result<Self> {
        let hex_str = if hex_str.starts_with("0x") {
            &hex_str[2..]
        } else {
            hex_str
        };
        
        let bytes = hex::decode(hex_str)?;
        Self::new(&bytes)
    }

    /// Get the public key as bytes (40 bytes)
    pub fn public_key_bytes(&self) -> [u8; 40] {
        use goldilocks_crypto::schnorr::Point;
        // Public key = generator * private_key, encoded as Fp5Element
        let generator = Point::generator();
        let public_point = generator.mul(&self.private_key);
        let public_fp5 = public_point.encode();
        public_fp5.to_bytes_le()
    }

    /// Get the private key as bytes (40 bytes)
    pub fn private_key_bytes(&self) -> [u8; 40] {
        self.private_key.to_bytes_le()
    }

    pub fn sign(&self, message: &[u8; 40]) -> Result<[u8; 80]> {
        // Generate cryptographically secure random nonce
        let nonce_scalar = ScalarField::sample_crypto();
        let nonce_bytes = nonce_scalar.to_bytes_le();
        self.sign_with_fixed_nonce(message, &nonce_bytes)
    }
    
    fn sign_with_fixed_nonce(&self, message: &[u8; 40], nonce_bytes: &[u8]) -> Result<[u8; 80]> {
        let pk_bytes = self.private_key.to_bytes_le();
        
        // Pass message directly - sign_with_nonce will convert it properly
        let signature = sign_with_nonce(&pk_bytes, message, nonce_bytes)?;
        let mut result = [0u8; 80];
        result.copy_from_slice(&signature);
        Ok(result)
    }
    
    pub fn create_auth_token(
        &self,
        deadline: i64,
        account_index: i64,
        api_key_index: u8,
        verify_sign:bool,
    ) -> Result<String> {
        // Match Go: ConstructAuthToken format "deadline:account_index:api_key_index"
        let auth_data = format!("{}:{}:{}", deadline, account_index, api_key_index);
        
        // Convert message bytes to Goldilocks elements
        let auth_bytes = auth_data.as_bytes();
        
        // CRITICAL: Pad each 8-byte chunk individually
        // Calculate missing bytes: (8 - len(in) % 8) % 8, then pad the last chunk with zeros
        let missing = (8 - auth_bytes.len() % 8) % 8;
        
        let mut elements = Vec::new();
        
        // Process in chunks of 8 bytes (one Goldilocks element per 8 bytes)
        let mut i = 0;
        while i < auth_bytes.len() {
            let next_start = (i + 8).min(auth_bytes.len());
            let chunk = &auth_bytes[i..next_start];
            
            let mut bytes = [0u8; 8];
            bytes[..chunk.len()].copy_from_slice(chunk);
            
            // Pad only the last chunk if needed
            if chunk.len() < 8 && missing > 0 {
                bytes[chunk.len()..].fill(0);
            }
            
            // Read as little-endian u64, then convert to Goldilocks
            let val = u64::from_le_bytes(bytes);
            elements.push(Goldilocks::from_canonical_u64(val));
            
            i = next_start;
        }
        
        // Hash the elements using Poseidon2 (matching Go's HashToQuinticExtension)
        use poseidon_hash::hash_to_quintic_extension;
        let hash_fp5 = hash_to_quintic_extension(&elements);
        
        // Convert Fp5Element to 40-byte array for signing
        let message_bytes = hash_fp5.to_bytes_le();
        
        // Sign the hash
        let signature = self.sign(&message_bytes)?;

        if verify_sign {
           
            let pubkey = self.private_key_bytes();
            let sig_ok = verify_signature(&signature,  &message_bytes, &pubkey).unwrap();

            if !sig_ok {
                // If signature doesn't match, return error (or log)
                return Err(SignerError::Crypto(
                    goldilocks_crypto::CryptoError::InvalidSignature
                ));
            }
        }
        let signature_hex = hex::encode(&signature);
        
        Ok(format!("{}:{}", auth_data, signature_hex))
    }

    pub fn sign_transaction(&self, tx_json: &str, tx_type: u32, lighter_chain_id: u32, verify_sign:bool) -> Result<String> {
                // Parse the transaction JSON to extract fields
        let tx_value: Value = serde_json::from_str(tx_json)?;

        // Determine chain ID based on base URL
        // Mainnet: 304, Testnet: 300
        let nonce = tx_value["Nonce"].as_i64().unwrap_or(0);
        let expired_at = tx_value["ExpiredAt"].as_i64().unwrap_or(0);
        let account_index = tx_value["AccountIndex"].as_i64().unwrap_or(0);
        let api_key_index = tx_value["ApiKeyIndex"].as_u64().unwrap_or(0) as u32;

        use poseidon_hash::Goldilocks;

        // Helper function to convert signed i64 to Goldilocks field element
        // Handles sign extension properly for negative values
        let to_goldi_i64 = |val: i64| Goldilocks::from_i64(val);

        let elements = match tx_type {
            14 => {
                // CREATE_ORDER: 16 elements
        let market_index = tx_value["MarketIndex"].as_u64().unwrap_or(0) as u32;
        let client_order_index = tx_value["ClientOrderIndex"].as_i64().unwrap_or(0);
        let base_amount = tx_value["BaseAmount"].as_i64().unwrap_or(0);
        let price = tx_value["Price"]
            .as_u64()
            .or_else(|| tx_value["Price"].as_i64().map(|v| v as u64))
            .unwrap_or(0) as u32;
        let is_ask = tx_value["IsAsk"]
            .as_u64()
            .or_else(|| tx_value["IsAsk"].as_i64().map(|v| v as u64))
            .unwrap_or(0) as u32;
        let order_type = tx_value["Type"]
            .as_u64()
            .or_else(|| tx_value["Type"].as_i64().map(|v| v as u64))
            .unwrap_or(0) as u32;
        let time_in_force = tx_value["TimeInForce"]
            .as_u64()
            .or_else(|| tx_value["TimeInForce"].as_i64().map(|v| v as u64))
            .unwrap_or(0) as u32;
        let reduce_only = tx_value["ReduceOnly"]
            .as_u64()
            .or_else(|| tx_value["ReduceOnly"].as_i64().map(|v| v as u64))
            .unwrap_or(0) as u32;
        let trigger_price = tx_value["TriggerPrice"]
            .as_u64()
            .or_else(|| tx_value["TriggerPrice"].as_i64().map(|v| v as u64))
            .unwrap_or(0) as u32;
        let order_expiry = tx_value["OrderExpiry"].as_i64().unwrap_or(0);
        
                vec![
                    Goldilocks::from_canonical_u64(lighter_chain_id as u64),
                    Goldilocks::from_canonical_u64(tx_type as u64),
                    to_goldi_i64(nonce),
                    to_goldi_i64(expired_at),
                    to_goldi_i64(account_index),
                    Goldilocks::from_canonical_u64(api_key_index as u64),
                    Goldilocks::from_canonical_u64(market_index as u64),
                    to_goldi_i64(client_order_index),
                    to_goldi_i64(base_amount),
                    Goldilocks::from_canonical_u64(price as u64),
                    Goldilocks::from_canonical_u64(is_ask as u64),
                    Goldilocks::from_canonical_u64(order_type as u64),
                    Goldilocks::from_canonical_u64(time_in_force as u64),
                    Goldilocks::from_canonical_u64(reduce_only as u64),
                    Goldilocks::from_canonical_u64(trigger_price as u64),
                    to_goldi_i64(order_expiry),
                ]
            }
            15 => {
                // CANCEL_ORDER: 8 elements
                let market_index = tx_value["MarketIndex"].as_u64().unwrap_or(0) as u32;
                let order_index = tx_value["Index"].as_i64().unwrap_or(0);

                vec![
                    Goldilocks::from_canonical_u64(lighter_chain_id as u64),
                    Goldilocks::from_canonical_u64(tx_type as u64),
                    to_goldi_i64(nonce),
                    to_goldi_i64(expired_at),
                    to_goldi_i64(account_index),
                    Goldilocks::from_canonical_u64(api_key_index as u64),
                    Goldilocks::from_canonical_u64(market_index as u64),
                    to_goldi_i64(order_index),
                ]
            }
            16 => {
                // CANCEL_ALL_ORDERS: 8 elements
                let time_in_force = tx_value["TimeInForce"]
                    .as_u64()
                    .or_else(|| tx_value["TimeInForce"].as_i64().map(|v| v as u64))
                    .unwrap_or(0) as u32;
                let time = tx_value["Time"].as_i64().unwrap_or(0);

                vec![
                    Goldilocks::from_canonical_u64(lighter_chain_id as u64),
                    Goldilocks::from_canonical_u64(tx_type as u64),
                    to_goldi_i64(nonce),
                    to_goldi_i64(expired_at),
                    to_goldi_i64(account_index),
                    Goldilocks::from_canonical_u64(api_key_index as u64),
                    Goldilocks::from_canonical_u64(time_in_force as u64),
                    to_goldi_i64(time),
                ]
            }
            8 => {
                // CHANGE_PUB_KEY: needs pubkey parsing (ArrayFromCanonicalLittleEndianBytes)
                let pubkey_hex = tx_value["PubKey"].as_str().unwrap_or("");
                let pubkey_bytes = hex::decode(pubkey_hex)
                    .map_err(|e| SignerError::API(format!("Invalid PubKey hex: {}", e)))?;
                if pubkey_bytes.len() != 40 {
                    return Err(SignerError::API("PubKey must be 40 bytes".to_string()));
                }
                // Convert 40-byte public key to 5 Goldilocks elements (8 bytes per element)
                let mut pubkey_elems = Vec::new();
                for i in 0..5 {
                    let chunk = &pubkey_bytes[i*8..(i+1)*8];
                    let val = u64::from_le_bytes(chunk.try_into().unwrap());
                    pubkey_elems.push(Goldilocks::from_canonical_u64(val));
                }

                let mut elems = vec![
                    Goldilocks::from_canonical_u64(lighter_chain_id as u64),
                    Goldilocks::from_canonical_u64(tx_type as u64),
                    to_goldi_i64(nonce),
                    to_goldi_i64(expired_at),
                    to_goldi_i64(account_index),
                    Goldilocks::from_canonical_u64(api_key_index as u64),
                ];
                elems.extend(pubkey_elems);
                elems
            }
            20 => {
                // UPDATE_LEVERAGE: 9 elements
                // Order: lighterChainId, txType, nonce, expiredAt, accountIndex, apiKeyIndex, marketIndex, initialMarginFraction, marginMode
                let market_index = tx_value["MarketIndex"]
                    .as_u64()
                    .or_else(|| tx_value["MarketIndex"].as_i64().map(|v| v as u64))
                    .unwrap_or(0) as u32;
                let initial_margin_fraction = tx_value["InitialMarginFraction"]
                    .as_u64()
                    .or_else(|| tx_value["InitialMarginFraction"].as_i64().map(|v| v as u64))
                    .unwrap_or(0) as u32;
                let margin_mode = tx_value["MarginMode"]
                    .as_u64()
                    .or_else(|| tx_value["MarginMode"].as_i64().map(|v| v as u64))
                    .unwrap_or(0) as u32;

                vec![
                    Goldilocks::from_canonical_u64(lighter_chain_id as u64),
                    Goldilocks::from_canonical_u64(tx_type as u64),
                    to_goldi_i64(nonce),
                    to_goldi_i64(expired_at),
                    to_goldi_i64(account_index),
                    Goldilocks::from_canonical_u64(api_key_index as u64),
                    Goldilocks::from_canonical_u64(market_index as u64),
                    Goldilocks::from_canonical_u64(initial_margin_fraction as u64),
                    Goldilocks::from_canonical_u64(margin_mode as u64),
                ]
            }
            _ => {
                return Err(SignerError::API(format!("Unsupported transaction type: {}", tx_type)));
            }
        };
        
                // Hash the Goldilocks field elements using Poseidon2 to produce a 40-byte hash
        // The result is a quintic extension field element (Fp5) which is then converted to bytes
        use poseidon_hash::hash_to_quintic_extension;
        let hash_result = hash_to_quintic_extension(&elements);
        let message_array = hash_result.to_bytes_le();

        // Sign the hash

        let signature = self.sign(&message_array)?;
        if verify_sign {
           
            let pubkey = self.public_key_bytes();
            let sig_ok = verify_signature(&signature,  &message_array, &pubkey).unwrap_or(false);

            if !sig_ok {
                // If signature doesn't match, return error (or log)
                return Err(SignerError::Crypto(
                    goldilocks_crypto::CryptoError::InvalidSignature
                ));
            }
        }

        

        let signature = self.sign(&message_array)?;
        let mut final_tx_info = tx_value;
        final_tx_info["Sig"] = json!(base64::engine::general_purpose::STANDARD.encode(&signature));
        
        let encode_json = serde_json::to_string(&final_tx_info);
        Ok(encode_json.unwrap())

    }
}
