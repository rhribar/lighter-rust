mod signer;
use std::os::raw::{c_char, c_int, c_longlong};
use signer::{KeyManager, Result};
use serde_json::json;
use std::ffi::{CStr, CString};
use std::time::{SystemTime, UNIX_EPOCH};

use hex;

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
            "Sig": ""
        });
    let js = serde_json::to_string(&tx_info).unwrap();
    build_transaction(&pk, &js, 14, chain_id as u32)
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

    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i64;
    let expired_at = now + 599_000; // 10 minutes - 1 second (in milliseconds)

    let pk = unsafe { CStr::from_ptr(private_key) }.to_string_lossy().to_string();
    let tx_info = json!({
        "AccountIndex": account_index,
        "ApiKeyIndex": api_key_index,
        "MarketIndex": market_index,
        "Index": order_index,
        "ExpiredAt": expired_at,
        "Nonce": nonce,
        "Sig": ""
    });

    let js = serde_json::to_string(&tx_info).unwrap();
    build_transaction(&pk, &js, 15, chain_id as u32)
}

#[no_mangle]
pub extern "C" fn SignModifyOrder(
    private_key: *const c_char,
    chain_id: c_int,
    api_key_index: c_int,
    account_index: c_longlong,
    market_index: c_int,
    client_order_index: c_longlong,
    new_base_amount: c_longlong,
    new_price: c_int,
    reduce_only: c_int,
    new_order_expiry: c_longlong,
    nonce: c_longlong,
) -> StrOrErr {
    let pk = unsafe { CStr::from_ptr(private_key) }.to_string_lossy().to_string();

    let tx_info = json!({
        "AccountIndex": account_index,
        "ApiKeyIndex": api_key_index,
        "MarketIndex": market_index,
        "ClientOrderIndex": client_order_index,
        "BaseAmount": new_base_amount,
        "Price": new_price,
        "ReduceOnly": reduce_only,
        "OrderExpiry": new_order_expiry,
        "ExpiredAt": new_order_expiry,
        "Nonce": nonce,
        "Sig": ""
    });

    let js = serde_json::to_string(&tx_info).unwrap();

    build_transaction(&pk, &js, 17, chain_id as u32)
}

#[no_mangle]
pub extern "C" fn SignJsonData(
    private_key: *const c_char,
    json_data: *const c_char,
    tx_type: c_int,
    chain_id: c_int,
)-> StrOrErr {
    let pk = unsafe { CStr::from_ptr(private_key) }.to_string_lossy().to_string();
    let js = unsafe { CStr::from_ptr(json_data) }.to_string_lossy().to_string();

    build_transaction(&pk, &js, tx_type as u32, chain_id as u32)
}

fn build_transaction(pk:&str, tx_json: &str, tx_type: u32, lighter_chain_id: u32)->StrOrErr{

    let mgr = match KeyManager::from_hex(&pk) {
        Ok(m) => m,
        Err(e) => return into_str_or_err(Err(e)),
    };
    let signature = mgr.sign_transaction(&tx_json, tx_type, lighter_chain_id, false);

    into_str_or_err(signature)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_direct_auth_token_loop() {
        let private_key = "bda332f3aaa2d9cfdd8920830ea37efce9636c671a426bd4cb9815007e2a2917604ab47857cbb200";

        let mgr = KeyManager::from_hex(private_key).expect("invalid private key");

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