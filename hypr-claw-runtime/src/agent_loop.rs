//! Agent loop - the core runtime kernel.

use crate::compactor::{Compactor, Summarizer};
use crate::interfaces::{LockManager, RuntimeError, SessionStore, ToolDispatcher, ToolRegistry};
use crate::llm_client::LLMClient;
use crate::types::{LLMResponse, Message, Role};
use serde_json::json;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

/// Core agent execution loop.
pub struct AgentLoop<S, L, D, R, Sum>
where
    S: SessionStore,
    L: LockManager,
    D: ToolDispatcher,
    R: ToolRegistry,
    Sum: Summarizer,
{
    session_store: Arc<S>,
    lock_manager: Arc<L>,
    tool_dispatcher: Arc<D>,
    tool_registry: Arc<R>,
    llm_client: LLMClient,
    compactor: Compactor<Sum>,
    max_iterations: usize,
}

impl<S, L, D, R, Sum> AgentLoop<S, L, D, R, Sum>
where
    S: SessionStore,
    L: LockManager,
    D: ToolDispatcher,
    R: ToolRegistry,
    Sum: Summarizer,
{
    /// Create a new agent loop.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        session_store: Arc<S>,
        lock_manager: Arc<L>,
        tool_dispatcher: Arc<D>,
        tool_registry: Arc<R>,
        llm_client: LLMClient,
        compactor: Compactor<Sum>,
        max_iterations: usize,
    ) -> Self {
        Self {
            session_store,
            lock_manager,
            tool_dispatcher,
            tool_registry,
            llm_client,
            compactor,
            max_iterations,
        }
    }

    /// Execute agent loop for a user message.
    ///
    /// # Arguments
    /// * `session_key` - Session identifier
    /// * `agent_id` - Agent identifier
    /// * `system_prompt` - System prompt (agent soul)
    /// * `user_message` - User's input message
    ///
    /// # Returns
    /// Final assistant response
    pub async fn run(
        &self,
        session_key: &str,
        agent_id: &str,
        system_prompt: &str,
        user_message: &str,
    ) -> Result<String, RuntimeError> {
        // Acquire lock
        info!("Acquiring lock for session: {}", session_key);
        self.lock_manager.acquire(session_key).await?;

        // Ensure lock is always released
        let result = self
            .run_inner(session_key, agent_id, system_prompt, user_message)
            .await;

        // Release lock
        info!("Releasing lock for session: {}", session_key);
        self.lock_manager.release(session_key).await;

        result
    }

    async fn run_inner(
        &self,
        session_key: &str,
        agent_id: &str,
        system_prompt: &str,
        user_message: &str,
    ) -> Result<String, RuntimeError> {
        // Load session
        debug!("Loading session: {}", session_key);
        let mut messages = self.session_store.load(session_key).await?;

        // Compact if needed
        messages = self.compactor.compact(messages)?;

        // Append user message
        messages.push(Message::new(Role::User, json!(user_message)));

        // Get available tools
        let tools = self.tool_registry.get_active_tools(agent_id);

        // Execute LLM loop
        let final_response = self
            .execute_loop(session_key, system_prompt, &mut messages, &tools)
            .await?;

        // Append final response
        messages.push(Message::new(Role::Assistant, json!(final_response.clone())));

        // Save session
        debug!("Saving session: {}", session_key);
        self.session_store.save(session_key, &messages).await?;

        Ok(final_response)
    }

    async fn execute_loop(
        &self,
        session_key: &str,
        system_prompt: &str,
        messages: &mut Vec<Message>,
        tools: &[String],
    ) -> Result<String, RuntimeError> {
        for iteration in 0..self.max_iterations {
            debug!(
                "LLM loop iteration {}/{}",
                iteration + 1,
                self.max_iterations
            );

            // Call LLM
            let response = self
                .llm_client
                .call(system_prompt, messages, tools)
                .await
                .map_err(|e| {
                    error!("LLM call failed: {}", e);
                    e
                })?;

            // Handle response type
            match response {
                LLMResponse::Final { content, .. } => {
                    info!("LLM returned final response after {} iterations", iteration + 1);
                    return Ok(content);
                }
                LLMResponse::ToolCall { tool_name, input, .. } => {
                    info!("LLM requested tool: {}", tool_name);

                    // Append tool call message
                    messages.push(Message::with_metadata(
                        Role::Assistant,
                        json!(format!("Calling tool: {}", tool_name)),
                        json!({
                            "tool_call": true,
                            "tool_name": tool_name.clone(),
                            "input": input.clone()
                        }),
                    ));

                    // Execute tool
                    let tool_result = match self.tool_dispatcher.execute(&tool_name, &input, session_key) {
                        Ok(result) => result,
                        Err(e) => {
                            warn!("Tool execution failed: {}", e);
                            json!({"error": e.to_string()})
                        }
                    };

                    // Append tool result
                    messages.push(Message::with_metadata(
                        Role::Tool,
                        tool_result,
                        json!({"tool_name": tool_name}),
                    ));

                    // Continue loop
                }
            }
        }

        // Max iterations exceeded
        Err(RuntimeError::LLMError(format!(
            "Max iterations ({}) exceeded without final response",
            self.max_iterations
        )))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use async_trait::async_trait;

    // Mock implementations
    struct MockSessionStore {
        storage: Mutex<HashMap<String, Vec<Message>>>,
    }

    impl MockSessionStore {
        fn new() -> Self {
            Self {
                storage: Mutex::new(HashMap::new()),
            }
        }
    }

    #[async_trait]
    impl SessionStore for MockSessionStore {
        async fn load(&self, session_key: &str) -> Result<Vec<Message>, RuntimeError> {
            let storage = self.storage.lock().unwrap();
            Ok(storage.get(session_key).cloned().unwrap_or_default())
        }

        async fn save(&self, session_key: &str, messages: &[Message]) -> Result<(), RuntimeError> {
            let mut storage = self.storage.lock().unwrap();
            storage.insert(session_key.to_string(), messages.to_vec());
            Ok(())
        }
    }

    struct MockLockManager {
        locks: Mutex<std::collections::HashSet<String>>,
    }

    impl MockLockManager {
        fn new() -> Self {
            Self {
                locks: Mutex::new(std::collections::HashSet::new()),
            }
        }
    }

    #[async_trait]
    impl LockManager for MockLockManager {
        async fn acquire(&self, session_key: &str) -> Result<(), RuntimeError> {
            let mut locks = self.locks.lock().unwrap();
            if locks.contains(session_key) {
                return Err(RuntimeError::LockError(format!(
                    "Lock already held: {}",
                    session_key
                )));
            }
            locks.insert(session_key.to_string());
            Ok(())
        }

        async fn release(&self, session_key: &str) {
            let mut locks = self.locks.lock().unwrap();
            locks.remove(session_key);
        }
    }

    struct MockToolDispatcher;

    impl ToolDispatcher for MockToolDispatcher {
        fn execute(
            &self,
            tool_name: &str,
            _input: &serde_json::Value,
            _session_key: &str,
        ) -> Result<serde_json::Value, RuntimeError> {
            Ok(json!({"status": "success", "tool": tool_name}))
        }
    }

    struct MockToolRegistry;

    impl ToolRegistry for MockToolRegistry {
        fn get_active_tools(&self, _agent_id: &str) -> Vec<String> {
            vec![]
        }
    }

    struct MockSummarizer;

    impl Summarizer for MockSummarizer {
        fn summarize(&self, messages: &[Message]) -> Result<String, RuntimeError> {
            Ok(format!("Summary of {} messages", messages.len()))
        }
    }

    fn create_mock_llm_client(_responses: Vec<LLMResponse>) -> LLMClient {
        // For testing, we'll need to mock the HTTP calls
        // This is a simplified version - in real tests we'd use a mock server
        LLMClient::new("http://localhost:8000".to_string(), 0)
    }

    #[tokio::test]
    async fn test_lock_always_released_on_error() {
        let store = Arc::new(MockSessionStore::new());
        let lock_mgr = Arc::new(MockLockManager::new());
        let dispatcher = Arc::new(MockToolDispatcher);
        let registry = Arc::new(MockToolRegistry);
        let llm_client = create_mock_llm_client(vec![]);
        let compactor = Compactor::new(10000, MockSummarizer);

        let agent_loop = AgentLoop::new(
            store,
            lock_mgr.clone(),
            dispatcher,
            registry,
            llm_client,
            compactor,
            10,
        );

        // This will fail because the mock LLM client can't actually make calls
        let result = agent_loop.run("agent:user1", "agent", "You are helpful", "Hi").await;
        assert!(result.is_err());

        // Lock should be released
        let locks = lock_mgr.locks.lock().unwrap();
        assert!(!locks.contains("agent:user1"));
    }

    #[test]
    fn test_agent_loop_creation() {
        let store = Arc::new(MockSessionStore::new());
        let lock_mgr = Arc::new(MockLockManager::new());
        let dispatcher = Arc::new(MockToolDispatcher);
        let registry = Arc::new(MockToolRegistry);
        let llm_client = create_mock_llm_client(vec![]);
        let compactor = Compactor::new(10000, MockSummarizer);

        let agent_loop = AgentLoop::new(
            store,
            lock_mgr,
            dispatcher,
            registry,
            llm_client,
            compactor,
            10,
        );

        assert_eq!(agent_loop.max_iterations, 10);
    }
}
