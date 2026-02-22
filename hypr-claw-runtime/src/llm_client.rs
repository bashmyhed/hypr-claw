//! LLM client for HTTP communication with Python service.

use crate::interfaces::RuntimeError;
use crate::types::{LLMResponse, Message};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use parking_lot::Mutex;
use tracing::{debug, warn};

/// Request payload for LLM service.
#[derive(Debug, Serialize)]
struct LLMRequest {
    system_prompt: String,
    messages: Vec<Message>,
    tools: Vec<String>,
}

/// OpenAI-compatible request format for NVIDIA/Google
#[derive(Debug, Serialize)]
struct OpenAIRequest {
    model: String,
    messages: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
}

/// OpenAI-compatible response format
#[derive(Debug, Deserialize)]
struct OpenAIResponse {
    choices: Vec<OpenAIChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenAIChoice {
    message: OpenAIMessage,
}

#[derive(Debug, Deserialize)]
struct OpenAIMessage {
    content: Option<String>,
    tool_calls: Option<Vec<OpenAIToolCall>>,
}

#[derive(Debug, Deserialize)]
struct OpenAIToolCall {
    function: OpenAIFunction,
}

#[derive(Debug, Deserialize)]
struct OpenAIFunction {
    name: String,
    arguments: String,
}

/// Circuit breaker state.
struct CircuitBreaker {
    consecutive_failures: AtomicUsize,
    breaker_open: AtomicBool,
    opened_at: Mutex<Option<Instant>>,
    failure_threshold: usize,
    cooldown_duration: Duration,
}

impl CircuitBreaker {
    fn new(failure_threshold: usize, cooldown_duration: Duration) -> Self {
        Self {
            consecutive_failures: AtomicUsize::new(0),
            breaker_open: AtomicBool::new(false),
            opened_at: Mutex::new(None),
            failure_threshold,
            cooldown_duration,
        }
    }

    fn record_success(&self) {
        self.consecutive_failures.store(0, Ordering::SeqCst);
        self.breaker_open.store(false, Ordering::SeqCst);
        *self.opened_at.lock() = None;
    }

    fn record_failure(&self) {
        let failures = self.consecutive_failures.fetch_add(1, Ordering::SeqCst) + 1;
        if failures >= self.failure_threshold {
            self.breaker_open.store(true, Ordering::SeqCst);
            *self.opened_at.lock() = Some(Instant::now());
        }
    }

    fn should_allow_request(&self) -> Result<(), RuntimeError> {
        if !self.breaker_open.load(Ordering::SeqCst) {
            return Ok(());
        }

        let opened_at = self.opened_at.lock();
        if let Some(opened_time) = *opened_at {
            if opened_time.elapsed() >= self.cooldown_duration {
                drop(opened_at);
                // Allow trial request
                return Ok(());
            }
        }

        Err(RuntimeError::LLMError(
            "Circuit breaker open: LLM service unavailable".to_string(),
        ))
    }
}

/// LLM client for calling Python service via HTTP.
#[derive(Clone)]
pub struct LLMClient {
    base_url: String,
    client: reqwest::Client,
    max_retries: u32,
    circuit_breaker: Arc<CircuitBreaker>,
    api_key: Option<String>,
    model: Option<String>,
}

impl LLMClient {
    /// Create a new LLM client.
    ///
    /// # Arguments
    /// * `base_url` - Base URL of the LLM service
    /// * `max_retries` - Maximum number of retries on failure
    pub fn new(base_url: String, max_retries: u32) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        
        Self {
            base_url,
            client,
            max_retries,
            circuit_breaker: Arc::new(CircuitBreaker::new(5, Duration::from_secs(30))),
            api_key: None,
            model: None,
        }
    }

    /// Create a new LLM client with API key for authentication.
    pub fn with_api_key(base_url: String, max_retries: u32, api_key: String) -> Self {
        let mut client = Self::new(base_url, max_retries);
        client.api_key = Some(api_key);
        client
    }

    /// Create a new LLM client with API key and model.
    pub fn with_api_key_and_model(base_url: String, max_retries: u32, api_key: String, model: String) -> Self {
        let mut client = Self::new(base_url, max_retries);
        client.api_key = Some(api_key);
        client.model = Some(model);
        client
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
        let _timer = crate::metrics::MetricTimer::new("llm_request_latency");

        // Check circuit breaker
        self.circuit_breaker.should_allow_request()?;

        let mut last_error = None;
        
        for attempt in 0..=self.max_retries {
            debug!("LLM call attempt {}/{}", attempt + 1, self.max_retries + 1);
            
            match self.call_once(system_prompt, messages, tools).await {
                Ok(response) => {
                    self.circuit_breaker.record_success();
                    return Ok(response);
                }
                Err(e) => {
                    warn!("LLM call failed (attempt {}): {}", attempt + 1, e);
                    last_error = Some(e);
                    if attempt < self.max_retries {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                }
            }
        }
        
        self.circuit_breaker.record_failure();
        
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
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        
        let mut req_builder = self.client.post(&url);

        // Use OpenAI format if model is specified (NVIDIA/Google), otherwise use custom format
        if let Some(model) = &self.model {
            // Convert to OpenAI format
            let mut openai_messages = Vec::new();
            
            // Add system message if present
            if !system_prompt.is_empty() {
                openai_messages.push(serde_json::json!({
                    "role": "system",
                    "content": system_prompt
                }));
            }
            
            // Add conversation messages
            for msg in messages {
                let role_str = match msg.role {
                    crate::types::Role::User => "user",
                    crate::types::Role::Assistant => "assistant",
                    crate::types::Role::Tool => "tool",
                    crate::types::Role::System => "system",
                };
                openai_messages.push(serde_json::json!({
                    "role": role_str,
                    "content": &msg.content
                }));
            }
            
            let openai_request = OpenAIRequest {
                model: model.clone(),
                messages: openai_messages,
                max_tokens: Some(2048),
            };
            
            println!("DEBUG: FINAL_URL = {}", url);
            println!("DEBUG: Has API key = {}", self.api_key.is_some());
            println!("DEBUG: Request body = {}", serde_json::to_string_pretty(&openai_request).unwrap_or_default());
            
            req_builder = req_builder.json(&openai_request);
        } else {
            // Use custom format for local/Python service
            let request = LLMRequest {
                system_prompt: system_prompt.to_string(),
                messages: messages.to_vec(),
                tools: tools.to_vec(),
            };
            
            println!("DEBUG: FINAL_URL = {}", url);
            println!("DEBUG: Request body = {}", serde_json::to_string_pretty(&request).unwrap_or_default());
            
            req_builder = req_builder.json(&request);
        }

        // Add Authorization header if API key is present
        if let Some(api_key) = &self.api_key {
            req_builder = req_builder.header("Authorization", format!("Bearer {}", api_key));
        }

        let response = req_builder
            .send()
            .await
            .map_err(|e| {
                if e.is_connect() || e.is_timeout() {
                    RuntimeError::LLMError("Network connection failed".to_string())
                } else {
                    RuntimeError::LLMError(format!("HTTP request failed: {}", e))
                }
            })?;
        
        let status = response.status();
        if !status.is_success() {
            // Try to get error details from response body
            let error_body = response.text().await.unwrap_or_else(|_| "Unable to read error response".to_string());
            
            let error_msg = match status.as_u16() {
                401 => {
                    if self.api_key.is_some() {
                        format!("Authentication failed. Check your API key. Details: {}", error_body)
                    } else {
                        format!("Authentication required (401 Unauthorized). Details: {}", error_body)
                    }
                }
                404 => format!("Invalid endpoint (404 Not Found). Details: {}", error_body),
                429 => {
                    if self.api_key.is_some() {
                        format!("Rate limit exceeded. Details: {}", error_body)
                    } else {
                        format!("Rate limit exceeded (429 Too Many Requests). Details: {}", error_body)
                    }
                }
                500..=599 => {
                    if self.api_key.is_some() {
                        format!("LLM service error. Details: {}", error_body)
                    } else {
                        format!("Server error: {}. Details: {}", status, error_body)
                    }
                }
                _ => format!("HTTP error: {}. Details: {}", status, error_body),
            };
            return Err(RuntimeError::LLMError(error_msg));
        }
        
        let llm_response: LLMResponse = if self.model.is_some() {
            // Parse OpenAI format response
            let openai_response: OpenAIResponse = response
                .json()
                .await
                .map_err(|e| RuntimeError::LLMError(format!("Failed to parse response: {}", e)))?;
            
            // Convert to our format
            if let Some(choice) = openai_response.choices.first() {
                if let Some(tool_calls) = &choice.message.tool_calls {
                    if let Some(tool_call) = tool_calls.first() {
                        LLMResponse::ToolCall {
                            schema_version: crate::types::SCHEMA_VERSION,
                            tool_name: tool_call.function.name.clone(),
                            input: serde_json::from_str(&tool_call.function.arguments)
                                .unwrap_or(serde_json::json!({})),
                        }
                    } else {
                        LLMResponse::Final {
                            schema_version: crate::types::SCHEMA_VERSION,
                            content: choice.message.content.clone().unwrap_or_default(),
                        }
                    }
                } else {
                    LLMResponse::Final {
                        schema_version: crate::types::SCHEMA_VERSION,
                        content: choice.message.content.clone().unwrap_or_default(),
                    }
                }
            } else {
                return Err(RuntimeError::LLMError("No choices in response".to_string()));
            }
        } else {
            // Parse custom format response
            response
                .json()
                .await
                .map_err(|e| RuntimeError::LLMError(format!("Failed to parse response: {}", e)))?
        };
        
        self.validate_response(&llm_response)?;
        
        Ok(llm_response)
    }

    fn validate_response(&self, response: &LLMResponse) -> Result<(), RuntimeError> {
        match response {
            LLMResponse::Final { content, .. } => {
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
            schema_version: crate::types::SCHEMA_VERSION,
            content: "Hello".to_string(),
        };
        assert!(client.validate_response(&response).is_ok());
    }

    #[test]
    fn test_validate_empty_final_response() {
        let client = LLMClient::new("http://localhost:8000".to_string(), 1);
        
        let response = LLMResponse::Final {
            schema_version: crate::types::SCHEMA_VERSION,
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
            schema_version: crate::types::SCHEMA_VERSION,
            tool_name: "search".to_string(),
            input: json!({"query": "test"}),
        };
        assert!(client.validate_response(&response).is_ok());
    }

    #[test]
    fn test_validate_empty_tool_name() {
        let client = LLMClient::new("http://localhost:8000".to_string(), 1);
        
        let response = LLMResponse::ToolCall {
            schema_version: crate::types::SCHEMA_VERSION,
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
