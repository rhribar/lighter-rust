mod signer;
use std::os::raw::{c_char, c_int, c_longlong};
use poseidon_hash::Goldilocks;
use signer::{KeyManager, Result};
use serde_json::json;
use std::ffi::{CStr, CString};
use std::time::{SystemTime, UNIX_EPOCH};

use hex;

use crate::signer::SignerError;

#[repr(C)]
pub struct StrOrErr {
    pub msg: *mut c_char,
    pub error:*mut c_char,
}

// --- helpers ----------------------------------------------------------

fn into_str_or_err(result: Result<String>) -> StrOrErr {
    match result {
        Ok(s) => {
            let c = CString::new(s).unwrap();
            StrOrErr { msg: c.into_raw() , error: std::ptr::null_mut(), }
        }
        Err(e) => {
            let c = CString::new(e.to_string()).unwrap();
            StrOrErr {msg: std::ptr::null_mut(), error: c.into_raw() }
        }
    }
}

/// Called by C# to free returned `char*`
#[no_mangle]
pub extern "C" fn FreeMessage(ptr: *mut c_char) {
    if !ptr.is_null() {
        unsafe { drop(CString::from_raw(ptr)); }
    }
}

// --- exported functions -----------------------------------------------

#[no_mangle]
pub extern "C" fn GetPubKey(private_key: *const c_char) -> StrOrErr {
    let pk = unsafe { CStr::from_ptr(private_key) }.to_string_lossy().to_string();
    let mgr = match KeyManager::from_hex(&pk) {
        Ok(m) => m,
        Err(e) => return into_str_or_err(Err(e)),
    };

    into_str_or_err(Ok(hex::encode(mgr.public_key_bytes())))
}

#[no_mangle]
pub extern "C" fn CreateAuthToken(
    private_key: *const c_char,
    _chain_id: c_int,
    api_key_index: c_int,
    account_index: c_longlong,
    deadline: c_longlong,
) -> StrOrErr {
    let pk = unsafe { CStr::from_ptr(private_key) }.to_string_lossy().to_string();
    let mgr = match KeyManager::from_hex(&pk) {
        Ok(m) => m,
        Err(e) => return into_str_or_err(Err(e)),
    };

    into_str_or_err(
        mgr.create_auth_token(deadline, account_index, api_key_index as u8, true)
    )
}

// --- SAME SIGNATURES AS GO DLL (put your internal signing logic here)--

#[no_mangle]
pub extern "C" fn SignCreateOrder(
    private_key: *const c_char,
    chain_id: c_int,
    api_key_index: c_int,
    account_index: c_longlong,
    market_index: c_int,
    client_order_index: c_longlong,
    base_amount: c_longlong,
    price: c_int,
    is_ask: c_int,
    order_type: c_int,
    time_in_force: c_int,
    reduce_only: c_int,
    trigger_price: c_int,
    order_expiry: c_longlong,
    nonce: c_longlong,
) -> StrOrErr {
    let pk = unsafe { CStr::from_ptr(private_key) }.to_string_lossy().to_string();

    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i64;
    let expired_at = now + 599_000; // 10 minutes - 1 second (in milliseconds)
    
    let elements = vec![
        Goldilocks::from_canonical_u64(chain_id as u64),
        Goldilocks::from_canonical_u64(14),
        Goldilocks::from_i64(nonce),
        Goldilocks::from_i64(expired_at),
        Goldilocks::from_i64(account_index),
        Goldilocks::from_canonical_u64(api_key_index as u64),
        Goldilocks::from_canonical_u64(market_index as u64),
        Goldilocks::from_i64(client_order_index),
        Goldilocks::from_i64(base_amount),
        Goldilocks::from_canonical_u64(price as u64),
        Goldilocks::from_canonical_u64(is_ask as u64),
        Goldilocks::from_canonical_u64(order_type as u64),
        Goldilocks::from_canonical_u64(time_in_force as u64),
        Goldilocks::from_canonical_u64(reduce_only as u64),
        Goldilocks::from_canonical_u64(trigger_price as u64),
        Goldilocks::from_i64(order_expiry),
    ];
    let sign_err = build_transaction_vector(&pk, &elements);
    if sign_err.is_err() {
        return into_str_or_err(sign_err);
    } 
    let signature = sign_err.unwrap();

    let tx_info = json!({
            "AccountIndex": account_index,
            "ApiKeyIndex": api_key_index,
            "MarketIndex": market_index,
            "ClientOrderIndex": client_order_index,
            "BaseAmount": base_amount,
            "Price": price,
            "IsAsk": is_ask,
            "Type": order_type,
            "TimeInForce": time_in_force,
            "ReduceOnly": reduce_only,
            "TriggerPrice": trigger_price,
            "OrderExpiry": order_expiry, // NilOrderExpiry for market orders
            "ExpiredAt": expired_at,
            "Nonce": nonce,
            "Sig": signature,
        });
    let js = serde_json::to_string(&tx_info).unwrap();
    return into_str_or_err(Ok(js));
}

#[no_mangle]
pub extern "C" fn SignCancelOrder(
    private_key: *const c_char,
    chain_id: c_int,
    api_key_index: c_int,
    account_index: c_longlong,
    market_index: c_int,
    order_index: c_longlong,
    nonce: c_longlong,
) -> StrOrErr {

    let pk = unsafe { CStr::from_ptr(private_key) }.to_string_lossy().to_string();
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i64;
    let expired_at = now + 599_000; // 10 minutes - 1 second (in milliseconds)

    let elements = vec![
        Goldilocks::from_canonical_u64(chain_id as u64),
        Goldilocks::from_canonical_u64(15),
        Goldilocks::from_i64(nonce),
        Goldilocks::from_i64(expired_at),
        Goldilocks::from_i64(account_index),
        Goldilocks::from_canonical_u64(api_key_index as u64),
        Goldilocks::from_canonical_u64(market_index as u64),
        Goldilocks::from_i64(order_index),
    ];

    let sign_err = build_transaction_vector(&pk, &elements);
    if sign_err.is_err() {
        return into_str_or_err(sign_err);
    } 
    let signature = sign_err.unwrap();
    
    let tx_info = json!({
        "AccountIndex": account_index,
        "ApiKeyIndex": api_key_index,
        "MarketIndex": market_index,
        "Index": order_index,
        "ExpiredAt": expired_at,
        "Nonce": nonce,
        "Sig": signature
    });

    let js = serde_json::to_string(&tx_info).unwrap();
    return into_str_or_err(Ok(js));
}

#[no_mangle]
pub extern "C" fn SignModifyOrder(
    private_key: *const c_char,
    chain_id: c_int,
    api_key_index: c_int,
    account_index: c_longlong,
    market_index: c_int,
    index: c_longlong,
    new_base_amount: c_longlong,
    new_price: c_int,
    trigger_price: c_int,
    nonce: c_longlong,
) -> StrOrErr {
    let pk = unsafe { CStr::from_ptr(private_key) }.to_string_lossy().to_string();
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i64;
    let expired_at = now + 599_000; // 10 minutes - 1 second (in milliseconds)

    let elements = vec![
        Goldilocks::from_canonical_u64(chain_id as u64),
        Goldilocks::from_canonical_u64(17),
        Goldilocks::from_i64(nonce),
        Goldilocks::from_i64(expired_at),

        Goldilocks::from_i64(account_index),
        Goldilocks::from_canonical_u64(api_key_index as u64),
        Goldilocks::from_canonical_u64(market_index as u64),
        Goldilocks::from_i64(index),
        
        Goldilocks::from_i64(new_base_amount),
        Goldilocks::from_canonical_u64(new_price as u64),
        Goldilocks::from_canonical_u64(trigger_price as u64),
    ];

    let sign_err = build_transaction_vector(&pk, &elements);
    if sign_err.is_err() {
        return into_str_or_err(sign_err);
    } 
    let signature = sign_err.unwrap();
    let tx_info = json!({
        "AccountIndex": account_index,
        "ApiKeyIndex": api_key_index,
        "MarketIndex": market_index,
        "Index": index,
        "BaseAmount": new_base_amount,
        "Price": new_price,
        "TriggerPrice": trigger_price,
        "ExpiredAt": expired_at,
        "Nonce": nonce,
        "Sig": signature
    });

    let js = serde_json::to_string(&tx_info).unwrap();
    return into_str_or_err(Ok(js));
}

fn build_transaction_vector(pk: &str, elements: &Vec<Goldilocks>) -> Result<String> {
    // Initialize key manager

    let mgr = KeyManager::from_hex(pk)
        .map_err(|e| SignerError::API(format!("Invalid private key {e}").to_string()))?;

    mgr.sign_vector(elements, false)
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_direct_auth_token_loop() {

        use goldilocks_crypto::ScalarField;
        let private_key = ScalarField::sample_crypto();
        let private_key_bytes = private_key.to_bytes_le();

        let hex_string = hex::encode(private_key_bytes);
        let mgr = KeyManager::from_hex(&hex_string).expect("invalid private key");

        let mut failed = 0;

        for i in 0..100 {
            let result = mgr.create_auth_token(
                123456789,  // deadline
                1,          // account_index
                0,          // api_key_index
                true        // user-requested flag
            );

            if let Err(e) = result {
                failed += 1;
                println!("Failed at iteration {i}: {}", e);
            }
        }

        if failed > 0 {
            panic!("{} failures occurred while generating auth tokens", failed);
        }
    }
}