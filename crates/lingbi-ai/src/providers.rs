//! Provider registry: the small, novice-friendly set of AI services the
//! desktop app offers. Every provider has a formal `ProviderDefinition`
//! (id, display name, protocol, default endpoint, recommended model,
//! models) so the UI never exposes raw API details to normal users and the
//! correct protocol implementation is always selected — Claude uses the
//! Anthropic provider, OpenAI/DeepSeek use the OpenAI-compatible provider.

use crate::ai::{AiProvider, AnthropicProvider, OpenAiCompatibleProvider};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderProtocol {
    OpenAiCompatible,
    Anthropic,
}

impl ProviderProtocol {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::OpenAiCompatible => "openai-compatible",
            Self::Anthropic => "anthropic",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ProviderDefinition {
    pub id: &'static str,
    pub display_name: &'static str,
    pub protocol: ProviderProtocol,
    pub default_endpoint: &'static str,
    pub recommended_model: &'static str,
    pub models: &'static [&'static str],
}

/// The built-in providers. New providers are added here, never as ad-hoc
/// UI strings.
pub fn provider_definitions() -> &'static [ProviderDefinition] {
    &[
        ProviderDefinition {
            id: "openai",
            display_name: "OpenAI",
            protocol: ProviderProtocol::OpenAiCompatible,
            default_endpoint: "https://api.openai.com/v1/chat/completions",
            recommended_model: "gpt-4o-mini",
            models: &["gpt-4o-mini", "gpt-4o", "gpt-4.1-mini", "gpt-4.1"],
        },
        ProviderDefinition {
            id: "claude",
            display_name: "Claude",
            protocol: ProviderProtocol::Anthropic,
            default_endpoint: "https://api.anthropic.com/v1/messages",
            recommended_model: "claude-3-5-haiku-latest",
            models: &[
                "claude-3-5-haiku-latest",
                "claude-3-5-sonnet-latest",
                "claude-3-7-sonnet-latest",
            ],
        },
        ProviderDefinition {
            id: "deepseek",
            display_name: "DeepSeek",
            protocol: ProviderProtocol::OpenAiCompatible,
            default_endpoint: "https://api.deepseek.com/v1/chat/completions",
            recommended_model: "deepseek-chat",
            models: &["deepseek-chat", "deepseek-reasoner"],
        },
    ]
}

pub fn find_provider(id: &str) -> Option<&'static ProviderDefinition> {
    provider_definitions()
        .iter()
        .find(|definition| definition.id == id)
}

/// Build the correct protocol implementation for a definition. `base_url`
/// and `model` override the defaults only when explicitly provided
/// (advanced settings); normal users get the recommended model and the
/// default endpoint automatically.
pub fn build_provider(
    definition: &ProviderDefinition,
    api_key: &str,
    base_url: Option<&str>,
    model: Option<&str>,
) -> Arc<dyn AiProvider> {
    let endpoint = base_url.unwrap_or(definition.default_endpoint);
    let model = model.unwrap_or(definition.recommended_model);
    match definition.protocol {
        ProviderProtocol::OpenAiCompatible => {
            Arc::new(OpenAiCompatibleProvider::new(api_key, endpoint, model))
        }
        ProviderProtocol::Anthropic => Arc::new(AnthropicProvider::new(api_key, endpoint, model)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_definitions_cover_the_three_services() {
        let definitions = provider_definitions();
        let ids: Vec<_> = definitions.iter().map(|definition| definition.id).collect();
        assert_eq!(ids, vec!["openai", "claude", "deepseek"]);
        let claude = find_provider("claude").expect("claude");
        assert_eq!(claude.protocol, ProviderProtocol::Anthropic);
        assert_eq!(
            claude.default_endpoint,
            "https://api.anthropic.com/v1/messages"
        );
        assert!(!claude.models.is_empty());
    }

    #[test]
    fn unknown_provider_is_not_found() {
        assert!(find_provider("not-a-provider").is_none());
    }

    #[tokio::test]
    async fn claude_definition_builds_anthropic_provider() {
        let definition = find_provider("claude").expect("claude");
        let provider = build_provider(definition, "sk-test", None, None);
        assert_eq!(provider.provider_id(), "anthropic");
        assert_eq!(provider.model_id(), definition.recommended_model);
    }

    #[tokio::test]
    async fn openai_definition_builds_openai_compatible_provider() {
        let definition = find_provider("openai").expect("openai");
        let provider = build_provider(definition, "sk-test", None, None);
        assert_eq!(provider.provider_id(), "openai-compatible");
        assert_eq!(provider.model_id(), "gpt-4o-mini");
    }

    #[tokio::test]
    async fn explicit_model_and_endpoint_override_defaults() {
        let definition = find_provider("openai").expect("openai");
        let provider = build_provider(
            definition,
            "sk-test",
            Some("https://example.com/v1/chat/completions"),
            Some("custom-model"),
        );
        assert_eq!(provider.model_id(), "custom-model");
    }
}
