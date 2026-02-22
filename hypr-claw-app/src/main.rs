use std::io::{self, Write};
use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create directories
    std::fs::create_dir_all("./data/sessions")?;
    std::fs::create_dir_all("./data/credentials")?;
    std::fs::create_dir_all("./data/agents")?;
    if !std::path::Path::new("./data/audit.log").exists() {
        std::fs::File::create("./data/audit.log")?;
    }
    std::fs::create_dir_all("./sandbox")?;

    // Create default agent config if it doesn't exist
    let default_agent_config = "./data/agents/default.yaml";
    let default_agent_soul = "./data/agents/default_soul.md";
    if !std::path::Path::new(default_agent_config).exists() {
        std::fs::write(
            default_agent_config,
            "id: default\nsoul: default_soul.md\ntools:\n  - echo\n  - file_read\n  - file_write\n  - file_list\n  - shell_exec\n"
        )?;
    }
    if !std::path::Path::new(default_agent_soul).exists() {
        std::fs::write(
            default_agent_soul,
            "You are a helpful assistant with access to file operations and shell commands."
        )?;
    }

    // Initialize infrastructure
    let session_store = Arc::new(hypr_claw::infra::session_store::SessionStore::new("./data/sessions")?);
    let lock_manager = Arc::new(hypr_claw::infra::lock_manager::LockManager::new(Duration::from_secs(30)));
    let permission_engine = Arc::new(hypr_claw::infra::permission_engine::PermissionEngine::new());
    let audit_logger = Arc::new(hypr_claw::infra::audit_logger::AuditLogger::new("./data/audit.log")?);

    // Wrap in async adapters
    let async_session = Arc::new(hypr_claw_runtime::AsyncSessionStore::new(session_store));
    let async_locks = Arc::new(hypr_claw_runtime::AsyncLockManager::new(lock_manager));

    // Create tool registry
    let mut registry = hypr_claw_tools::ToolRegistryImpl::new();
    registry.register(Arc::new(hypr_claw_tools::tools::EchoTool));
    registry.register(Arc::new(
        hypr_claw_tools::tools::FileReadTool::new("./sandbox")
            .map_err(|e| format!("Failed to create FileReadTool: {}", e))?
    ));
    registry.register(Arc::new(
        hypr_claw_tools::tools::FileWriteTool::new("./sandbox")
            .map_err(|e| format!("Failed to create FileWriteTool: {}", e))?
    ));
    registry.register(Arc::new(
        hypr_claw_tools::tools::FileListTool::new("./sandbox")
            .map_err(|e| format!("Failed to create FileListTool: {}", e))?
    ));
    registry.register(Arc::new(hypr_claw_tools::tools::ShellExecTool));

    let registry_arc = Arc::new(registry);

    // Create tool dispatcher
    let dispatcher = Arc::new(hypr_claw_tools::ToolDispatcherImpl::new(
        registry_arc.clone(),
        permission_engine as Arc<dyn hypr_claw_tools::PermissionEngine>,
        audit_logger as Arc<dyn hypr_claw_tools::AuditLogger>,
        5000,
    ));

    // Create runtime adapters
    let runtime_dispatcher = Arc::new(RuntimeDispatcherAdapter::new(dispatcher));
    let runtime_registry = Arc::new(RuntimeRegistryAdapter::new(registry_arc));

    // Get user input
    print!("Enter LLM base URL: ");
    io::stdout().flush()?;
    let mut llm_url = String::new();
    io::stdin().read_line(&mut llm_url)?;
    let llm_url = llm_url.trim();

    print!("Enter agent name: ");
    io::stdout().flush()?;
    let mut agent_name = String::new();
    io::stdin().read_line(&mut agent_name)?;
    let agent_name = agent_name.trim();

    print!("Enter your task: ");
    io::stdout().flush()?;
    let mut task = String::new();
    io::stdin().read_line(&mut task)?;
    let task = task.trim();

    // Initialize LLM client
    let llm_client = hypr_claw_runtime::LLMClient::new(llm_url.to_string(), 1);

    // Create compactor
    let compactor = hypr_claw_runtime::Compactor::new(4000, SimpleSummarizer);

    // Create agent loop
    let agent_loop = hypr_claw_runtime::AgentLoop::new(
        async_session,
        async_locks,
        runtime_dispatcher,
        runtime_registry,
        llm_client,
        compactor,
        10,
    );

    // Create runtime controller
    let controller = hypr_claw_runtime::RuntimeController::new(agent_loop, "./data/agents".to_string());

    // Execute
    println!("\n=== Executing ===");
    match controller.execute("default_user", agent_name, task).await {
        Ok(response) => {
            println!("\n=== Response ===");
            println!("{}", response);
            Ok(())
        }
        Err(e) => {
            eprintln!("\n=== Error ===");
            eprintln!("{}", e);
            Err(Box::new(e) as Box<dyn std::error::Error>)
        }
    }
}

// Adapter for ToolDispatcher
struct RuntimeDispatcherAdapter {
    inner: Arc<hypr_claw_tools::ToolDispatcherImpl>,
}

impl RuntimeDispatcherAdapter {
    fn new(inner: Arc<hypr_claw_tools::ToolDispatcherImpl>) -> Self {
        Self { inner }
    }
}

impl hypr_claw_runtime::ToolDispatcher for RuntimeDispatcherAdapter {
    fn execute(
        &self,
        tool_name: &str,
        input: &serde_json::Value,
        session_key: &str,
    ) -> Result<serde_json::Value, hypr_claw_runtime::RuntimeError> {
        let inner = self.inner.clone();
        let tool_name = tool_name.to_string();
        let input = input.clone();
        let session_key = session_key.to_string();

        let handle = tokio::runtime::Handle::current();
        let result = handle.block_on(async move {
            inner.dispatch(session_key, tool_name, input).await
        });

        match result {
            Ok(tool_result) => {
                if tool_result.success {
                    Ok(tool_result.output.unwrap_or(serde_json::json!({})))
                } else {
                    Err(hypr_claw_runtime::RuntimeError::ToolError(
                        tool_result.error.unwrap_or_else(|| "Unknown error".to_string())
                    ))
                }
            }
            Err(e) => Err(hypr_claw_runtime::RuntimeError::ToolError(e.to_string())),
        }
    }
}

// Adapter for ToolRegistry
struct RuntimeRegistryAdapter {
    inner: Arc<hypr_claw_tools::ToolRegistryImpl>,
}

impl RuntimeRegistryAdapter {
    fn new(inner: Arc<hypr_claw_tools::ToolRegistryImpl>) -> Self {
        Self { inner }
    }
}

impl hypr_claw_runtime::ToolRegistry for RuntimeRegistryAdapter {
    fn get_active_tools(&self, _agent_id: &str) -> Vec<String> {
        self.inner.list()
    }
}

// Simple summarizer implementation
struct SimpleSummarizer;

impl hypr_claw_runtime::Summarizer for SimpleSummarizer {
    fn summarize(
        &self,
        messages: &[hypr_claw_runtime::Message],
    ) -> Result<String, hypr_claw_runtime::RuntimeError> {
        Ok(format!("Summary of {} messages", messages.len()))
    }
}
