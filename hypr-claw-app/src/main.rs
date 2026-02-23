use std::io::{self, Write};
use std::collections::HashSet;
use std::sync::{Arc, RwLock};
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
        LLMProvider::Google => "Google Gemini",
        LLMProvider::Local { .. } => "Local",
        LLMProvider::Antigravity => "Antigravity (Claude + Gemini)",
        LLMProvider::GeminiCli => "Gemini CLI",
        LLMProvider::Codex => "OpenAI Codex (ChatGPT Plus/Pro)",
    };
    println!("Using provider: {}", provider_name);
    println!();

    if !config.provider.supports_function_calling() {
        eprintln!(
            "❌ Provider '{}' does not support function/tool calling in agent mode.",
            provider_name
        );
        eprintln!("   Use NVIDIA, Google, or Local providers for autonomous execution.");
        return Err("Provider capability check failed".into());
    }

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
    let session_key = format!("{}:{}", user_id, agent_name);

    let context_manager = hypr_claw_memory::ContextManager::new("./data/context");
    context_manager.initialize().await?;
    let mut context = context_manager.load(&session_key).await?;
    if context.session_id.is_empty() {
        context.session_id = session_key.clone();
    }

    let mut active_soul_id = if context.active_soul_id.is_empty() {
        "safe_assistant".to_string()
    } else {
        context.active_soul_id.clone()
    };
    let mut active_soul = match load_soul_profile(&active_soul_id).await {
        Ok(soul) => soul,
        Err(e) => {
            eprintln!(
                "⚠️  Failed to load soul '{}': {}. Falling back to safe_assistant.",
                active_soul_id, e
            );
            active_soul_id = "safe_assistant".to_string();
            load_soul_profile(&active_soul_id).await?
        }
    };
    context.active_soul_id = active_soul_id.clone();
    context_manager.save(&context).await?;

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

    // Register OS capability tools
    registry.register(Arc::new(hypr_claw_tools::os_tools::FsCreateDirTool));
    registry.register(Arc::new(hypr_claw_tools::os_tools::FsDeleteTool));
    registry.register(Arc::new(hypr_claw_tools::os_tools::FsMoveTool));
    registry.register(Arc::new(hypr_claw_tools::os_tools::FsCopyTool));
    registry.register(Arc::new(hypr_claw_tools::os_tools::FsReadTool));
    registry.register(Arc::new(hypr_claw_tools::os_tools::FsWriteTool));
    registry.register(Arc::new(hypr_claw_tools::os_tools::FsListTool));
    registry.register(Arc::new(hypr_claw_tools::os_tools::HyprWorkspaceSwitchTool));
    registry.register(Arc::new(hypr_claw_tools::os_tools::HyprWorkspaceMoveWindowTool));
    registry.register(Arc::new(hypr_claw_tools::os_tools::HyprWindowFocusTool));
    registry.register(Arc::new(hypr_claw_tools::os_tools::HyprWindowCloseTool));
    registry.register(Arc::new(hypr_claw_tools::os_tools::HyprWindowMoveTool));
    registry.register(Arc::new(hypr_claw_tools::os_tools::HyprExecTool));
    registry.register(Arc::new(hypr_claw_tools::os_tools::ProcSpawnTool));
    registry.register(Arc::new(hypr_claw_tools::os_tools::ProcKillTool));
    registry.register(Arc::new(hypr_claw_tools::os_tools::ProcListTool));
    registry.register(Arc::new(hypr_claw_tools::os_tools::WallpaperSetTool));
    registry.register(Arc::new(hypr_claw_tools::os_tools::SystemShutdownTool));
    registry.register(Arc::new(hypr_claw_tools::os_tools::SystemRebootTool));
    registry.register(Arc::new(hypr_claw_tools::os_tools::SystemBatteryTool));
    registry.register(Arc::new(hypr_claw_tools::os_tools::SystemMemoryTool));

    let registry_arc = Arc::new(registry);

    // Create tool dispatcher
    let dispatcher = Arc::new(hypr_claw_tools::ToolDispatcherImpl::new(
        registry_arc.clone(),
        permission_engine as Arc<dyn hypr_claw_tools::PermissionEngine>,
        audit_logger as Arc<dyn hypr_claw_tools::AuditLogger>,
        5000,
    ));

    let agent_config_path = format!("./data/agents/{}.yaml", agent_name);
    let agent_tools = load_agent_tool_set(&agent_config_path);
    let allowed_tools = resolve_allowed_tools(&active_soul.allowed_tools, &agent_tools);
    if allowed_tools.is_empty() {
        return Err(format!("Soul '{}' has no allowed tools after filtering", active_soul_id).into());
    }
    let allowed_tools_state = Arc::new(RwLock::new(allowed_tools.clone()));

    // Create runtime adapters
    let runtime_dispatcher = Arc::new(RuntimeDispatcherAdapter::new(dispatcher));
    let runtime_registry = Arc::new(RuntimeRegistryAdapter::new(
        registry_arc,
        allowed_tools_state.clone(),
    ));

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
            hypr_claw_runtime::LLMClientType::Standard(
                hypr_claw_runtime::LLMClient::with_api_key_and_model(
                    config.provider.base_url(),
                    1,
                    api_key,
                    config.model.clone(),
                )
            )
        }
        LLMProvider::Google => {
            let api_key = match bootstrap::get_google_api_key() {
                Ok(key) => key,
                Err(e) => {
                    eprintln!("❌ Failed to retrieve Google API key: {}", e);
                    eprintln!("💡 Tip: Run 'hypr-claw config reset' to reconfigure");
                    return Err(e.into());
                }
            };
            hypr_claw_runtime::LLMClientType::Standard(
                hypr_claw_runtime::LLMClient::with_api_key_and_model(
                    config.provider.base_url(),
                    1,
                    api_key,
                    config.model.clone(),
                )
            )
        }
        LLMProvider::Local { .. } => {
            hypr_claw_runtime::LLMClientType::Standard(
                hypr_claw_runtime::LLMClient::new(config.provider.base_url(), 1)
            )
        }
        LLMProvider::Codex | LLMProvider::Antigravity | LLMProvider::GeminiCli => {
            return Err("Provider does not support agent-mode tool calling".into());
        }
    };

    // Create compactor
    let compactor = hypr_claw_runtime::Compactor::new(4000, SimpleSummarizer);

    // Create agent loop
    let agent_loop = hypr_claw_runtime::AgentLoop::new(
        async_session,
        async_locks,
        runtime_dispatcher,
        runtime_registry.clone(),
        llm_client,
        compactor,
        active_soul.max_iterations,
    );

    // Create task manager
    let task_manager = Arc::new(hypr_claw_tasks::TaskManager::with_state_file("./data/tasks/tasks.json"));
    task_manager.restore().await?;
    context.active_tasks = to_context_tasks(task_manager.list_tasks().await);
    context_manager.save(&context).await?;

    // Run REPL loop
    println!("✅ System initialized");
    println!("🤖 Agent '{}' ready for user '{}'\n", agent_name, user_id);
    println!("🧠 Active soul: {}", active_soul_id);
    let mut system_prompt = active_soul.system_prompt.clone();
    
    println!("╔══════════════════════════════════════════════════════════════════╗");
    println!("║              Hypr-Claw Agent REPL                                ║");
    println!("║  Commands: exit, status, tasks, clear, soul switch <id>, approve ║");
    println!("╚══════════════════════════════════════════════════════════════════╝");
    println!();

    // Setup graceful shutdown
    let shutdown = Arc::new(tokio::sync::Notify::new());
    let shutdown_clone = shutdown.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        shutdown_clone.notify_one();
    });

    loop {
        tokio::select! {
            _ = shutdown.notified() => {
                println!("\n\n🛑 Shutting down gracefully...");
                task_manager.cleanup_completed().await;
                context.active_tasks = to_context_tasks(task_manager.list_tasks().await);
                let _ = context_manager.save(&context).await;
                println!("✅ Context saved. Goodbye!");
                break;
            }
            result = async {
                print!("hypr> ");
                io::stdout().flush().ok();

                let mut input = String::new();
                io::stdin().read_line(&mut input).ok();
                Some(input.trim().to_string())
            } => {
                let input = match result {
                    Some(s) if !s.is_empty() => s,
                    _ => continue,
                };

                match input.as_str() {
                    "exit" | "quit" => {
                        println!("👋 Goodbye!");
                        break;
                    }
                    "help" => {
                        println!("\n📖 Available Commands:");
                        println!("  exit, quit  - Exit the agent");
                        println!("  help        - Show this help message");
                        println!("  status      - Show agent status");
                        println!("  tasks       - List background tasks");
                        println!("  clear       - Clear screen");
                        println!("  soul switch <id> - Switch active soul profile");
                        println!("  approve <task_id> - Approve pending action (reserved)");
                        println!("\n💡 Enter any natural language command to execute\n");
                        continue;
                    }
                    "status" => {
                        println!("\n📊 Agent Status:");
                        println!("  Session: {}", session_key);
                        println!("  Agent ID: {}", agent_name);
                        println!("  Active Soul: {}", active_soul_id);
                        println!("  Status: Active");
                        let task_list = task_manager.list_tasks().await;
                        println!("  Active Tasks: {}", task_list.iter().filter(|t| t.status == hypr_claw_tasks::TaskStatus::Running).count());
                        if let Some(plan) = &context.current_plan {
                            println!("  Plan: {} [{}]", plan.goal, plan.status);
                        }
                        println!();
                        continue;
                    }
                    "tasks" => {
                        println!("\n📋 Background Tasks:");
                        let task_list = task_manager.list_tasks().await;
                        if task_list.is_empty() {
                            println!("  No tasks running");
                        } else {
                            for task in &task_list {
                                let status_icon = match task.status {
                                    hypr_claw_tasks::TaskStatus::Running => "🔄",
                                    hypr_claw_tasks::TaskStatus::Completed => "✅",
                                    hypr_claw_tasks::TaskStatus::Failed => "❌",
                                    hypr_claw_tasks::TaskStatus::Cancelled => "🚫",
                                    _ => "⏸️",
                                };
                                println!("  {} {} - {} ({:.0}%)", status_icon, task.id, task.description, task.progress * 100.0);
                            }
                        }
                        println!();
                        context.active_tasks = to_context_tasks(task_list);
                        context_manager.save(&context).await?;
                        continue;
                    }
                    "clear" => {
                        print!("\x1B[2J\x1B[1;1H");
                        continue;
                    }
                    _ => {}
                }

                if let Some(soul_id) = input.strip_prefix("soul switch ").map(str::trim) {
                    match load_soul_profile(soul_id).await {
                        Ok(profile) => {
                            let resolved = resolve_allowed_tools(&profile.allowed_tools, &agent_tools);
                            if resolved.is_empty() {
                                eprintln!("❌ Soul '{}' has no tools allowed after filtering", soul_id);
                                continue;
                            }
                            runtime_registry.set_allowed_tools(resolved.clone());
                            active_soul = profile;
                            active_soul_id = soul_id.to_string();
                            system_prompt = active_soul.system_prompt.clone();
                            context.active_soul_id = active_soul_id.clone();
                            context_manager.save(&context).await?;
                            println!(
                                "✅ Soul switched to '{}' ({} tools)",
                                active_soul_id,
                                resolved.len()
                            );
                        }
                        Err(e) => {
                            eprintln!("❌ Failed to switch soul: {}", e);
                        }
                    }
                    continue;
                }

                if input.starts_with("approve ") {
                    println!("ℹ️  Approval checks are inline. System-critical tools prompt immediately.");
                    continue;
                }

                context.recent_history.push(hypr_claw_memory::types::HistoryEntry {
                    timestamp: chrono::Utc::now().timestamp(),
                    role: "user".to_string(),
                    content: input.clone(),
                    token_count: None,
                });
                context.current_plan = Some(plan_for_input(&input));
                context_manager.save(&context).await?;

                match agent_loop.run(&session_key, agent_name, &system_prompt, &input).await {
                    Ok(response) => {
                        context.recent_history.push(hypr_claw_memory::types::HistoryEntry {
                            timestamp: chrono::Utc::now().timestamp(),
                            role: "assistant".to_string(),
                            content: response.clone(),
                            token_count: None,
                        });
                        mark_plan_completed(&mut context, &response);
                        context.active_tasks = to_context_tasks(task_manager.list_tasks().await);
                        context_manager.save(&context).await?;
                        println!("\n{}\n", response);
                    }
                    Err(e) => {
                        mark_plan_failed(&mut context, &e.to_string());
                        context_manager.save(&context).await?;
                        eprintln!("❌ Error: {}\n", e);
                    }
                }
            }
        }
    }

    context.active_tasks = to_context_tasks(task_manager.list_tasks().await);
    context_manager.save(&context).await?;
    
    Ok(())
}

fn handle_config_reset() -> Result<(), Box<dyn std::error::Error>> {
    println!("Resetting configuration...");
    
    Config::delete()?;
    
    if let Err(e) = bootstrap::delete_nvidia_api_key() {
        // Ignore if key doesn't exist
        if !e.to_string().contains("not found") {
            eprintln!("⚠️  Warning: Failed to delete NVIDIA API key: {}", e);
        }
    }
    
    if let Err(e) = bootstrap::delete_google_api_key() {
        // Ignore if key doesn't exist
        if !e.to_string().contains("not found") {
            eprintln!("⚠️  Warning: Failed to delete Google API key: {}", e);
        }
    }
    
    println!("✅ Configuration reset. Run hypr-claw again to reconfigure.");
    Ok(())
}

fn initialize_directories() -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all("./data/sessions")?;
    std::fs::create_dir_all("./data/credentials")?;
    std::fs::create_dir_all("./data/agents")?;
    std::fs::create_dir_all("./data/context")?;
    std::fs::create_dir_all("./data/tasks")?;
    
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
            "id: default\nsoul: default_soul.md\ntools:\n  - echo\n  - fs.read\n  - fs.write\n  - fs.list\n  - fs.create_dir\n  - fs.move\n  - fs.copy\n  - fs.delete\n  - hypr.workspace.switch\n  - hypr.exec\n  - proc.spawn\n  - proc.list\n  - wallpaper.set\n"
        )?;
    }
    
    if !std::path::Path::new(default_agent_soul).exists() {
        std::fs::write(
            default_agent_soul,
            "You are a local OS assistant. Use structured tools for every system action."
        )?;
    }

    Ok(())
}

struct SoulProfile {
    system_prompt: String,
    allowed_tools: Vec<String>,
    max_iterations: usize,
}

async fn load_soul_profile(soul_id: &str) -> Result<SoulProfile, Box<dyn std::error::Error>> {
    let path = format!("./souls/{}.yaml", soul_id);
    let soul = hypr_claw_policy::Soul::load(path).await?;
    Ok(SoulProfile {
        system_prompt: soul.system_prompt,
        allowed_tools: soul
            .config
            .allowed_tools
            .into_iter()
            .map(|t| normalize_tool_name(&t))
            .collect(),
        max_iterations: soul.config.max_iterations,
    })
}

fn load_agent_tool_set(config_path: &str) -> HashSet<String> {
    match hypr_claw_runtime::load_agent_config(config_path) {
        Ok(config) => config
            .tools
            .into_iter()
            .map(|t| normalize_tool_name(&t))
            .collect(),
        Err(_) => HashSet::new(),
    }
}

fn resolve_allowed_tools(soul_allowed: &[String], agent_allowed: &HashSet<String>) -> HashSet<String> {
    let soul_set: HashSet<String> = soul_allowed
        .iter()
        .map(|t| normalize_tool_name(t))
        .collect();

    if agent_allowed.is_empty() {
        return soul_set;
    }
    if soul_set.is_empty() {
        return agent_allowed.clone();
    }

    soul_set
        .into_iter()
        .filter(|tool| agent_allowed.contains(tool))
        .collect()
}

fn normalize_tool_name(name: &str) -> String {
    match name {
        "file_read" | "file.read" => "fs.read".to_string(),
        "file_write" | "file.write" => "fs.write".to_string(),
        "file_list" | "file.list" => "fs.list".to_string(),
        "shell_exec" | "shell.exec" => "hypr.exec".to_string(),
        "process_list" => "proc.list".to_string(),
        "process_spawn" => "proc.spawn".to_string(),
        "system_info" => "system.memory".to_string(),
        other => other.to_string(),
    }
}

fn to_context_tasks(task_list: Vec<hypr_claw_tasks::TaskInfo>) -> Vec<hypr_claw_memory::types::TaskState> {
    task_list
        .into_iter()
        .map(|task| hypr_claw_memory::types::TaskState {
            id: task.id,
            description: task.description,
            status: format!("{:?}", task.status).to_lowercase(),
            progress: task.progress,
            created_at: task.created_at,
            updated_at: task.updated_at,
        })
        .collect()
}

fn plan_for_input(goal: &str) -> hypr_claw_memory::types::PlanState {
    hypr_claw_memory::types::PlanState {
        goal: goal.to_string(),
        steps: vec![
            hypr_claw_memory::types::PlanStepState {
                id: 0,
                description: "Analyze request".to_string(),
                status: "completed".to_string(),
                result: Some("Intent parsed".to_string()),
            },
            hypr_claw_memory::types::PlanStepState {
                id: 1,
                description: "Execute required tools".to_string(),
                status: "in_progress".to_string(),
                result: None,
            },
            hypr_claw_memory::types::PlanStepState {
                id: 2,
                description: "Report completion".to_string(),
                status: "pending".to_string(),
                result: None,
            },
        ],
        current_step: 1,
        status: "in_progress".to_string(),
        updated_at: chrono::Utc::now().timestamp(),
    }
}

fn mark_plan_completed(context: &mut hypr_claw_memory::types::ContextData, response: &str) {
    if let Some(plan) = &mut context.current_plan {
        if let Some(step) = plan.steps.get_mut(1) {
            step.status = "completed".to_string();
            step.result = Some("Tool execution completed".to_string());
        }
        if let Some(step) = plan.steps.get_mut(2) {
            step.status = "completed".to_string();
            step.result = Some(response.to_string());
        }
        plan.current_step = plan.steps.len();
        plan.status = "completed".to_string();
        plan.updated_at = chrono::Utc::now().timestamp();
    }
}

fn mark_plan_failed(context: &mut hypr_claw_memory::types::ContextData, error: &str) {
    if let Some(plan) = &mut context.current_plan {
        let step_index = plan.current_step.min(plan.steps.len().saturating_sub(1));
        if let Some(step) = plan.steps.get_mut(step_index) {
            step.status = "failed".to_string();
            step.result = Some(error.to_string());
        }
        plan.status = "failed".to_string();
        plan.updated_at = chrono::Utc::now().timestamp();
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

#[async_trait::async_trait]
impl hypr_claw_runtime::ToolDispatcher for RuntimeDispatcherAdapter {
    async fn execute(
        &self,
        tool_name: &str,
        input: &serde_json::Value,
        session_key: &str,
    ) -> Result<serde_json::Value, hypr_claw_runtime::RuntimeError> {
        let result = self
            .inner
            .dispatch(session_key.to_string(), tool_name.to_string(), input.clone())
            .await;

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
    allowed_tools: Arc<RwLock<HashSet<String>>>,
}

impl RuntimeRegistryAdapter {
    fn new(inner: Arc<hypr_claw_tools::ToolRegistryImpl>, allowed_tools: Arc<RwLock<HashSet<String>>>) -> Self {
        Self { inner, allowed_tools }
    }

    fn set_allowed_tools(&self, allowed_tools: HashSet<String>) {
        if let Ok(mut guard) = self.allowed_tools.write() {
            *guard = allowed_tools;
        }
    }
}

impl hypr_claw_runtime::ToolRegistry for RuntimeRegistryAdapter {
    fn get_active_tools(&self, _agent_id: &str) -> Vec<String> {
        let allowed = self.allowed_tools.read().ok();
        let Some(allowed) = allowed else {
            return Vec::new();
        };
        self.inner
            .list()
            .into_iter()
            .filter(|name| allowed.contains(name))
            .collect()
    }
    
    fn get_tool_schemas(&self, _agent_id: &str) -> Vec<serde_json::Value> {
        let allowed = self.allowed_tools.read().ok();
        let Some(allowed) = allowed else {
            return Vec::new();
        };
        self.inner
            .schemas()
            .into_iter()
            .filter(|schema| {
                schema
                    .get("function")
                    .and_then(|f| f.get("name"))
                    .and_then(|n| n.as_str())
                    .map(|name| allowed.contains(name))
                    .unwrap_or(false)
            })
            .collect()
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
