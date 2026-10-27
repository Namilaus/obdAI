use super::sse::SseDecoder;
use crate::ai::*;
use futures_util::StreamExt;
use serde_json::Value;
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone)]
pub enum RequestKind {
    Models { cursor: Option<String> },
    Generate,
}
#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub provider: ProviderId,
    pub kind: RequestKind,
    pub body: Option<Value>,
}
#[derive(Debug, Clone)]
pub enum HttpReply {
    Json(Value),
    Stream(Vec<Value>),
}
pub type WireSink = Arc<dyn Fn(&Value) + Send + Sync>;
#[async_trait::async_trait]
pub trait HttpTransport: Send + Sync {
    async fn execute(
        &self,
        request: HttpRequest,
        secret: &SecretString,
        cancel: CancellationToken,
        sink: WireSink,
    ) -> Result<HttpReply, AiError>;
}
pub struct NetworkTransport {
    client: reqwest::Client,
}
impl NetworkTransport {
    pub fn new() -> Result<Self, AiError> {
        let client = reqwest::Client::builder()
            .https_only(true)
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(10))
            .read_timeout(Duration::from_secs(60))
            .timeout(Duration::from_secs(180))
            .build()
            .map_err(|_| {
                AiError::new("network", "Could not initialize the provider connection.")
            })?;
        Ok(Self { client })
    }
}
fn network_error(_: reqwest::Error) -> AiError {
    AiError::new("network","Provider connection failed or timed out. Check connectivity before retrying; a request may already have been billed.")
}
#[async_trait::async_trait]
impl HttpTransport for NetworkTransport {
    async fn execute(
        &self,
        request: HttpRequest,
        secret: &SecretString,
        cancel: CancellationToken,
        sink: WireSink,
    ) -> Result<HttpReply, AiError> {
        let (origin, path) = match (request.provider, &request.kind) {
            (ProviderId::Openai, RequestKind::Models { .. }) => {
                ("https://api.openai.com", "/v1/models")
            }
            (ProviderId::Openai, RequestKind::Generate) => {
                ("https://api.openai.com", "/v1/responses")
            }
            (ProviderId::Anthropic, RequestKind::Models { .. }) => {
                ("https://api.anthropic.com", "/v1/models")
            }
            (ProviderId::Anthropic, RequestKind::Generate) => {
                ("https://api.anthropic.com", "/v1/messages")
            }
            (ProviderId::Deepseek, RequestKind::Models { .. }) => {
                ("https://api.deepseek.com", "/models")
            }
            (ProviderId::Deepseek, RequestKind::Generate) => {
                ("https://api.deepseek.com", "/chat/completions")
            }
        };
        let mut url = reqwest::Url::parse(&format!("{origin}{path}"))
            .map_err(|_| AiError::new("network", "Invalid provider endpoint."))?;
        if let RequestKind::Models {
            cursor: Some(cursor),
        } = &request.kind
        {
            url.query_pairs_mut().append_pair("after_id", cursor);
        }
        let mut builder = if let Some(body) = &request.body {
            self.client.post(url).json(body)
        } else {
            self.client.get(url)
        };
        if request.provider == ProviderId::Anthropic {
            builder = builder
                .header("x-api-key", secret.expose())
                .header("anthropic-version", "2026-10-21");
        } else {
            builder = builder.bearer_auth(secret.expose());
        }
        let response = tokio::select! {_=cancel.cancelled()=>return Err(AiError::new("cancelled","Request cancelled.")),result=builder.send()=>result.map_err(network_error)?};
        if !response.status().is_success() {
            let(code,message)=match response.status().as_u16(){401|403=>("auth","Provider rejected the API key or account access."),429=>("quota","Provider rate or usage limit reached. Check the account before retrying."),400|404|422=>("unsupported_model","Provider rejected this model or request. Refresh models and test the selected model."),_ =>("provider","Provider returned an error. Retry later if appropriate.")};
            return Err(AiError::new(code, message));
        }
        let stream = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|s| s.to_str().ok())
            .map(|s| s.contains("text/event-stream"))
            .unwrap_or(false);
        let mut chunks = response.bytes_stream();
        let mut bytes = Vec::new();
        let mut decoder = SseDecoder::default();
        let mut events = Vec::new();
        let mut total = 0;
        loop {
            let chunk = tokio::select! {_=cancel.cancelled()=>return Err(AiError::new("cancelled","Request cancelled.")),chunk=chunks.next()=>chunk};
            let Some(chunk) = chunk else {
                break;
            };
            let chunk = chunk.map_err(network_error)?;
            total += chunk.len();
            if total > 4 * 1024 * 1024 {
                return Err(AiError::new(
                    "protocol",
                    "Provider response exceeded its size limit.",
                ));
            }
            if stream {
                for event in decoder.push(&chunk)? {
                    if event.data == "[DONE]" {
                        continue;
                    }
                    let value: Value = serde_json::from_str(&event.data).map_err(|_| {
                        AiError::new("protocol", "Provider returned malformed streaming data.")
                    })?;
                    sink(&value);
                    events.push(value);
                }
            } else {
                bytes.extend_from_slice(&chunk);
            }
        }
        if stream {
            decoder.finish()?;
            Ok(HttpReply::Stream(events))
        } else {
            Ok(HttpReply::Json(serde_json::from_slice(&bytes).map_err(
                |_| AiError::new("protocol", "Provider returned invalid JSON."),
            )?))
        }
    }
}
