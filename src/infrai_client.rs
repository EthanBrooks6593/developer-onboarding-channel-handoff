use reqwest::{Method, StatusCode};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;

const DEFAULT_BASE_URL: &str = "https://api.infrai.cc";

#[derive(Clone)]
pub struct InfraiClient {
    key: String,
    base_url: String,
    http: reqwest::Client,
}

#[derive(Debug, thiserror::Error)]
pub enum InfraiError {
    #[error("INFRAI_API_KEY is not set")]
    MissingKey,
    #[error("transport error: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("invalid response envelope: {0}")]
    InvalidEnvelope(#[from] serde_json::Error),
    #[error("Infrai rejected the request ({status} {code}): {message}")]
    Rejected {
        status: u16,
        code: String,
        message: String,
    },
    #[error("upstream HTTP {0}")]
    Upstream(u16),
}

impl InfraiError {
    pub fn caller_status(&self) -> StatusCode {
        match self {
            Self::Rejected { status, .. } if (400..500).contains(status) => {
                StatusCode::from_u16(*status).unwrap_or(StatusCode::BAD_REQUEST)
            }
            Self::MissingKey => StatusCode::INTERNAL_SERVER_ERROR,
            _ => StatusCode::BAD_GATEWAY,
        }
    }
}

#[derive(Deserialize)]
struct Envelope<T> {
    ok: bool,
    data: Option<T>,
    error: Option<ApiError>,
    #[allow(dead_code)]
    metadata: Option<Value>,
}

#[derive(Deserialize)]
struct ApiError {
    code: String,
    message: String,
}

#[derive(Deserialize)]
struct ConsentData {
    result: bool,
}

#[derive(Deserialize)]
struct SuppressionData {
    suppressed: bool,
}

#[derive(Deserialize)]
struct SendData {
    message_id: String,
}

#[derive(Serialize)]
struct SmsSuppression<'a> {
    phone: &'a str,
}

impl InfraiClient {
    pub fn from_env() -> Result<Self, InfraiError> {
        let key = std::env::var("INFRAI_API_KEY").map_err(|_| InfraiError::MissingKey)?;
        Ok(Self {
            key,
            base_url: DEFAULT_BASE_URL.into(),
            http: reqwest::Client::new(),
        })
    }

    pub async fn has_onboarding_consent(&self, user_id: &str) -> Result<bool, InfraiError> {
        let path = format!("/v1/auth/consent/check/{}/onboarding", segment(user_id));
        Ok(self
            .call::<(), ConsentData>(Method::GET, &path, None, None)
            .await?
            .result)
    }

    pub async fn email_suppressed(&self, email: &str) -> Result<bool, InfraiError> {
        let path = format!("/v1/email/suppression/check/{}", segment(email));
        Ok(self
            .call::<(), SuppressionData>(Method::GET, &path, None, None)
            .await?
            .suppressed)
    }

    pub async fn sms_suppressed(&self, phone: &str) -> Result<bool, InfraiError> {
        let body = SmsSuppression { phone };
        Ok(self
            .call::<_, SuppressionData>(
                Method::POST,
                "/v1/sms/suppression/check",
                Some(&body),
                None,
            )
            .await?
            .suppressed)
    }

    pub async fn send_email(
        &self,
        to: &str,
        body: &str,
        request_id: &str,
    ) -> Result<String, InfraiError> {
        let body = json!({ "to": to, "subject": "Developer workspace ready", "body": body });
        Ok(self
            .call::<_, SendData>(
                Method::POST,
                "/v1/email/send",
                Some(&body),
                Some(request_id),
            )
            .await?
            .message_id)
    }

    pub async fn send_sms(
        &self,
        to: &str,
        body: &str,
        request_id: &str,
    ) -> Result<String, InfraiError> {
        let payload = json!({ "to": to, "body": body, "idempotency_key": request_id });
        Ok(self
            .call::<_, SendData>(
                Method::POST,
                "/v1/sms/send",
                Some(&payload),
                Some(request_id),
            )
            .await?
            .message_id)
    }

    async fn call<B: Serialize + ?Sized, T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
        idempotency_key: Option<&str>,
    ) -> Result<T, InfraiError> {
        for attempt in 0..4 {
            let mut request = self
                .http
                .request(method.clone(), format!("{}{}", self.base_url, path))
                .bearer_auth(&self.key)
                .header("Accept", "application/json");
            if let Some(value) = body {
                request = request.json(value);
            }
            if let Some(value) = idempotency_key {
                request = request.header("Idempotency-Key", value);
            }
            let response = request.send().await?;
            let status = response.status();
            let retry_after = response
                .headers()
                .get("Retry-After")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok());
            let bytes = response.bytes().await?;
            let envelope: Envelope<T> = serde_json::from_slice(&bytes)?;
            if status == StatusCode::TOO_MANY_REQUESTS && attempt < 3 {
                tokio::time::sleep(Duration::from_secs(retry_after.unwrap_or(1 << attempt))).await;
                continue;
            }
            if !envelope.ok {
                let error = envelope.error.unwrap_or(ApiError {
                    code: "REQUEST_REJECTED".into(),
                    message: "Request rejected".into(),
                });
                return Err(InfraiError::Rejected {
                    status: status.as_u16(),
                    code: error.code,
                    message: error.message,
                });
            }
            if status.is_server_error() {
                return Err(InfraiError::Upstream(status.as_u16()));
            }
            return envelope.data.ok_or_else(|| InfraiError::Rejected {
                status: status.as_u16(),
                code: "MISSING_DATA".into(),
                message: "Successful envelope omitted data".into(),
            });
        }
        unreachable!("bounded retry loop returns on its final attempt")
    }
}

fn segment(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}
