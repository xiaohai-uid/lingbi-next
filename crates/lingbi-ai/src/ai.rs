use async_stream::stream;
use async_trait::async_trait;
use futures_core::Stream;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use std::time::Instant;
use thiserror::Error;

pub type AiStream = Pin<Box<dyn Stream<Item = Result<AiEvent, AiError>> + Send>>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatRequest {
    pub messages: Vec<ChatMessage>,
    pub temperature: f32,
    pub max_tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AiEvent {
    ReasoningDelta(String),
    ContentDelta(String),
    Usage(TokenUsage),
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderHealth {
    pub provider_id: String,
    pub ok: bool,
    pub latency_ms: u64,
    pub model_id: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Error)]
pub enum AiError {
    #[error("no API key configured")]
    NoApiKey,
    #[error("authentication failed")]
    AuthFailed,
    #[error("rate limited")]
    RateLimited,
    #[error("request timed out")]
    Timeout,
    #[error("network error")]
    Network,
    #[error("server error: {0}")]
    Server(u16),
    #[error("invalid provider response")]
    InvalidResponse,
}

impl AiError {
    pub fn from_status(status: u16) -> Self {
        match status {
            401 | 403 => Self::AuthFailed,
            429 => Self::RateLimited,
            status if status >= 500 => Self::Server(status),
            _ => Self::InvalidResponse,
        }
    }
}

#[async_trait]
pub trait AiProvider: Send + Sync {
    fn provider_id(&self) -> &str {
        "unknown"
    }

    async fn test_connection(&self) -> ProviderHealth {
        let started = Instant::now();
        let request = ChatRequest {
            messages: vec![ChatMessage {
                role: "user".to_owned(),
                content: "Hi".to_owned(),
            }],
            temperature: 0.0,
            max_tokens: 5,
        };
        let mut stream = self.stream_chat(request);
        let mut received_content = false;
        let mut error = None;
        while let Some(event) = stream.next().await {
            match event {
                Ok(AiEvent::ContentDelta(_)) => {
                    received_content = true;
                }
                Ok(AiEvent::Completed) => break,
                Ok(_) => {}
                Err(stream_error) => {
                    error = Some(stream_error.to_string());
                    break;
                }
            }
        }
        if error.is_none() && !received_content {
            error = Some(AiError::InvalidResponse.to_string());
        }
        ProviderHealth {
            provider_id: self.provider_id().to_owned(),
            ok: error.is_none(),
            latency_ms: started.elapsed().as_millis() as u64,
            model_id: self.model_id().to_owned(),
            error,
        }
    }

    fn model_id(&self) -> &str;

    fn stream_chat(&self, request: ChatRequest) -> AiStream;
}

pub struct OpenAiCompatibleProvider {
    api_key: Option<String>,
    base_url: String,
    model: String,
    client: reqwest::Client,
}

impl OpenAiCompatibleProvider {
    pub fn new(
        api_key: impl Into<String>,
        base_url: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self {
            api_key: Some(api_key.into()),
            base_url: base_url.into(),
            model: model.into(),
            client: reqwest::Client::new(),
        }
    }

    pub fn unconfigured(base_url: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            api_key: None,
            base_url: base_url.into(),
            model: model.into(),
            client: reqwest::Client::new(),
        }
    }
}

impl AiProvider for OpenAiCompatibleProvider {
    fn provider_id(&self) -> &str {
        "openai-compatible"
    }

    fn model_id(&self) -> &str {
        &self.model
    }

    fn stream_chat(&self, request: ChatRequest) -> AiStream {
        let client = self.client.clone();
        let base_url = self.base_url.clone();
        let model = self.model.clone();
        let api_key = self.api_key.clone().unwrap_or_default();

        Box::pin(stream! {
            if api_key.is_empty() {
                yield Err(AiError::NoApiKey);
                return;
            }
            let body = serde_json::json!({
                "model": model,
                "messages": request.messages,
                "temperature": request.temperature,
                "max_tokens": request.max_tokens,
                "stream": true,
            });
            let response = match client
                .post(&base_url)
                .bearer_auth(&api_key)
                .json(&body)
                .send()
                .await
            {
                Ok(response) => response,
                Err(error) if error.is_timeout() => {
                    yield Err(AiError::Timeout);
                    return;
                }
                Err(_) => {
                    yield Err(AiError::Network);
                    return;
                }
            };
            let status = response.status().as_u16();
            if !response.status().is_success() {
                yield Err(AiError::from_status(status));
                return;
            }

            let mut stream = response.bytes_stream();
            let mut pending = String::new();
            while let Some(chunk) = stream.next().await {
                let chunk = match chunk {
                    Ok(chunk) => chunk,
                    Err(_) => {
                        yield Err(AiError::Network);
                        return;
                    }
                };
                pending.push_str(&String::from_utf8_lossy(&chunk));
                let lines: Vec<String> = pending.split('\n').map(str::to_owned).collect();
                pending = lines
                    .last()
                    .cloned()
                    .unwrap_or_default()
                    .to_owned();
                for line in lines.iter().take(lines.len().saturating_sub(1)) {
                    let line = line.trim();
                    if !line.starts_with("data: ") {
                        continue;
                    }
                    let data = line.trim_start_matches("data: ").trim();
                    if data == "[DONE]" {
                        yield Ok(AiEvent::Completed);
                        return;
                    }
                    let value: serde_json::Value = match serde_json::from_str(data) {
                        Ok(value) => value,
                        Err(_) => {
                            yield Err(AiError::InvalidResponse);
                            return;
                        }
                    };
                    let Some(content) = value
                        .pointer("/choices/0/delta/content")
                        .and_then(|value| value.as_str())
                    else {
                        continue;
                    };
                    if !content.is_empty() {
                        yield Ok(AiEvent::ContentDelta(content.to_owned()));
                    }
                }
            }
            yield Ok(AiEvent::Completed);
        })
    }
}

pub struct AnthropicProvider {
    api_key: Option<String>,
    base_url: String,
    model: String,
    client: reqwest::Client,
}

impl AnthropicProvider {
    pub fn new(
        api_key: impl Into<String>,
        base_url: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self {
            api_key: Some(api_key.into()),
            base_url: base_url.into(),
            model: model.into(),
            client: reqwest::Client::new(),
        }
    }

    pub fn unconfigured(base_url: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            api_key: None,
            base_url: base_url.into(),
            model: model.into(),
            client: reqwest::Client::new(),
        }
    }
}

impl AiProvider for AnthropicProvider {
    fn provider_id(&self) -> &str {
        "anthropic"
    }

    fn model_id(&self) -> &str {
        &self.model
    }

    fn stream_chat(&self, request: ChatRequest) -> AiStream {
        let client = self.client.clone();
        let base_url = self.base_url.clone();
        let model = self.model.clone();
        let api_key = self.api_key.clone().unwrap_or_default();

        Box::pin(stream! {
            if api_key.is_empty() {
                yield Err(AiError::NoApiKey);
                return;
            }
            let system = request
                .messages
                .iter()
                .filter(|message| message.role == "system")
                .map(|message| message.content.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            let messages: Vec<_> = request
                .messages
                .iter()
                .filter(|message| message.role != "system")
                .map(|message| {
                    serde_json::json!({
                        "role": message.role,
                        "content": message.content,
                    })
                })
                .collect();
            let body = serde_json::json!({
                "model": model,
                "max_tokens": request.max_tokens,
                "system": if system.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(system) },
                "messages": messages,
            });
            let response = match client
                .post(&base_url)
                .header("x-api-key", &api_key)
                .header("anthropic-version", "2023-06-01")
                .json(&body)
                .send()
                .await
            {
                Ok(response) => response,
                Err(error) if error.is_timeout() => {
                    yield Err(AiError::Timeout);
                    return;
                }
                Err(_) => {
                    yield Err(AiError::Network);
                    return;
                }
            };
            let status = response.status().as_u16();
            if !response.status().is_success() {
                yield Err(AiError::from_status(status));
                return;
            }
            let value: serde_json::Value = match response.json().await {
                Ok(value) => value,
                Err(_) => {
                    yield Err(AiError::InvalidResponse);
                    return;
                }
            };
            let Some(blocks) = value.get("content").and_then(|value| value.as_array()) else {
                yield Err(AiError::InvalidResponse);
                return;
            };
            for block in blocks {
                if block.get("type").and_then(|value| value.as_str()) == Some("text")
                    && let Some(text) = block.get("text").and_then(|value| value.as_str())
                {
                    yield Ok(AiEvent::ContentDelta(text.to_owned()));
                }
            }
            yield Ok(AiEvent::Completed);
        })
    }
}

pub struct FakeProvider {
    response: String,
    error: Option<AiError>,
}

impl FakeProvider {
    pub fn new(response: impl Into<String>) -> Self {
        Self {
            response: response.into(),
            error: None,
        }
    }

    pub fn with_error(error: AiError) -> Self {
        Self {
            response: String::new(),
            error: Some(error),
        }
    }
}

impl AiProvider for FakeProvider {
    fn provider_id(&self) -> &str {
        "fake"
    }

    fn model_id(&self) -> &str {
        "fake-provider"
    }

    fn stream_chat(&self, _request: ChatRequest) -> AiStream {
        let response = self.response.clone();
        let error = self.error.clone();
        Box::pin(stream! {
            if let Some(error) = error {
                yield Err(error);
                return;
            }
            if !response.is_empty() {
                yield Ok(AiEvent::ContentDelta(response));
            }
            yield Ok(AiEvent::Completed);
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_http_status_to_typed_error() {
        assert_eq!(AiError::from_status(401), AiError::AuthFailed);
        assert_eq!(AiError::from_status(429), AiError::RateLimited);
        assert_eq!(AiError::from_status(500), AiError::Server(500));
    }

    #[tokio::test]
    async fn unconfigured_provider_fails_with_no_api_key() {
        let provider = OpenAiCompatibleProvider::unconfigured(
            "https://example.com/v1/chat/completions",
            "model",
        );
        let request = ChatRequest {
            messages: vec![ChatMessage {
                role: "user".to_owned(),
                content: "hello".to_owned(),
            }],
            temperature: 0.7,
            max_tokens: 100,
        };

        let mut stream = provider.stream_chat(request);
        let event = stream.next().await.expect("event");

        assert_eq!(event, Err(AiError::NoApiKey));
    }

    #[tokio::test]
    async fn fake_provider_streams_response_and_completes() {
        let provider = FakeProvider::new("第一章正文");
        let request = ChatRequest {
            messages: vec![ChatMessage {
                role: "user".to_owned(),
                content: "write".to_owned(),
            }],
            temperature: 0.7,
            max_tokens: 100,
        };

        let events: Vec<_> = provider.stream_chat(request).collect().await;

        assert_eq!(
            events,
            vec![
                Ok(AiEvent::ContentDelta("第一章正文".to_owned())),
                Ok(AiEvent::Completed),
            ]
        );
    }

    #[tokio::test]
    async fn fake_provider_test_connection_returns_full_contract() {
        let provider = FakeProvider::new("ok");

        let health = provider.test_connection().await;

        assert_eq!(health.provider_id, "fake");
        assert_eq!(health.model_id, "fake-provider");
        assert!(health.ok);
        assert!(health.error.is_none());
    }

    #[tokio::test]
    async fn failed_provider_test_connection_returns_error_field() {
        let provider = FakeProvider::with_error(AiError::AuthFailed);

        let health = provider.test_connection().await;

        assert!(!health.ok);
        assert!(health.error.is_some());
    }
}
