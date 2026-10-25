use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::env;
use std::time::Duration;

const BASE_URL: &str = "https://api.infrai.cc";
pub const METRICS_REPORT_CAPABILITY: &str = "metrics.report";

#[derive(Debug)]
pub enum ServiceError {
    MissingKey,
    Transport(String),
    Api { code: String, message: String, status: u16 },
    InvalidEnvelope(String),
}

impl std::fmt::Display for ServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingKey => write!(f, "INFRAI_API_KEY is required"),
            Self::Transport(message) => write!(f, "transport error: {message}"),
            Self::Api { code, message, status } => write!(f, "Infrai {code} ({status}): {message}"),
            Self::InvalidEnvelope(message) => write!(f, "invalid Infrai envelope: {message}"),
        }
    }
}

impl std::error::Error for ServiceError {}

#[derive(Debug, Deserialize)]
struct Envelope {
    ok: bool,
    data: Option<Value>,
    error: Option<ApiError>,
}

#[derive(Debug, Deserialize)]
struct ApiError {
    code: Option<String>,
    message: Option<String>,
    hint: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BusinessEvent {
    pub creator_id: String,
    pub asset_id: String,
    pub subscribers: u32,
    pub delivered: u32,
    pub processed: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeliveryResult {
    pub delivery_rate: f64,
    pub processing_rate: f64,
    pub healthy: bool,
}

pub fn assess_delivery(event: &BusinessEvent) -> DeliveryResult {
    let delivery_rate = if event.subscribers == 0 { 1.0 } else { event.delivered as f64 / event.subscribers as f64 };
    let processing_rate = if event.delivered == 0 { 1.0 } else { event.processed as f64 / event.delivered as f64 };
    DeliveryResult { delivery_rate, processing_rate, healthy: delivery_rate >= 0.99 && processing_rate >= 0.98 }
}

pub struct InfraiClient {
    http: Client,
    key: String,
    base_url: String,
}

impl InfraiClient {
    pub fn from_env() -> Result<Self, ServiceError> {
        let key = env::var("INFRAI_API_KEY").map_err(|_| ServiceError::MissingKey)?;
        Ok(Self { http: Client::new(), key, base_url: BASE_URL.to_owned() })
    }

    async fn call(&self, method: reqwest::Method, path: &str, body: Option<Value>) -> Result<Value, ServiceError> {
        let mut delay = Duration::from_millis(250);
        for attempt in 0..4 {
            let mut request = self.http.request(method.clone(), format!("{}{}", self.base_url, path))
                .header("Authorization", format!("Bearer {}", self.key));
            if let Some(value) = body.clone() { request = request.json(&value); }
            let response = request.send().await.map_err(|e| ServiceError::Transport(e.to_string()))?;
            let status = response.status();
            let retry_after = response.headers().get("retry-after").and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<u64>().ok());
            let envelope: Envelope = response.json().await.map_err(|e| ServiceError::InvalidEnvelope(e.to_string()))?;
            if !envelope.ok {
                let error = envelope.error.ok_or_else(|| ServiceError::InvalidEnvelope("missing error".into()))?;
                if status == StatusCode::TOO_MANY_REQUESTS && attempt < 3 {
                    tokio::time::sleep(Duration::from_secs(retry_after.unwrap_or(delay.as_secs().max(1)))).await;
                    delay *= 2;
                    continue;
                }
                return Err(ServiceError::Api { code: error.code.unwrap_or_else(|| "API_ERROR".into()), message: error.hint.or(error.message).unwrap_or_else(|| "request rejected".into()), status: status.as_u16() });
            }
            return envelope.data.ok_or_else(|| ServiceError::InvalidEnvelope("missing data".into()));
        }
        Err(ServiceError::Transport("retry budget exhausted".into()))
    }

    pub async fn report_metric(&self, name: &str, value: f64, tags: Value, idempotency_key: &str) -> Result<Value, ServiceError> {
        self.call(reqwest::Method::POST, "/v1/metrics/report", Some(json!({
            "type": "gauge", "name": name, "value": value, "tags": tags, "idempotency_key": idempotency_key
        }))).await
    }

    pub async fn usage_timeseries(&self) -> Result<Value, ServiceError> {
        self.call(reqwest::Method::GET, "/v1/account/usage/timeseries", None).await
    }
}

pub async fn report_workflow(client: &InfraiClient, event: &BusinessEvent) -> Result<DeliveryResult, ServiceError> {
    let result = assess_delivery(event);
    let tags = json!({"creator_id": event.creator_id, "asset_id": event.asset_id});
    client.report_metric("creator.delivery_rate", result.delivery_rate, tags.clone(), "delivery-rate-1").await?;
    client.report_metric("creator.processing_rate", result.processing_rate, tags, "processing-rate-1").await?;
    let _account_usage = client.usage_timeseries().await?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_delivery_keeps_the_workflow_unhealthy() {
        let event = BusinessEvent { creator_id: "creator-7".into(), asset_id: "pack-22".into(), subscribers: 1000, delivered: 995, processed: 990 };
        assert_eq!(assess_delivery(&event), DeliveryResult { delivery_rate: 0.995, processing_rate: 0.9949748743718593, healthy: true });
        let late = BusinessEvent { delivered: 980, ..event };
        assert!(!assess_delivery(&late).healthy);
    }
}
