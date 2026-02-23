//! LLM client type wrapper - supports both standard HTTP and Codex providers.

use crate::codex_adapter::CodexAdapter;
use crate::interfaces::RuntimeError;
use crate::llm_client::LLMClient;
use crate::types::{LLMResponse, Message};

/// Enum wrapper for different LLM client types.
pub enum LLMClientType {
    Standard(LLMClient),
    Codex(CodexAdapter),
}

impl LLMClientType {
    /// Call LLM with unified interface.
    pub async fn call(
        &self,
        system_prompt: &str,
        messages: &[Message],
        tools: &[String],
    ) -> Result<LLMResponse, RuntimeError> {
        match self {
            Self::Standard(client) => client.call(system_prompt, messages, tools).await,
            Self::Codex(adapter) => adapter.call(system_prompt, messages, tools).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enum_variants_exist() {
        // Just verify the enum compiles and variants are accessible
        let _standard = LLMClientType::Standard(
            LLMClient::new("http://test".to_string(), 3)
        );
        
        // Can't easily test Codex without OAuth, but verify it compiles
        assert!(true);
    }
}
