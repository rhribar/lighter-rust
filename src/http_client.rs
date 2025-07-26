use reqwest::{Client, Response};
use std::time::{Duration, Instant};
use std::sync::Mutex;
use std::collections::HashMap;
use serde::de::DeserializeOwned;
use crate::{PointsBotResult, PointsBotError};

pub struct HttpClient {
    client: Client,
    base_url: String,
    rate_limit_per_minute: u64,
    last_request: Mutex<Option<Instant>>,
}

impl HttpClient {
    pub fn new(base_url: String, rate_limit_per_minute: Option<u64>) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");
            
        Self {
            client,
            base_url,
            rate_limit_per_minute: rate_limit_per_minute.unwrap_or(60),
            last_request: Mutex::new(None),
        }
    }
    
    pub async fn get(&self, endpoint: &str, headers: Option<HashMap<String, String>>) -> PointsBotResult<Response> {
        self.rate_limit().await;
        
        let url = format!("{}{}", self.base_url, endpoint);
        let mut request = self.client.get(&url);
        
        if let Some(headers) = headers {
            for (key, value) in headers {
                request = request.header(key, value);
            }
        }
        
        let response = request.send().await?;
            
        if response.status().is_success() {
            Ok(response)
        } else {
            Err(PointsBotError::Exchange {
                code: response.status().as_str().to_string(),
                message: format!("Request failed: {}", response.status()),
            })
        }
    }
    
    pub async fn post(&self, endpoint: &str, body: &str, headers: Option<HashMap<String, String>>) -> PointsBotResult<Response> {
        self.rate_limit().await;
        
        let url = format!("{}{}", self.base_url, endpoint);
        let mut request = self.client.post(&url);
        
        if let Some(headers) = headers {
            for (key, value) in headers {
                request = request.header(&key, &value);
            }
        } else {
            request = request.header("Content-Type", "application/json");
        }
        
        let response = request
            .body(body.to_string())
            .send()
            .await?;
            
        if response.status().is_success() {
            Ok(response)
        } else {
            Err(PointsBotError::Exchange {
                code: response.status().as_str().to_string(),
                message: format!("POST request failed: {}", response.status()),
            })
        }
    }
    
    pub async fn patch(&self, endpoint: &str, body: &str, headers: Option<HashMap<String, String>>) -> PointsBotResult<Response> {
        self.rate_limit().await;

        let url = format!("{}{}", self.base_url, endpoint);
        let mut request = self.client.patch(&url);

        if let Some(headers) = headers {
            for (key, value) in headers {
                request = request.header(&key, &value);
            }
        } else {
            request = request.header("Content-Type", "application/json");
        }

        let response = request
            .body(body.to_string())
            .send()
            .await?;

        if response.status().is_success() {
            Ok(response)
        } else {
            Err(PointsBotError::Exchange {
                code: response.status().as_str().to_string(),
                message: format!("PATCH request failed: {}", response.status()),
            })
        }
    }
    
    pub async fn parse_json<T: DeserializeOwned>(&self, response: Response) -> PointsBotResult<T> {
        let text = response.text().await?;
        serde_json::from_str(&text).map_err(|e| {
            PointsBotError::Parse(format!("Failed to parse JSON: {}", e))
        })
    }
    
    async fn rate_limit(&self) {
        let interval = Duration::from_millis(60_000 / self.rate_limit_per_minute);
        
        let should_sleep = {
            let last_request = self.last_request.lock().unwrap();
            if let Some(last) = *last_request {
                let elapsed = last.elapsed();
                if elapsed < interval {
                    Some(interval - elapsed)
                } else {
                    None
                }
            } else {
                None
            }
        };
        
        if let Some(sleep_duration) = should_sleep {
            tokio::time::sleep(sleep_duration).await;
        }
        
        {
            let mut last_request = self.last_request.lock().unwrap();
            *last_request = Some(Instant::now());
        }
    }
}