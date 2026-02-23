//! Agent loop - the core runtime kernel.

use crate::compactor::{Compactor, Summarizer};
use crate::interfaces::{LockManager, RuntimeError, SessionStore, ToolDispatcher, ToolRegistry};
use crate::llm_client_type::LLMClientType;
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
    llm_client: LLMClientType,
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
        llm_client: LLMClientType,
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

        // Get available tool schemas
        let tool_schemas = self.tool_registry.get_tool_schemas(agent_id);
        
        // CRITICAL: Fail early if no tools available
        if tool_schemas.is_empty() {
            warn!("No tools available for agent: {}", agent_id);
            return Err(RuntimeError::LLMError(
                "Agent has no tools registered. Cannot execute OS operations.".to_string()
            ));
        }
        
        info!("Agent {} has {} tools available", agent_id, tool_schemas.len());

        // Execute LLM loop
        let final_response = self
            .execute_loop(
                session_key,
                system_prompt,
                user_message,
                &mut messages,
                &tool_schemas,
            )
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
        user_message: &str,
        messages: &mut Vec<Message>,
        tool_schemas: &[serde_json::Value],
    ) -> Result<String, RuntimeError> {
        // Reinforce system prompt with tool capability
        let tool_names: Vec<String> = tool_schemas
            .iter()
            .filter_map(|schema| {
                schema.get("function")
                    .and_then(|f| f.get("name"))
                    .and_then(|n| n.as_str())
                    .map(|s| s.to_string())
            })
            .collect();
        
        let reinforced_prompt = format!(
            "{}\n\nYou are a local autonomous Linux agent. You MUST use tools to perform file, process, wallpaper, or system operations. Do not describe actions — call the appropriate tool.\n\nAvailable tools: {}",
            system_prompt,
            tool_names.join(", ")
        );
        
        let action_requires_tool = requires_tool_call_for_user_message(user_message);
        let mut saw_tool_call = false;

        for iteration in 0..self.max_iterations {
            debug!(
                "LLM loop iteration {}/{}",
                iteration + 1,
                self.max_iterations
            );

            // Call LLM with reinforced prompt
            let response = self
                .llm_client
                .call(&reinforced_prompt, messages, tool_schemas)
                .await
                .map_err(|e| {
                    error!("LLM call failed: {}", e);
                    e
                })?;

            // Handle response type
            match response {
                LLMResponse::Final { content, .. } => {
                    if action_requires_tool && !saw_tool_call {
                        return Err(RuntimeError::ToolError(
                            "Tool invocation required but not performed".to_string(),
                        ));
                    }
                    info!("LLM returned final response after {} iterations", iteration + 1);
                    return Ok(content);
                }
                LLMResponse::ToolCall { tool_name, input, .. } => {
                    saw_tool_call = true;
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
                    let tool_result = match self
                        .tool_dispatcher
                        .execute(&tool_name, &input, session_key)
                        .await
                    {
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

fn requires_tool_call_for_user_message(user_message: &str) -> bool {
    let lower = user_message.to_lowercase();
    let action_tokens = [
        "create",
        "delete",
        "remove",
        "move",
        "copy",
        "read",
        "write",
        "list",
        "open",
        "switch",
        "workspace",
        "focus",
        "close",
        "wallpaper",
        "spawn",
        "start",
        "stop",
        "kill",
        "run",
        "execute",
        "build",
        "install",
        "shutdown",
        "reboot",
    ];
    action_tokens.iter().any(|token| lower.contains(token))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::LLMClient;
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

    #[async_trait]
    impl ToolDispatcher for MockToolDispatcher {
        async fn execute(
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
        
        fn get_tool_schemas(&self, _agent_id: &str) -> Vec<serde_json::Value> {
            vec![
                json!({
                    "type": "function",
                    "function": {
                        "name": "echo",
                        "description": "Echo a message",
                        "parameters": {
                            "type": "object",
                            "properties": {
                                "message": {"type": "string"}
                            },
                            "required": ["message"]
                        }
                    }
                })
            ]
        }
    }

    struct MockSummarizer;

    impl Summarizer for MockSummarizer {
        fn summarize(&self, messages: &[Message]) -> Result<String, RuntimeError> {
            Ok(format!("Summary of {} messages", messages.len()))
        }
    }

    fn create_mock_llm_client(_responses: Vec<LLMResponse>) -> LLMClientType {
        // For testing, we'll need to mock the HTTP calls
        // This is a simplified version - in real tests we'd use a mock server
        LLMClientType::Standard(LLMClient::new("http://localhost:8000".to_string(), 0))
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
