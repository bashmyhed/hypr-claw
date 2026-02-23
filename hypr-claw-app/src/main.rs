use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::io::{self, Write};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::process::Command;

pub mod bootstrap;
pub mod config;

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

    let agent_name = detect_agent_name();
    let user_id = detect_user_id();
    println!("Agent profile: {}", agent_name);
    println!("User identity: {}", user_id);
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
    let available_souls = discover_souls("./souls");
    let mut agent_state = load_agent_os_state(&context);
    ensure_default_thread(&mut agent_state);
    run_first_run_onboarding(&user_id, &mut context, &mut agent_state).await?;
    context.active_soul_id = active_soul_id.clone();
    persist_agent_os_state(&mut context, &agent_state);
    context_manager.save(&context).await?;

    println!("\n🔧 Initializing system...");

    // Initialize infrastructure
    let session_store = match hypr_claw::infra::session_store::SessionStore::new("./data/sessions")
    {
        Ok(store) => Arc::new(store),
        Err(e) => {
            eprintln!("❌ Failed to initialize session store: {}", e);
            return Err(Box::new(e));
        }
    };

    let lock_manager = Arc::new(hypr_claw::infra::lock_manager::LockManager::new(
        Duration::from_secs(30),
    ));
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
    registry.register(Arc::new(
        hypr_claw_tools::os_tools::HyprWorkspaceMoveWindowTool,
    ));
    registry.register(Arc::new(hypr_claw_tools::os_tools::HyprWindowFocusTool));
    registry.register(Arc::new(hypr_claw_tools::os_tools::HyprWindowCloseTool));
    registry.register(Arc::new(hypr_claw_tools::os_tools::HyprWindowMoveTool));
    registry.register(Arc::new(hypr_claw_tools::os_tools::HyprExecTool));
    registry.register(Arc::new(hypr_claw_tools::os_tools::ProcSpawnTool));
    registry.register(Arc::new(hypr_claw_tools::os_tools::ProcKillTool));
    registry.register(Arc::new(hypr_claw_tools::os_tools::ProcListTool));
    registry.register(Arc::new(hypr_claw_tools::os_tools::DesktopOpenUrlTool));
    registry.register(Arc::new(hypr_claw_tools::os_tools::DesktopLaunchAppTool));
    registry.register(Arc::new(hypr_claw_tools::os_tools::DesktopSearchWebTool));
    registry.register(Arc::new(hypr_claw_tools::os_tools::DesktopOpenGmailTool));
    registry.register(Arc::new(hypr_claw_tools::os_tools::DesktopTypeTextTool));
    registry.register(Arc::new(hypr_claw_tools::os_tools::DesktopKeyPressTool));
    registry.register(Arc::new(hypr_claw_tools::os_tools::DesktopKeyComboTool));
    registry.register(Arc::new(hypr_claw_tools::os_tools::DesktopMouseClickTool));
    registry.register(Arc::new(
        hypr_claw_tools::os_tools::DesktopCaptureScreenTool,
    ));
    registry.register(Arc::new(hypr_claw_tools::os_tools::DesktopActiveWindowTool));
    registry.register(Arc::new(hypr_claw_tools::os_tools::DesktopListWindowsTool));
    registry.register(Arc::new(hypr_claw_tools::os_tools::DesktopMouseMoveTool));
    registry.register(Arc::new(hypr_claw_tools::os_tools::DesktopClickAtTool));
    registry.register(Arc::new(hypr_claw_tools::os_tools::DesktopOcrScreenTool));
    registry.register(Arc::new(hypr_claw_tools::os_tools::DesktopFindTextTool));
    registry.register(Arc::new(hypr_claw_tools::os_tools::DesktopClickTextTool));
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
        return Err(format!(
            "Soul '{}' has no allowed tools after filtering",
            active_soul_id
        )
        .into());
    }
    let mut active_allowed_tools = allowed_tools.clone();
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
                ),
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
                ),
            )
        }
        LLMProvider::Local { .. } => hypr_claw_runtime::LLMClientType::Standard(
            hypr_claw_runtime::LLMClient::new(config.provider.base_url(), 1),
        ),
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
    let task_manager = Arc::new(hypr_claw_tasks::TaskManager::with_state_file(
        "./data/tasks/tasks.json",
    ));
    task_manager.restore().await?;
    context.active_tasks = to_context_tasks(task_manager.list_tasks().await);
    context_manager.save(&context).await?;

    // Run REPL loop
    println!("✅ System initialized");
    println!(
        "🤖 Agent '{}' ready for {} ({})\n",
        agent_name,
        display_name(&agent_state, &user_id),
        user_id
    );
    println!("🧠 Active soul: {}", active_soul_id);
    println!("🧵 Active task thread: {}", agent_state.active_thread_id);
    let mut system_prompt = active_soul.system_prompt.clone();

    println!("╔══════════════════════════════════════════════════════════════════╗");
    println!("║                Hypr-Claw Agent Console                           ║");
    println!("║  Commands: help, status, tasks, profile, scan, soul, /task ...  ║");
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
                persist_agent_os_state(&mut context, &agent_state);
                let _ = context_manager.save(&context).await;
                println!("✅ Context saved. Goodbye!");
                break;
            }
            result = async {
                let prompt = format!("hypr[{}/{}]> ", agent_state.active_thread_id, active_soul_id);
                print!("{prompt}");
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
                        println!("  exit, quit            - Exit the agent");
                        println!("  help                  - Show this help message");
                        println!("  status                - Show agent status");
                        println!("  tasks                 - List background tasks");
                        println!("  profile               - Show learned system profile");
                        println!("  scan                  - Re-scan system profile");
                        println!("  clear                 - Clear screen");
                        println!("  soul list             - List installed souls");
                        println!("  soul switch <id>      - Switch active soul profile");
                        println!("  soul auto on|off      - Enable/disable auto soul routing");
                        println!("  /task new <title>     - Create a task thread");
                        println!("  /task list            - List task threads");
                        println!("  /task switch <id>     - Switch active task thread");
                        println!("  /task close <id>      - Archive a task thread");
                        println!("  approve <task_id>     - Approval helper");
                        println!("\n💡 Enter any natural language command to execute\n");
                        continue;
                    }
                    "status" => {
                        println!("\n📊 Agent Status:");
                        println!("  Session: {}", session_key);
                        println!("  Agent ID: {}", agent_name);
                        println!("  User: {}", display_name(&agent_state, &user_id));
                        println!("  Active Soul: {}", active_soul_id);
                        println!("  Soul Auto: {}", if agent_state.soul_auto { "on" } else { "off" });
                        println!("  Active Thread: {}", agent_state.active_thread_id);
                        println!(
                            "  Threads: {} active / {} total",
                            agent_state.task_threads.iter().filter(|t| !t.archived).count(),
                            agent_state.task_threads.len()
                        );
                        let task_list = task_manager.list_tasks().await;
                        println!("  Background Tasks: {}", task_list.iter().filter(|t| t.status == hypr_claw_tasks::TaskStatus::Running).count());
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
                        persist_agent_os_state(&mut context, &agent_state);
                        context_manager.save(&context).await?;
                        continue;
                    }
                    "profile" => {
                        let profile = &agent_state.onboarding.system_profile;
                        if profile.is_null() || profile == &json!({}) {
                            println!("\nNo profile stored yet.\n");
                        } else {
                            println!(
                                "\n{}\n",
                                serde_json::to_string_pretty(profile).unwrap_or_else(|_| "{}".to_string())
                            );
                        }
                        continue;
                    }
                    "scan" => {
                        if prompt_yes_no("Run a new system scan now? [Y/n] ", true)? {
                            agent_state.onboarding.system_profile =
                                collect_system_profile(&user_id).await;
                            agent_state.onboarding.profile_confirmed = true;
                            agent_state.onboarding.last_scan_at = Some(chrono::Utc::now().timestamp());
                            persist_agent_os_state(&mut context, &agent_state);
                            context_manager.save(&context).await?;
                            println!("✅ System profile updated");
                        }
                        continue;
                    }
                    "clear" => {
                        print!("\x1B[2J\x1B[1;1H");
                        continue;
                    }
                    _ => {}
                }

                if input.starts_with("/task ") {
                    if handle_task_command(&input, &mut agent_state) {
                        persist_agent_os_state(&mut context, &agent_state);
                        context_manager.save(&context).await?;
                    }
                    continue;
                }

                if input == "soul list" {
                    println!("\n🧬 Available Souls ({}):", available_souls.len());
                    for soul_id in &available_souls {
                        let marker = if soul_id == &active_soul_id { "*" } else { " " };
                        println!("  {} {}", marker, soul_id);
                    }
                    println!();
                    continue;
                }

                if let Some(raw_mode) = input.strip_prefix("soul auto ").map(str::trim) {
                    match raw_mode {
                        "on" => {
                            agent_state.soul_auto = true;
                            persist_agent_os_state(&mut context, &agent_state);
                            context_manager.save(&context).await?;
                            println!("✅ Soul auto-routing enabled");
                        }
                        "off" => {
                            agent_state.soul_auto = false;
                            persist_agent_os_state(&mut context, &agent_state);
                            context_manager.save(&context).await?;
                            println!("✅ Soul auto-routing disabled");
                        }
                        _ => eprintln!("❌ Use: soul auto on|off"),
                    }
                    continue;
                }

                if let Some(soul_id) = input.strip_prefix("soul switch ").map(str::trim) {
                    match switch_soul(soul_id, &runtime_registry, &agent_tools).await {
                        Ok((profile, resolved_tools)) => {
                            let resolved_count = resolved_tools.len();
                            active_soul = profile;
                            active_soul_id = soul_id.to_string();
                            active_allowed_tools = resolved_tools;
                            system_prompt = active_soul.system_prompt.clone();
                            context.active_soul_id = active_soul_id.clone();
                            persist_agent_os_state(&mut context, &agent_state);
                            context_manager.save(&context).await?;
                            println!(
                                "✅ Soul switched to '{}' ({} tools)",
                                active_soul_id,
                                resolved_count
                            );
                        }
                        Err(e) => eprintln!("❌ Failed to switch soul: {}", e),
                    }
                    continue;
                }

                if input.starts_with("approve ") {
                    println!("ℹ️  Approval checks are inline. System-critical tools prompt immediately.");
                    continue;
                }

                if agent_state.soul_auto {
                    if let Some(candidate_soul_id) = auto_select_soul_id(&input, &available_souls) {
                        if candidate_soul_id != active_soul_id {
                            if let Ok((profile, resolved_tools)) =
                                switch_soul(&candidate_soul_id, &runtime_registry, &agent_tools).await
                            {
                                let resolved_count = resolved_tools.len();
                                active_soul = profile;
                                active_soul_id = candidate_soul_id;
                                active_allowed_tools = resolved_tools;
                                system_prompt = active_soul.system_prompt.clone();
                                context.active_soul_id = active_soul_id.clone();
                                println!(
                                    "🧠 Auto soul: '{}' ({} tools)",
                                    active_soul_id,
                                    resolved_count
                                );
                            }
                        }
                    }
                }

                context.recent_history.push(hypr_claw_memory::types::HistoryEntry {
                    timestamp: chrono::Utc::now().timestamp(),
                    role: "user".to_string(),
                    content: format!("[thread:{}] {}", agent_state.active_thread_id, input),
                    token_count: None,
                });
                touch_active_thread(&mut agent_state);
                context.current_plan = Some(plan_for_input(&input));
                persist_agent_os_state(&mut context, &agent_state);
                context_manager.save(&context).await?;

                let task_session_key = thread_session_key(&session_key, &agent_state.active_thread_id);
                let focused_tools = focused_tools_for_input(&input, &active_allowed_tools);
                let use_focused = !focused_tools.is_empty() && focused_tools.len() < active_allowed_tools.len();
                if use_focused {
                    runtime_registry.set_allowed_tools(focused_tools.clone());
                }

                let mut run_result = agent_loop
                    .run(&task_session_key, &agent_name, &system_prompt, &input)
                    .await;

                if use_focused {
                    runtime_registry.set_allowed_tools(active_allowed_tools.clone());
                }

                if let Err(err) = &run_result {
                    let err_msg = err.to_string();
                    let is_tool_enforcement_error = err_msg.contains("Tool invocation required but not performed");
                    let is_provider_argument_error =
                        err_msg.contains("INVALID_ARGUMENT") || err_msg.contains("400 Bad Request");

                    if is_tool_enforcement_error {
                        if let Some(candidate_soul_id) = auto_select_soul_id(&input, &available_souls) {
                            if candidate_soul_id != active_soul_id {
                                if let Ok((profile, resolved_tools)) =
                                    switch_soul(&candidate_soul_id, &runtime_registry, &agent_tools).await
                                {
                                    active_soul = profile;
                                    active_soul_id = candidate_soul_id;
                                    active_allowed_tools = resolved_tools;
                                    system_prompt = active_soul.system_prompt.clone();
                                    context.active_soul_id = active_soul_id.clone();
                                    println!("🧠 Recovery soul switch: {}", active_soul_id);
                                }
                            }
                        }
                        let retry_prompt = format!(
                            "{}\n\nExecute this now using available tools. Do not answer with explanation-only text.",
                            input
                        );
                        run_result = agent_loop
                            .run(&task_session_key, &agent_name, &system_prompt, &retry_prompt)
                            .await;
                    } else if is_provider_argument_error {
                        let emergency_tools = emergency_tool_subset(&input, &active_allowed_tools);
                        if !emergency_tools.is_empty() && emergency_tools.len() < active_allowed_tools.len() {
                            runtime_registry.set_allowed_tools(emergency_tools);
                            run_result = agent_loop
                                .run(&task_session_key, &agent_name, &system_prompt, &input)
                                .await;
                            runtime_registry.set_allowed_tools(active_allowed_tools.clone());
                        }
                    }
                }

                match run_result {
                    Ok(response) => {
                        context.recent_history.push(hypr_claw_memory::types::HistoryEntry {
                            timestamp: chrono::Utc::now().timestamp(),
                            role: "assistant".to_string(),
                            content: format!("[thread:{}] {}", agent_state.active_thread_id, response),
                            token_count: None,
                        });
                        mark_plan_completed(&mut context, &response);
                        context.active_tasks = to_context_tasks(task_manager.list_tasks().await);
                        persist_agent_os_state(&mut context, &agent_state);
                        context_manager.save(&context).await?;
                        println!("\n{}\n", response);
                    }
                    Err(e) => {
                        mark_plan_failed(&mut context, &e.to_string());
                        persist_agent_os_state(&mut context, &agent_state);
                        context_manager.save(&context).await?;
                        eprintln!("❌ Error: {}\n", e);
                    }
                }
            }
        }
    }

    context.active_tasks = to_context_tasks(task_manager.list_tasks().await);
    persist_agent_os_state(&mut context, &agent_state);
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
            "id: default\nsoul: default_soul.md\ntools:\n  - echo\n  - fs.read\n  - fs.write\n  - fs.list\n  - fs.create_dir\n  - fs.move\n  - fs.copy\n  - fs.delete\n  - hypr.workspace.switch\n  - hypr.workspace.move_window\n  - hypr.window.focus\n  - hypr.window.close\n  - hypr.window.move\n  - hypr.exec\n  - proc.spawn\n  - proc.kill\n  - proc.list\n  - desktop.open_url\n  - desktop.launch_app\n  - desktop.search_web\n  - desktop.open_gmail\n  - desktop.type_text\n  - desktop.key_press\n  - desktop.key_combo\n  - desktop.mouse_click\n  - desktop.capture_screen\n  - desktop.active_window\n  - desktop.list_windows\n  - desktop.mouse_move\n  - desktop.click_at\n  - desktop.ocr_screen\n  - desktop.find_text\n  - desktop.click_text\n  - wallpaper.set\n  - system.memory\n  - system.battery\n"
        )?;
    }

    if !std::path::Path::new(default_agent_soul).exists() {
        std::fs::write(
            default_agent_soul,
            "You are a local OS assistant. Use structured tools for every system action.",
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

fn resolve_allowed_tools(
    soul_allowed: &[String],
    agent_allowed: &HashSet<String>,
) -> HashSet<String> {
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
        "browser.open_url" => "desktop.open_url".to_string(),
        "browser.search" => "desktop.search_web".to_string(),
        "app.launch" => "desktop.launch_app".to_string(),
        "gmail.open" => "desktop.open_gmail".to_string(),
        other => other.to_string(),
    }
}

fn to_context_tasks(
    task_list: Vec<hypr_claw_tasks::TaskInfo>,
) -> Vec<hypr_claw_memory::types::TaskState> {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AgentOsState {
    #[serde(default)]
    onboarding: OnboardingState,
    #[serde(default)]
    soul_auto: bool,
    #[serde(default)]
    task_threads: Vec<TaskThread>,
    #[serde(default)]
    active_thread_id: String,
}

impl Default for AgentOsState {
    fn default() -> Self {
        Self {
            onboarding: OnboardingState::default(),
            soul_auto: true,
            task_threads: vec![TaskThread::new("task-1".to_string(), "Main".to_string())],
            active_thread_id: "task-1".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OnboardingState {
    #[serde(default)]
    completed: bool,
    #[serde(default)]
    preferred_name: String,
    #[serde(default)]
    profile_confirmed: bool,
    #[serde(default = "empty_json")]
    system_profile: Value,
    #[serde(default)]
    last_scan_at: Option<i64>,
}

impl Default for OnboardingState {
    fn default() -> Self {
        Self {
            completed: false,
            preferred_name: String::new(),
            profile_confirmed: false,
            system_profile: json!({}),
            last_scan_at: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TaskThread {
    id: String,
    title: String,
    archived: bool,
    created_at: i64,
    updated_at: i64,
}

impl TaskThread {
    fn new(id: String, title: String) -> Self {
        let now = chrono::Utc::now().timestamp();
        Self {
            id,
            title,
            archived: false,
            created_at: now,
            updated_at: now,
        }
    }
}

fn empty_json() -> Value {
    json!({})
}

fn load_agent_os_state(context: &hypr_claw_memory::types::ContextData) -> AgentOsState {
    let maybe_state = context.system_state.get("agent_os_state").cloned();
    let mut state = maybe_state
        .as_ref()
        .and_then(|v| serde_json::from_value::<AgentOsState>(v.clone()).ok())
        .unwrap_or_default();

    // Migration: if legacy state did not include soul_auto, default to enabled.
    let soul_auto_present = maybe_state
        .as_ref()
        .and_then(|v| v.get("soul_auto"))
        .is_some();
    if !soul_auto_present {
        state.soul_auto = true;
    }

    state
}

fn persist_agent_os_state(
    context: &mut hypr_claw_memory::types::ContextData,
    state: &AgentOsState,
) {
    if !context.system_state.is_object() {
        context.system_state = json!({});
    }
    if let Some(obj) = context.system_state.as_object_mut() {
        let value = serde_json::to_value(state).unwrap_or_else(|_| json!({}));
        obj.insert("agent_os_state".to_string(), value);
    }
}

fn ensure_default_thread(state: &mut AgentOsState) {
    if state.task_threads.is_empty() {
        state
            .task_threads
            .push(TaskThread::new("task-1".to_string(), "Main".to_string()));
    }
    let active_exists = state
        .task_threads
        .iter()
        .any(|thread| thread.id == state.active_thread_id && !thread.archived);
    if !active_exists {
        if let Some(thread) = state.task_threads.iter().find(|thread| !thread.archived) {
            state.active_thread_id = thread.id.clone();
        } else {
            let fallback = TaskThread::new(next_thread_id(state), "Main".to_string());
            state.active_thread_id = fallback.id.clone();
            state.task_threads.push(fallback);
        }
    }
}

fn display_name(state: &AgentOsState, fallback_user_id: &str) -> String {
    if state.onboarding.preferred_name.trim().is_empty() {
        fallback_user_id.to_string()
    } else {
        state.onboarding.preferred_name.clone()
    }
}

fn detect_agent_name() -> String {
    if let Ok(from_env) = std::env::var("HYPR_CLAW_AGENT") {
        let candidate = from_env.trim();
        if !candidate.is_empty() {
            let path = format!("./data/agents/{}.yaml", candidate);
            if std::path::Path::new(&path).exists() {
                return candidate.to_string();
            }
        }
    }

    let default_path = "./data/agents/default.yaml";
    if std::path::Path::new(default_path).exists() {
        return "default".to_string();
    }

    if let Ok(entries) = std::fs::read_dir("./data/agents") {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("yaml") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    return stem.to_string();
                }
            }
        }
    }

    "default".to_string()
}

fn detect_user_id() -> String {
    std::env::var("HYPR_CLAW_USER")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| std::env::var("USER").ok().filter(|s| !s.trim().is_empty()))
        .unwrap_or_else(|| "local_user".to_string())
}

fn prompt_line(prompt: &str) -> io::Result<String> {
    print!("{prompt}");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_string())
}

fn prompt_yes_no(prompt: &str, default_yes: bool) -> io::Result<bool> {
    let input = prompt_line(prompt)?;
    if input.is_empty() {
        return Ok(default_yes);
    }
    match input.to_lowercase().as_str() {
        "y" | "yes" => Ok(true),
        "n" | "no" => Ok(false),
        _ => Ok(default_yes),
    }
}

async fn run_first_run_onboarding(
    user_id: &str,
    context: &mut hypr_claw_memory::types::ContextData,
    state: &mut AgentOsState,
) -> Result<(), Box<dyn std::error::Error>> {
    if state.onboarding.completed {
        return Ok(());
    }

    println!("\n🧭 First Run Onboarding");
    if state.onboarding.preferred_name.trim().is_empty() {
        let preferred = prompt_line("What should I call you? ")?;
        state.onboarding.preferred_name = if preferred.trim().is_empty() {
            user_id.to_string()
        } else {
            preferred
        };
    }

    if prompt_yes_no("Allow first-time system study scan? [Y/n] ", true)? {
        state.onboarding.system_profile = collect_system_profile(user_id).await;
        state.onboarding.last_scan_at = Some(chrono::Utc::now().timestamp());
        print_system_profile_summary(&state.onboarding.system_profile);

        loop {
            if prompt_yes_no("Is this system profile correct? [Y/n] ", true)? {
                state.onboarding.profile_confirmed = true;
                break;
            }
            if !prompt_yes_no("Edit profile now? [Y/n] ", true)? {
                break;
            }
            edit_profile_interactively(&mut state.onboarding.system_profile)?;
            print_system_profile_summary(&state.onboarding.system_profile);
        }
    } else {
        println!("ℹ️  System study skipped for now. Use `scan` later.");
    }

    state.onboarding.completed = true;
    if !context
        .facts
        .iter()
        .any(|f| f.starts_with("preferred_name:"))
    {
        context.facts.push(format!(
            "preferred_name:{}",
            state.onboarding.preferred_name
        ));
    }
    Ok(())
}

fn print_system_profile_summary(profile: &Value) {
    let distro = profile
        .pointer("/platform/distro_name")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let kernel = profile
        .pointer("/platform/kernel")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let hypr = profile
        .pointer("/desktop/hyprland_available")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let active_ws = profile
        .pointer("/desktop/active_workspace")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    println!("\n🖥️  Profile Summary");
    println!("  Distro: {}", distro);
    println!("  Kernel: {}", kernel);
    println!("  Hyprland available: {}", if hypr { "yes" } else { "no" });
    if hypr {
        println!("  Active workspace: {}", active_ws);
    }
    println!();
}

fn edit_profile_interactively(profile: &mut Value) -> io::Result<()> {
    println!("Profile edit mode.");
    println!("Commands: show | set <path> <value> | done | accept | help");
    println!("Path example: desktop.active_workspace");
    loop {
        let line = prompt_line("profile> ")?;
        if line.is_empty() {
            continue;
        }
        if line == "done" || line == "accept" {
            break;
        }
        if line == "help" {
            println!("Commands: show | set <path> <value> | done | accept");
            continue;
        }
        if line == "show" {
            println!(
                "{}",
                serde_json::to_string_pretty(profile).unwrap_or_else(|_| "{}".to_string())
            );
            continue;
        }
        if let Some(rest) = line.strip_prefix("set ") {
            let mut parts = rest.splitn(2, ' ');
            let path = parts.next().unwrap_or_default();
            let raw_value = parts.next().unwrap_or_default().trim();
            if path.is_empty() || raw_value.is_empty() {
                println!("Usage: set <path> <value>");
                continue;
            }
            let parsed = serde_json::from_str::<Value>(raw_value)
                .unwrap_or_else(|_| Value::String(raw_value.to_string()));
            set_json_path(profile, path, parsed);
            println!("Updated {}", path);
            continue;
        }
        println!("Unknown command. Use show | set <path> <value> | done");
    }
    Ok(())
}

fn set_json_path(root: &mut Value, path: &str, value: Value) {
    let segments: Vec<&str> = path.split('.').filter(|s| !s.is_empty()).collect();
    if segments.is_empty() {
        *root = value;
        return;
    }

    let mut current = root;
    let mut pending = Some(value);
    for (index, segment) in segments.iter().enumerate() {
        let is_last = index == segments.len() - 1;
        if !current.is_object() {
            *current = json!({});
        }
        let map = current.as_object_mut().expect("object was just created");
        if is_last {
            map.insert(
                (*segment).to_string(),
                pending.take().unwrap_or(Value::Null),
            );
            return;
        }
        current = map
            .entry((*segment).to_string())
            .or_insert_with(|| json!({}));
    }
}

async fn collect_system_profile(user_id: &str) -> Value {
    let os_release = tokio::fs::read_to_string("/etc/os-release")
        .await
        .unwrap_or_default();
    let distro_name = parse_os_release_value(&os_release, "PRETTY_NAME")
        .or_else(|| parse_os_release_value(&os_release, "NAME"))
        .unwrap_or_else(|| "Unknown Linux".to_string());
    let distro_id =
        parse_os_release_value(&os_release, "ID").unwrap_or_else(|| "unknown".to_string());
    let distro_version = parse_os_release_value(&os_release, "VERSION_ID").unwrap_or_default();
    let kernel = command_output("uname", &["-r"])
        .await
        .unwrap_or_else(|| "unknown".to_string());
    let hostname = command_output("hostname", &[])
        .await
        .unwrap_or_else(|| "unknown".to_string());

    let hyprland_available = command_exists("hyprctl").await;
    let active_workspace = if hyprland_available {
        command_output("hyprctl", &["activeworkspace", "-j"])
            .await
            .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
            .and_then(|json| json.get("id").and_then(|v| v.as_u64()))
            .unwrap_or(0)
    } else {
        0
    };
    let workspace_count = if hyprland_available {
        command_output("hyprctl", &["workspaces", "-j"])
            .await
            .and_then(|raw| serde_json::from_str::<Vec<Value>>(&raw).ok())
            .map(|arr| arr.len())
            .unwrap_or(0)
    } else {
        0
    };

    let known_commands = [
        "code",
        "codex",
        "firefox",
        "chromium",
        "google-chrome-stable",
        "kitty",
        "wezterm",
        "thunderbird",
        "discord",
        "slack",
        "pacman",
        "yay",
        "paru",
    ];
    let mut commands = serde_json::Map::new();
    for command in known_commands {
        commands.insert(
            command.to_string(),
            Value::Bool(command_exists(command).await),
        );
    }

    json!({
        "scanned_at": chrono::Utc::now().timestamp(),
        "platform": {
            "distro_name": distro_name,
            "distro_id": distro_id,
            "distro_version": distro_version,
            "kernel": kernel,
            "arch": std::env::consts::ARCH
        },
        "user": {
            "id": user_id,
            "name": std::env::var("USER").unwrap_or_else(|_| user_id.to_string()),
            "home": std::env::var("HOME").unwrap_or_default(),
            "shell": std::env::var("SHELL").unwrap_or_default(),
            "hostname": hostname
        },
        "desktop": {
            "session": std::env::var("XDG_SESSION_TYPE").unwrap_or_default(),
            "desktop_env": std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default(),
            "hyprland_available": hyprland_available,
            "active_workspace": active_workspace,
            "workspace_count": workspace_count
        },
        "commands": commands
    })
}

fn parse_os_release_value(content: &str, key: &str) -> Option<String> {
    content
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{key}=")).map(str::to_string))
        .map(|value| value.trim_matches('"').to_string())
}

async fn command_exists(command: &str) -> bool {
    Command::new("which")
        .arg(command)
        .output()
        .await
        .map(|output| output.status.success())
        .unwrap_or(false)
}

async fn command_output(command: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(command).args(args).output().await.ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn discover_souls(souls_dir: &str) -> Vec<String> {
    let mut souls = Vec::new();
    if let Ok(entries) = std::fs::read_dir(souls_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("yaml") {
                continue;
            }
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                souls.push(stem.to_string());
            }
        }
    }
    souls.sort();
    souls
}

fn next_thread_id(state: &AgentOsState) -> String {
    let max_id = state
        .task_threads
        .iter()
        .filter_map(|thread| thread.id.strip_prefix("task-"))
        .filter_map(|suffix| suffix.parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    format!("task-{}", max_id + 1)
}

fn handle_task_command(input: &str, state: &mut AgentOsState) -> bool {
    let rest = input.strip_prefix("/task ").unwrap_or("").trim();
    if rest.is_empty() {
        println!("Usage: /task new <title> | /task list | /task switch <id> | /task close <id>");
        return false;
    }

    if rest == "list" {
        println!("\n🧵 Task Threads:");
        for thread in &state.task_threads {
            let marker = if thread.id == state.active_thread_id {
                "*"
            } else {
                " "
            };
            let status = if thread.archived {
                "archived"
            } else {
                "active"
            };
            println!("  {} {} - {} [{}]", marker, thread.id, thread.title, status);
        }
        println!();
        return false;
    }

    if let Some(title) = rest.strip_prefix("new ").map(str::trim) {
        let title = if title.is_empty() {
            "Untitled Task"
        } else {
            title
        };
        let id = next_thread_id(state);
        state
            .task_threads
            .push(TaskThread::new(id.clone(), title.to_string()));
        state.active_thread_id = id.clone();
        println!("✅ Created thread '{}' ({})", title, id);
        return true;
    }

    if let Some(id) = rest.strip_prefix("switch ").map(str::trim) {
        let found = state
            .task_threads
            .iter()
            .find(|thread| thread.id == id && !thread.archived)
            .map(|thread| thread.id.clone());
        if let Some(thread_id) = found {
            state.active_thread_id = thread_id.clone();
            touch_active_thread(state);
            println!("✅ Switched to thread {}", thread_id);
            return true;
        }
        println!("❌ Thread '{}' not found or archived", id);
        return false;
    }

    if let Some(id) = rest.strip_prefix("close ").map(str::trim) {
        let mut changed = false;
        for thread in &mut state.task_threads {
            if thread.id == id {
                thread.archived = true;
                thread.updated_at = chrono::Utc::now().timestamp();
                changed = true;
                break;
            }
        }
        if !changed {
            println!("❌ Thread '{}' not found", id);
            return false;
        }
        if state.active_thread_id == id {
            if let Some(next_active) = state.task_threads.iter().find(|t| !t.archived) {
                state.active_thread_id = next_active.id.clone();
            } else {
                let fallback = TaskThread::new(next_thread_id(state), "Main".to_string());
                state.active_thread_id = fallback.id.clone();
                state.task_threads.push(fallback);
            }
        }
        println!("✅ Archived thread {}", id);
        return true;
    }

    println!("Usage: /task new <title> | /task list | /task switch <id> | /task close <id>");
    false
}

fn touch_active_thread(state: &mut AgentOsState) {
    for thread in &mut state.task_threads {
        if thread.id == state.active_thread_id {
            thread.updated_at = chrono::Utc::now().timestamp();
            return;
        }
    }
}

fn thread_session_key(base_session_key: &str, thread_id: &str) -> String {
    format!("{base_session_key}::thread::{thread_id}")
}

async fn switch_soul(
    soul_id: &str,
    runtime_registry: &Arc<RuntimeRegistryAdapter>,
    agent_tools: &HashSet<String>,
) -> Result<(SoulProfile, HashSet<String>), String> {
    let profile = load_soul_profile(soul_id)
        .await
        .map_err(|e| e.to_string())?;
    let resolved = resolve_allowed_tools(&profile.allowed_tools, agent_tools);
    if resolved.is_empty() {
        return Err(format!(
            "Soul '{}' has no tools allowed after filtering",
            soul_id
        ));
    }
    runtime_registry.set_allowed_tools(resolved.clone());
    Ok((profile, resolved))
}

fn focused_tools_for_input(input: &str, allowed: &HashSet<String>) -> HashSet<String> {
    let lower = input.to_lowercase();
    let mut preferred = HashSet::new();

    let add = |set: &mut HashSet<String>, name: &str, allowed: &HashSet<String>| {
        if allowed.contains(name) {
            set.insert(name.to_string());
        }
    };

    if lower.contains("wallpaper") || lower.contains("workspace") || lower.contains("window") {
        add(&mut preferred, "wallpaper.set", allowed);
        add(&mut preferred, "hypr.workspace.switch", allowed);
        add(&mut preferred, "hypr.workspace.move_window", allowed);
        add(&mut preferred, "hypr.window.focus", allowed);
        add(&mut preferred, "hypr.window.close", allowed);
        add(&mut preferred, "hypr.window.move", allowed);
        add(&mut preferred, "hypr.exec", allowed);
    }

    if lower.contains("file")
        || lower.contains("folder")
        || lower.contains("dir")
        || lower.contains("read")
        || lower.contains("write")
        || lower.contains("create")
        || lower.contains("delete")
        || lower.contains("copy")
        || lower.contains("move")
    {
        add(&mut preferred, "fs.read", allowed);
        add(&mut preferred, "fs.write", allowed);
        add(&mut preferred, "fs.list", allowed);
        add(&mut preferred, "fs.create_dir", allowed);
        add(&mut preferred, "fs.delete", allowed);
        add(&mut preferred, "fs.copy", allowed);
        add(&mut preferred, "fs.move", allowed);
    }

    if lower.contains("open")
        || lower.contains("browser")
        || lower.contains("search")
        || lower.contains("web")
        || lower.contains("gmail")
        || lower.contains("mail")
    {
        add(&mut preferred, "desktop.open_url", allowed);
        add(&mut preferred, "desktop.search_web", allowed);
        add(&mut preferred, "desktop.launch_app", allowed);
        add(&mut preferred, "desktop.open_gmail", allowed);
    }

    if lower.contains("type")
        || lower.contains("press")
        || lower.contains("shortcut")
        || lower.contains("hotkey")
        || lower.contains("click")
        || lower.contains("mouse")
        || lower.contains("screenshot")
        || lower.contains("screen")
    {
        add(&mut preferred, "desktop.type_text", allowed);
        add(&mut preferred, "desktop.key_press", allowed);
        add(&mut preferred, "desktop.key_combo", allowed);
        add(&mut preferred, "desktop.mouse_click", allowed);
        add(&mut preferred, "desktop.capture_screen", allowed);
        add(&mut preferred, "desktop.active_window", allowed);
        add(&mut preferred, "desktop.list_windows", allowed);
        add(&mut preferred, "desktop.mouse_move", allowed);
        add(&mut preferred, "desktop.click_at", allowed);
        add(&mut preferred, "desktop.ocr_screen", allowed);
        add(&mut preferred, "desktop.find_text", allowed);
        add(&mut preferred, "desktop.click_text", allowed);
    }

    if lower.contains("run")
        || lower.contains("build")
        || lower.contains("start")
        || lower.contains("execute")
        || lower.contains("kill")
        || lower.contains("process")
    {
        add(&mut preferred, "proc.spawn", allowed);
        add(&mut preferred, "proc.kill", allowed);
        add(&mut preferred, "proc.list", allowed);
        add(&mut preferred, "hypr.exec", allowed);
    }

    if lower.contains("battery") || lower.contains("memory") || lower.contains("system") {
        add(&mut preferred, "system.battery", allowed);
        add(&mut preferred, "system.memory", allowed);
    }

    if preferred.is_empty() {
        return allowed.clone();
    }

    add(&mut preferred, "echo", allowed);
    preferred
}

fn emergency_tool_subset(input: &str, allowed: &HashSet<String>) -> HashSet<String> {
    let focused = focused_tools_for_input(input, allowed);
    if focused.is_empty() {
        return HashSet::new();
    }
    if focused.len() <= 10 {
        return focused;
    }
    let mut names: Vec<String> = focused.into_iter().collect();
    names.sort();
    names.into_iter().take(10).collect()
}

fn auto_select_soul_id(input: &str, available_souls: &[String]) -> Option<String> {
    let lower = input.to_lowercase();
    let candidates = [
        (
            "hyprland_operator",
            [
                "workspace",
                "hypr",
                "window",
                "wallpaper",
                "click",
                "type",
                "press",
                "shortcut",
                "screenshot",
                "screen",
                "ocr",
                "find text",
            ]
            .as_slice(),
        ),
        (
            "coder_engineer",
            [
                "code", "compile", "build", "refactor", "debug", "python", "rust", "script",
                "file", "folder", "project",
            ]
            .as_slice(),
        ),
        (
            "qa_tester",
            ["test", "bug", "failure", "regression"].as_slice(),
        ),
        (
            "devops_operator",
            ["docker", "deploy", "k8s", "server", "infra"].as_slice(),
        ),
        (
            "browser_researcher",
            ["search", "browser", "web", "lookup"].as_slice(),
        ),
        (
            "mail_assistant",
            ["mail", "gmail", "email", "message"].as_slice(),
        ),
        (
            "project_manager",
            ["plan", "roadmap", "task list", "milestone"].as_slice(),
        ),
        (
            "cleanup_agent",
            ["clean", "organize", "tidy", "delete old"].as_slice(),
        ),
    ];

    for (soul, tokens) in candidates {
        if tokens.iter().any(|token| lower.contains(token))
            && available_souls.iter().any(|candidate| candidate == soul)
        {
            return Some(soul.to_string());
        }
    }

    if available_souls.iter().any(|s| s == "safe_assistant") {
        return Some("safe_assistant".to_string());
    }
    available_souls.first().cloned()
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
            .dispatch(
                session_key.to_string(),
                tool_name.to_string(),
                input.clone(),
            )
            .await;

        match result {
            Ok(tool_result) => {
                if tool_result.success {
                    Ok(tool_result.output.unwrap_or(serde_json::json!({})))
                } else {
                    Err(hypr_claw_runtime::RuntimeError::ToolError(
                        tool_result
                            .error
                            .unwrap_or_else(|| "Unknown error".to_string()),
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
    fn new(
        inner: Arc<hypr_claw_tools::ToolRegistryImpl>,
        allowed_tools: Arc<RwLock<HashSet<String>>>,
    ) -> Self {
        Self {
            inner,
            allowed_tools,
        }
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
