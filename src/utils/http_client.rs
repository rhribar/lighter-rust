use crate::{PointsBotError, PointsBotResult};
use reqwest::{Client, Response};
use serde::de::DeserializeOwned;
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

pub struct HttpClient {
    client: Client,
    base_url: String,
    rate_limit_per_minute: u64,
    last_request: Mutex<Option<Instant>>,
}

impl HttpClient {
    pub fn new(base_url: String, rate_limit_per_minute: Option<u64>) -> Self {
        let client = Client::new();
        Self {
            client,
            base_url,
            rate_limit_per_minute: rate_limit_per_minute.unwrap_or(60),
            last_request: Mutex::new(None),
        }
    }

    async fn handle_response(&self, response: Response) -> PointsBotResult<Response> {
        let status = response.status();
        if status.is_success() {
            Ok(response)
        } else {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "<failed to read body>".to_string());
            Err(PointsBotError::Exchange {
                code: status.as_str().to_string(),
                message: format!("Server error: {} - {}", status, body),
            })
        }
    }

    fn apply_headers(
        request: reqwest::RequestBuilder,
        headers: Option<&HashMap<String, String>>,
    ) -> reqwest::RequestBuilder {
        if let Some(headers) = headers {
            headers
                .into_iter()
                .fold(request, |req, (key, value)| req.header(key, value))
        } else {
            request
        }
    }

    pub async fn get(
        &self,
        endpoint: &str,
        headers: Option<HashMap<String, String>>,
    ) -> PointsBotResult<Response> {
        self.rate_limit().await;
        let url = format!("{}{}", self.base_url, endpoint);
        let request = Self::apply_headers(self.client.get(&url), headers.as_ref());
        let response = request.send().await?;
        self.handle_response(response).await
    }

    pub async fn post(
        &self,
        endpoint: &str,
        body: &str,
        headers: Option<HashMap<String, String>>,
    ) -> PointsBotResult<Response> {
        self.rate_limit().await;
        let url = format!("{}{}", self.base_url, endpoint);
        let mut request = Self::apply_headers(self.client.post(&url), headers.as_ref());
        if headers.is_none() {
            request = request.header("Content-Type", "application/json");
        }
        let response = request.body(body.to_string()).send().await?;
        self.handle_response(response).await
    }

    pub async fn patch(
        &self,
        endpoint: &str,
        body: &str,
        headers: Option<HashMap<String, String>>,
    ) -> PointsBotResult<Response> {
        self.rate_limit().await;
        let url = format!("{}{}", self.base_url, endpoint);
        let mut request = Self::apply_headers(self.client.patch(&url), headers.as_ref());
        if headers.is_none() {
            request = request.header("Content-Type", "application/json");
        }
        let response = request.body(body.to_string()).send().await?;
        self.handle_response(response).await
    }

    pub async fn parse_json<T: DeserializeOwned>(&self, response: Response) -> PointsBotResult<T> {
        let text = response.text().await?;
        serde_json::from_str(&text).map_err(|e| PointsBotError::Parse {
            msg: format!("Failed to parse JSON: {}", e),
            source: Some(Box::new(e)),
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
        let mut last_request = self.last_request.lock().unwrap();
        *last_request = Some(Instant::now());
    }
}
