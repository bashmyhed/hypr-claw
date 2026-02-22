//! LLM client for HTTP communication with Python service.

use crate::interfaces::RuntimeError;
use crate::types::{LLMResponse, Message};
use serde::Serialize;
use std::time::Duration;
use tracing::{debug, warn};

/// Request payload for LLM service.
#[derive(Debug, Serialize)]
struct LLMRequest {
    system_prompt: String,
    messages: Vec<Message>,
    tools: Vec<String>,
}

/// LLM client for calling Python service via HTTP.
#[derive(Clone)]
pub struct LLMClient {
    base_url: String,
    client: reqwest::Client,
    max_retries: u32,
}

impl LLMClient {
    /// Create a new LLM client.
    ///
    /// # Arguments
    /// * `base_url` - Base URL of the Python LLM service
    /// * `max_retries` - Maximum number of retries on failure
    pub fn new(base_url: String, max_retries: u32) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        
        Self {
            base_url,
            client,
            max_retries,
        }
    }

    /// Call LLM service with retry logic.
    ///
    /// # Arguments
    /// * `system_prompt` - System prompt for the LLM
    /// * `messages` - Conversation history
    /// * `tools` - Available tools
    ///
    /// # Returns
    /// Normalized LLMResponse
    ///
    /// # Errors
    /// Returns error if all retries fail
    pub async fn call(
        &self,
        system_prompt: &str,
        messages: &[Message],
        tools: &[String],
    ) -> Result<LLMResponse, RuntimeError> {
        let mut last_error = None;
        
        for attempt in 0..=self.max_retries {
            debug!("LLM call attempt {}/{}", attempt + 1, self.max_retries + 1);
            
            match self.call_once(system_prompt, messages, tools).await {
                Ok(response) => return Ok(response),
                Err(e) => {
                    warn!("LLM call failed (attempt {}): {}", attempt + 1, e);
                    last_error = Some(e);
                    if attempt < self.max_retries {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                }
            }
        }
        
        Err(RuntimeError::LLMError(format!(
            "LLM call failed after {} attempts: {}",
            self.max_retries + 1,
            last_error.map(|e| e.to_string()).unwrap_or_else(|| "Unknown error".to_string())
        )))
    }

    async fn call_once(
        &self,
        system_prompt: &str,
        messages: &[Message],
        tools: &[String],
    ) -> Result<LLMResponse, RuntimeError> {
        let request = LLMRequest {
            system_prompt: system_prompt.to_string(),
            messages: messages.to_vec(),
            tools: tools.to_vec(),
        };
        
        let response = self
            .client
            .post(format!("{}/llm/call", self.base_url))
            .json(&request)
            .send()
            .await
            .map_err(|e| RuntimeError::LLMError(format!("HTTP request failed: {}", e)))?;
        
        if !response.status().is_success() {
            return Err(RuntimeError::LLMError(format!(
                "HTTP error: {}",
                response.status()
            )));
        }
        
        let llm_response: LLMResponse = response
            .json()
            .await
            .map_err(|e| RuntimeError::LLMError(format!("Failed to parse response: {}", e)))?;
        
        self.validate_response(&llm_response)?;
        
        Ok(llm_response)
    }

    fn validate_response(&self, response: &LLMResponse) -> Result<(), RuntimeError> {
        match response {
            LLMResponse::Final { content } => {
                if content.is_empty() {
                    return Err(RuntimeError::LLMError(
                        "Final response has empty content".to_string(),
                    ));
                }
            }
            LLMResponse::ToolCall { tool_name, .. } => {
                if tool_name.is_empty() {
                    return Err(RuntimeError::LLMError(
                        "Tool call missing tool_name".to_string(),
                    ));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::types::Role;
    use serde_json::json;

    // Mock server tests would use wiremock or similar
    // For now, we test the validation logic

    #[test]
    fn test_validate_final_response() {
        let client = LLMClient::new("http://localhost:8000".to_string(), 1);
        
        let response = LLMResponse::Final {
            content: "Hello".to_string(),
        };
        assert!(client.validate_response(&response).is_ok());
    }

    #[test]
    fn test_validate_empty_final_response() {
        let client = LLMClient::new("http://localhost:8000".to_string(), 1);
        
        let response = LLMResponse::Final {
            content: "".to_string(),
        };
        let result = client.validate_response(&response);
        assert!(result.is_err());
        match result {
            Err(RuntimeError::LLMError(msg)) => {
                assert!(msg.contains("empty content"));
            }
            _ => panic!("Expected LLMError"),
        }
    }

    #[test]
    fn test_validate_tool_call_response() {
        let client = LLMClient::new("http://localhost:8000".to_string(), 1);
        
        let response = LLMResponse::ToolCall {
            tool_name: "search".to_string(),
            input: json!({"query": "test"}),
        };
        assert!(client.validate_response(&response).is_ok());
    }

    #[test]
    fn test_validate_empty_tool_name() {
        let client = LLMClient::new("http://localhost:8000".to_string(), 1);
        
        let response = LLMResponse::ToolCall {
            tool_name: "".to_string(),
            input: json!({"query": "test"}),
        };
        let result = client.validate_response(&response);
        assert!(result.is_err());
        match result {
            Err(RuntimeError::LLMError(msg)) => {
                assert!(msg.contains("missing tool_name"));
            }
            _ => panic!("Expected LLMError"),
        }
    }

    #[test]
    fn test_client_creation() {
        let client = LLMClient::new("http://localhost:8000".to_string(), 2);
        assert_eq!(client.base_url, "http://localhost:8000");
        assert_eq!(client.max_retries, 2);
    }

    #[test]
    fn test_request_serialization() {
        let messages = vec![Message::new(Role::User, json!("Hello"))];
        let request = LLMRequest {
            system_prompt: "You are helpful".to_string(),
            messages,
            tools: vec!["search".to_string()],
        };
        
        let serialized = serde_json::to_string(&request).unwrap();
        assert!(serialized.contains("system_prompt"));
        assert!(serialized.contains("messages"));
        assert!(serialized.contains("tools"));
    }
}
