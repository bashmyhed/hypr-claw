use std::io::{self, Write};
use std::sync::Arc;
use std::time::Duration;

pub mod config;
pub mod bootstrap;

use config::{Config, LLMProvider};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse CLI arguments
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 && args[1] == "config" && args.get(2).map(|s| s.as_str()) == Some("reset") {
        return handle_config_reset();
    }

    // Print banner
    println!("╔══════════════════════════════════════════════════════════════════╗");
    println!("║              Hypr-Claw Terminal Agent                            ║");
    println!("╚══════════════════════════════════════════════════════════════════╝");
    println!();

    // Initialize directories
    if let Err(e) = initialize_directories() {
        eprintln!("❌ Failed to initialize directories: {}", e);
        return Err(e);
    }

    // Load or bootstrap configuration
    let config = if Config::exists() {
        match Config::load() {
            Ok(cfg) => {
                if let Err(e) = cfg.validate() {
                    eprintln!("❌ Invalid configuration: {}", e);
                    eprintln!("💡 Tip: Run 'hypr-claw config reset' to reconfigure");
                    return Err(e.into());
                }
                cfg
            }
            Err(e) => {
                eprintln!("❌ Failed to load config: {}", e);
                eprintln!("💡 Tip: Run 'hypr-claw config reset' to reconfigure");
                return Err(e.into());
            }
        }
    } else {
        match bootstrap::run_bootstrap() {
            Ok(cfg) => cfg,
            Err(e) => {
                eprintln!("❌ Bootstrap failed: {}", e);
                return Err(e.into());
            }
        }
    };

    // Display provider info
    let provider_name = match &config.provider {
        LLMProvider::Nvidia => "NVIDIA Kimi",
        LLMProvider::Local { .. } => "Local",
    };
    println!("Using provider: {}", provider_name);
    println!();

    // Get user input
    print!("Enter agent name [default]: ");
    io::stdout().flush()?;
    let mut agent_name_input = String::new();
    io::stdin().read_line(&mut agent_name_input)?;
    let agent_name_input = agent_name_input.trim();
    let agent_name = if agent_name_input.is_empty() { 
        "default" 
    } else { 
        // Check if agent exists, fallback to default if not
        let agent_path = format!("./data/agents/{}.yaml", agent_name_input);
        if std::path::Path::new(&agent_path).exists() {
            agent_name_input
        } else {
            eprintln!("⚠️  Agent '{}' not found, using 'default' agent", agent_name_input);
            "default"
        }
    };

    print!("Enter user ID [local_user]: ");
    io::stdout().flush()?;
    let mut user_id = String::new();
    io::stdin().read_line(&mut user_id)?;
    let user_id = user_id.trim();
    let user_id = if user_id.is_empty() { "local_user" } else { user_id };

    print!("Enter task: ");
    io::stdout().flush()?;
    let mut task = String::new();
    io::stdin().read_line(&mut task)?;
    let task = task.trim().to_string();

    if task.is_empty() {
        eprintln!("❌ Task cannot be empty");
        return Ok(());
    }

    println!("\n🔧 Initializing system...");

    // Initialize infrastructure
    let session_store = match hypr_claw::infra::session_store::SessionStore::new("./data/sessions") {
        Ok(store) => Arc::new(store),
        Err(e) => {
            eprintln!("❌ Failed to initialize session store: {}", e);
            return Err(Box::new(e));
        }
    };

    let lock_manager = Arc::new(hypr_claw::infra::lock_manager::LockManager::new(Duration::from_secs(30)));
    let permission_engine = Arc::new(hypr_claw::infra::permission_engine::PermissionEngine::new());
    
    let audit_logger = match hypr_claw::infra::audit_logger::AuditLogger::new("./data/audit.log") {
        Ok(logger) => Arc::new(logger),
        Err(e) => {
            eprintln!("❌ Failed to initialize audit logger: {}", e);
            return Err(Box::new(e));
        }
    };

    // Wrap in async adapters
    let async_session = Arc::new(hypr_claw_runtime::AsyncSessionStore::new(session_store));
    let async_locks = Arc::new(hypr_claw_runtime::AsyncLockManager::new(lock_manager));

    // Create tool registry
    let mut registry = hypr_claw_tools::ToolRegistryImpl::new();
    registry.register(Arc::new(hypr_claw_tools::tools::EchoTool));
    
    if let Ok(tool) = hypr_claw_tools::tools::FileReadTool::new("./sandbox") {
        registry.register(Arc::new(tool));
    } else {
        eprintln!("⚠️  Warning: FileReadTool initialization failed");
    }
    
    if let Ok(tool) = hypr_claw_tools::tools::FileWriteTool::new("./sandbox") {
        registry.register(Arc::new(tool));
    } else {
        eprintln!("⚠️  Warning: FileWriteTool initialization failed");
    }
    
    if let Ok(tool) = hypr_claw_tools::tools::FileListTool::new("./sandbox") {
        registry.register(Arc::new(tool));
    } else {
        eprintln!("⚠️  Warning: FileListTool initialization failed");
    }
    
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

    // Initialize LLM client based on provider
    let llm_client = match &config.provider {
        LLMProvider::Nvidia => {
            let api_key = match bootstrap::get_nvidia_api_key() {
                Ok(key) => key,
                Err(e) => {
                    eprintln!("❌ Failed to retrieve NVIDIA API key: {}", e);
                    eprintln!("💡 Tip: Run 'hypr-claw config reset' to reconfigure");
                    return Err(e.into());
                }
            };
            hypr_claw_runtime::LLMClient::with_api_key_and_model(
                config.provider.base_url(),
                1,
                api_key,
                config.model.clone(),
            )
        }
        LLMProvider::Local { .. } => {
            hypr_claw_runtime::LLMClient::new(config.provider.base_url(), 1)
        }
    };

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
    println!("✅ System initialized");
    println!("\n🤖 Executing task for user '{}' with agent '{}'...\n", user_id, agent_name);
    
    match controller.execute(user_id, agent_name, &task).await {
        Ok(response) => {
            println!("\n╔══════════════════════════════════════════════════════════════════╗");
            println!("║                         Response                                 ║");
            println!("╚══════════════════════════════════════════════════════════════════╝");
            println!("{}", response);
            println!("\n✅ Task completed successfully");
            Ok(())
        }
        Err(e) => {
            eprintln!("\n╔══════════════════════════════════════════════════════════════════╗");
            eprintln!("║                          Error                                   ║");
            eprintln!("╚══════════════════════════════════════════════════════════════════╝");
            
            match &e {
                hypr_claw_runtime::RuntimeError::LLMError(msg) => {
                    eprintln!("❌ LLM Error: {}", msg);
                    if msg.contains("Authentication failed") || msg.contains("401") {
                        eprintln!("\n💡 Tip: Run 'hypr-claw config reset' to reconfigure your API key");
                    } else if msg.contains("Rate limited") || msg.contains("429") {
                        eprintln!("\n💡 Tip: Wait a moment and try again");
                    } else if msg.contains("404") || msg.contains("Invalid endpoint") {
                        eprintln!("\n💡 Tip: Endpoint configuration error. Run 'hypr-claw config reset' to reconfigure");
                    } else if msg.contains("service error") || msg.contains("5") {
                        eprintln!("\n💡 Tip: The LLM service is experiencing issues. Try again later");
                    } else if msg.contains("Network connection failed") {
                        eprintln!("\n💡 Tip: Check your internet connection");
                    } else {
                        eprintln!("\n💡 Tip: Check that your LLM service is running and accessible");
                    }
                }
                hypr_claw_runtime::RuntimeError::ToolError(msg) => {
                    eprintln!("❌ Tool Error: {}", msg);
                }
                hypr_claw_runtime::RuntimeError::LockError(msg) => {
                    eprintln!("❌ Lock Error: {}", msg);
                    eprintln!("\n💡 Tip: Another session may be active. Wait 30 seconds and try again");
                }
                hypr_claw_runtime::RuntimeError::SessionError(msg) => {
                    eprintln!("❌ Session Error: {}", msg);
                    eprintln!("\n💡 Tip: Check disk space and permissions in ./data/sessions");
                }
                hypr_claw_runtime::RuntimeError::ConfigError(msg) => {
                    eprintln!("❌ Config Error: {}", msg);
                    eprintln!("\n💡 Tip: Check that ./data/agents/{}.yaml exists", agent_name);
                }
                _ => {
                    eprintln!("❌ Error: {}", e);
                }
            }
            
            Err(Box::new(e) as Box<dyn std::error::Error>)
        }
    }
}

fn handle_config_reset() -> Result<(), Box<dyn std::error::Error>> {
    println!("Resetting configuration...");
    
    Config::delete()?;
    
    if let Err(e) = bootstrap::delete_nvidia_api_key() {
        // Ignore if key doesn't exist
        if !e.to_string().contains("not found") {
            eprintln!("⚠️  Warning: Failed to delete API key: {}", e);
        }
    }
    
    println!("✅ Configuration reset. Run hypr-claw again to reconfigure.");
    Ok(())
}

fn initialize_directories() -> Result<(), Box<dyn std::error::Error>> {
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

    Ok(())
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
