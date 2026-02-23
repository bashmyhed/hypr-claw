use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::io::{self, Write};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;
use tokio::process::Command;

pub mod bootstrap;
pub mod config;
pub mod tui;

use config::{Config, LLMProvider};

enum UiInputEvent {
    Line(String),
    RunQueued(SupervisedTask),
    CloseTui,
    ExitApp,
    RefreshTui,
    Skip,
}

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
    let mut config = if Config::exists() {
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

    if matches!(config.provider, LLMProvider::Nvidia)
        && (config.model.trim().is_empty() || config.model == "moonshotai/kimi-k2.5")
    {
        config.model = "z-ai/glm4.7".to_string();
        let _ = config.save();
        println!("ℹ️  NVIDIA default model set to {}", config.model);
    }

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
    println!("Model: {}", config.model);
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
    if profile_needs_capability_refresh(&agent_state.onboarding.system_profile) {
        agent_state.onboarding.system_profile = collect_system_profile(&user_id).await;
        agent_state.onboarding.last_scan_at = Some(chrono::Utc::now().timestamp());
    }
    if let Err(e) = set_full_auto_mode(agent_state.onboarding.trusted_full_auto) {
        eprintln!("⚠️  Failed to persist full-auto mode flag: {}", e);
    }
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
    let action_feed: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    // Create runtime adapters
    let runtime_dispatcher = Arc::new(RuntimeDispatcherAdapter::new(
        dispatcher,
        action_feed.clone(),
    ));
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
    let mut system_prompt = active_soul.system_prompt.clone();

    print_console_bootstrap(
        provider_name,
        &config.model,
        &agent_name,
        &display_name(&agent_state, &user_id),
        &user_id,
        &session_key,
        &active_soul_id,
        &agent_state.active_thread_id,
    );

    // Setup interrupt signal (Ctrl+C interrupts current request, does not exit process)
    let interrupt = Arc::new(tokio::sync::Notify::new());
    let interrupt_clone = interrupt.clone();
    tokio::spawn(async move {
        loop {
            tokio::signal::ctrl_c().await.ok();
            interrupt_clone.notify_waiters();
        }
    });
    let mut tui_mode = false;
    let mut auto_queued_task: Option<SupervisedTask> = None;

    loop {
        if auto_queued_task.is_none() && agent_state.supervisor.auto_run {
            auto_queued_task = start_next_queued_supervised_task(&mut agent_state);
            if let Some(task) = &auto_queued_task {
                persist_agent_os_state(&mut context, &agent_state);
                context_manager.save(&context).await?;
                println!(
                    "▶ Auto-running queued task {} ({})",
                    task.id,
                    task.class.as_str()
                );
            }
        }

        tokio::select! {
            _ = interrupt.notified() => {
                println!("\n^C received. No active request to interrupt. Use 'exit' to quit.");
                continue;
            }
            result = async {
                if let Some(task) = auto_queued_task.take() {
                    return UiInputEvent::RunQueued(task);
                }
                if tui_mode {
                    let task_list = task_manager.list_tasks().await;
                    let snapshot = build_tui_snapshot(
                        provider_name,
                        &config.model,
                        &session_key,
                        &user_id,
                        &active_soul_id,
                        &agent_state,
                        &context,
                        &task_list,
                        available_souls.len(),
                        &action_feed,
                    );
                    match tui::run_command_center(&snapshot) {
                        Ok(tui::TuiOutcome::Submit(cmd)) => UiInputEvent::Line(cmd),
                        Ok(tui::TuiOutcome::Close) => UiInputEvent::CloseTui,
                        Ok(tui::TuiOutcome::ExitApp) => UiInputEvent::ExitApp,
                        Ok(tui::TuiOutcome::Refresh) => UiInputEvent::RefreshTui,
                        Err(e) => {
                            eprintln!("❌ TUI error: {}", e);
                            UiInputEvent::CloseTui
                        }
                    }
                } else {
                    let running_tasks = task_manager
                        .list_tasks()
                        .await
                        .into_iter()
                        .filter(|t| t.status == hypr_claw_tasks::TaskStatus::Running)
                        .count();
                    let prompt = format!(
                        "hypr[{}|{}|{}|run:{}]> ",
                        agent_state.active_thread_id,
                        active_soul_id,
                        short_model_name(&config.model),
                        running_tasks
                    );
                    print!("{}", ui_accent(&prompt));
                    io::stdout().flush().ok();

                    let mut input = String::new();
                    io::stdin().read_line(&mut input).ok();
                    let line = input.trim().to_string();
                    if line.is_empty() {
                        UiInputEvent::Skip
                    } else {
                        UiInputEvent::Line(line)
                    }
                }
            } => {
                let mut queued_execution: Option<SupervisedTask> = None;
                let (input, input_from_queue) = match result {
                    UiInputEvent::Line(s) => (sanitize_user_input_line(&s), false),
                    UiInputEvent::RunQueued(task) => {
                        queued_execution = Some(task.clone());
                        (task.prompt.clone(), true)
                    }
                    UiInputEvent::CloseTui => {
                        tui_mode = false;
                        continue;
                    }
                    UiInputEvent::ExitApp => {
                        println!("👋 Goodbye!");
                        break;
                    }
                    UiInputEvent::RefreshTui => continue,
                    UiInputEvent::Skip => continue,
                };

                if input.as_bytes().first() == Some(&0x1B) {
                    println!("⚠ Ignored terminal escape input. Type a command or task request.");
                    continue;
                }

                if !tui_mode && matches!(input.as_str(), ":q" | ":r" | ":refresh" | "/q" | "/r") {
                    println!("ℹ️  Use '/tui' first. Inside TUI, ':q' closes and ':r' refreshes.\n");
                    continue;
                }

                if !input_from_queue {
                    match input.as_str() {
                    "exit" | "quit" | "/exit" => {
                        println!("👋 Goodbye!");
                        break;
                    }
                    "help" | "/help" => {
                        print_help();
                        continue;
                    }
                    "tui" | "/tui" => {
                        tui_mode = true;
                        continue;
                    }
                    "repl" | "/repl" => {
                        tui_mode = false;
                        continue;
                    }
                    "status" | "/status" => {
                        let task_list = task_manager.list_tasks().await;
                        print_status_panel(
                            &session_key,
                            &agent_name,
                            &display_name(&agent_state, &user_id),
                            &active_soul_id,
                            &config.model,
                            agent_state.soul_auto,
                            &agent_state.active_thread_id,
                            &agent_state,
                            &context,
                            &task_list,
                        );
                        println!();
                        continue;
                    }
                    "dashboard" | "dash" | "/dashboard" => {
                        let task_list = task_manager.list_tasks().await;
                        print_runtime_dashboard(
                            provider_name,
                            &config.model,
                            &session_key,
                            &agent_name,
                            &user_id,
                            &active_soul_id,
                            &agent_state,
                            &context,
                            &task_list,
                            available_souls.len(),
                        );
                        println!();
                        continue;
                    }
                    "tasks" | "/tasks" => {
                        let task_list = task_manager.list_tasks().await;
                        print_tasks_panel(&task_list);
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
                            let deep_scan = prompt_yes_no(
                                "Deep scan (home/.config/packages/hyprland)? [Y/n] ",
                                true,
                            )?;
                            println!(
                                "🔎 Running {} scan...",
                                if deep_scan { "deep" } else { "standard" }
                            );
                            agent_state.onboarding.system_profile = if deep_scan {
                                collect_deep_system_profile(&user_id).await
                            } else {
                                collect_system_profile(&user_id).await
                            };
                            agent_state.onboarding.deep_scan_completed = deep_scan;
                            agent_state.onboarding.profile_confirmed = true;
                            agent_state.onboarding.last_scan_at = Some(chrono::Utc::now().timestamp());
                            persist_agent_os_state(&mut context, &agent_state);
                            context_manager.save(&context).await?;
                            println!("✅ System profile updated");
                        }
                        continue;
                    }
                    "clear" | "/clear" => {
                        print!("\x1B[2J\x1B[1;1H");
                        continue;
                    }
                    "interrupt" | "/interrupt" => {
                        println!("ℹ️  Press Ctrl+C while a request is running to interrupt it.");
                        continue;
                    }
                    "trust" | "/trust" => {
                        println!(
                            "Trust mode: {}",
                            if agent_state.onboarding.trusted_full_auto {
                                "full-auto"
                            } else {
                                "standard"
                            }
                        );
                        println!("Use: trust on | trust off");
                        continue;
                    }
                    "queue" | "/queue" => {
                        print_supervisor_queue(&agent_state);
                        continue;
                    }
                    _ => {}
                }
                }

                if let Some(mode) = input.strip_prefix("trust ").or_else(|| input.strip_prefix("/trust ")).map(str::trim) {
                    match mode {
                        "on" => {
                            agent_state.onboarding.trusted_full_auto = true;
                            let _ = set_full_auto_mode(true);
                            persist_agent_os_state(&mut context, &agent_state);
                            context_manager.save(&context).await?;
                            println!("✅ Trust mode enabled (full-auto)");
                        }
                        "off" => {
                            agent_state.onboarding.trusted_full_auto = false;
                            let _ = set_full_auto_mode(false);
                            persist_agent_os_state(&mut context, &agent_state);
                            context_manager.save(&context).await?;
                            println!("✅ Trust mode disabled (standard)");
                        }
                        _ => {
                            println!("Use: trust on | trust off");
                        }
                    }
                    continue;
                }

                if !input_from_queue {
                if let Some(prompt) = input
                    .strip_prefix("queue add ")
                    .or_else(|| input.strip_prefix("/queue add "))
                    .map(str::trim)
                {
                    if prompt.is_empty() {
                        println!("Usage: queue add <task prompt>");
                        continue;
                    }
                    let class = classify_supervised_task_class(prompt);
                    let task_id = enqueue_supervised_task(
                        &mut agent_state,
                        prompt.to_string(),
                        class.clone(),
                    );
                    persist_agent_os_state(&mut context, &agent_state);
                    context_manager.save(&context).await?;
                    println!(
                        "🗂 Queued {} ({}) as {}",
                        task_id,
                        truncate_for_table(prompt, 48),
                        class.as_str()
                    );
                    continue;
                }
                }

                if !input_from_queue && (input == "queue clear" || input == "/queue clear") {
                    let cleared = cancel_queued_supervised_tasks(&mut agent_state);
                    persist_agent_os_state(&mut context, &agent_state);
                    context_manager.save(&context).await?;
                    println!("🧹 Cleared {} queued supervisor tasks", cleared);
                    continue;
                }

                if !input_from_queue && (input == "queue run" || input == "/queue run") {
                    queued_execution = start_next_queued_supervised_task(&mut agent_state);
                    if queued_execution.is_none() {
                        println!("ℹ️  No queued tasks.");
                        continue;
                    }
                    persist_agent_os_state(&mut context, &agent_state);
                    context_manager.save(&context).await?;
                }

                if !input_from_queue {
                    if let Some(mode) = input
                        .strip_prefix("queue auto ")
                        .or_else(|| input.strip_prefix("/queue auto "))
                        .map(str::trim)
                    {
                        match mode {
                            "on" => {
                                agent_state.supervisor.auto_run = true;
                                persist_agent_os_state(&mut context, &agent_state);
                                context_manager.save(&context).await?;
                                println!("✅ Supervisor auto-run enabled");
                            }
                            "off" => {
                                agent_state.supervisor.auto_run = false;
                                auto_queued_task = None;
                                persist_agent_os_state(&mut context, &agent_state);
                                context_manager.save(&context).await?;
                                println!("✅ Supervisor auto-run disabled");
                            }
                            _ => println!("Use: queue auto on|off"),
                        }
                        continue;
                    }
                }

                if input.starts_with("/task ") {
                    if handle_task_command(&input, &mut agent_state) {
                        persist_agent_os_state(&mut context, &agent_state);
                        context_manager.save(&context).await?;
                    }
                    continue;
                }

                if input == "soul" || input == "/soul" {
                    println!("Usage:");
                    println!("  soul list");
                    println!("  soul switch <id>");
                    println!("  soul auto on|off");
                    continue;
                }

                if input == "/models" || input == "models" {
                    let current_model = agent_loop
                        .current_model()
                        .unwrap_or_else(|| config.model.clone());
                    println!("\n🧠 Current model: {}", current_model);
                    println!("Fetching provider models...");
                    match agent_loop.list_models().await {
                        Ok(models) => {
                            let candidates = filter_agentic_models(&models);
                            if candidates.is_empty() {
                                println!("No models returned by provider.");
                                println!();
                                continue;
                            }
                            let limit = candidates.len().min(20);
                            println!("Top agent-friendly models:");
                            for (i, model) in candidates.iter().take(limit).enumerate() {
                                let marker = if model == &current_model { "*" } else { " " };
                                println!("  {} {:>2}. {}", marker, i + 1, model);
                            }
                            println!("\nUse '/models set <model_id>' to switch.");
                            let choice = prompt_line("Select model number (Enter to keep): ")?;
                            if choice.trim().is_empty() {
                                println!();
                                continue;
                            }
                            if let Ok(index) = choice.trim().parse::<usize>() {
                                if index >= 1 && index <= limit {
                                    let selected = &candidates[index - 1];
                                    if let Err(e) = apply_model_switch(
                                        selected,
                                        &agent_loop,
                                        &mut config,
                                        &context_manager,
                                        &mut context,
                                    ).await {
                                        eprintln!("❌ Failed to switch model: {}", e);
                                    }
                                } else {
                                    eprintln!("❌ Invalid model index");
                                }
                            } else {
                                eprintln!("❌ Invalid input");
                            }
                        }
                        Err(e) => {
                            eprintln!("❌ Failed to fetch models: {}", e);
                            print_model_recommendations(&config.provider, &current_model);
                        }
                    }
                    println!();
                    continue;
                }

                if input == "/models list" || input == "models list" {
                    match agent_loop.list_models().await {
                        Ok(models) => {
                            let filtered = filter_agentic_models(&models);
                            println!("\n📦 Provider models ({}):", filtered.len());
                            for model in filtered.iter().take(60) {
                                println!("  {}", model);
                            }
                            if filtered.len() > 60 {
                                println!("  ... and {} more", filtered.len() - 60);
                            }
                            println!();
                        }
                        Err(e) => eprintln!("❌ Failed to list models: {}\n", e),
                    }
                    continue;
                }

                if let Some(model_id) = input
                    .strip_prefix("/models set ")
                    .or_else(|| input.strip_prefix("models set "))
                    .map(str::trim)
                {
                    if model_id.is_empty() {
                        eprintln!("❌ Usage: /models set <model_id>");
                        continue;
                    }
                    if let Err(e) = apply_model_switch(
                        model_id,
                        &agent_loop,
                        &mut config,
                        &context_manager,
                        &mut context,
                    ).await {
                        eprintln!("❌ Failed to switch model: {}", e);
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
                            agent_loop.set_max_iterations(active_soul.max_iterations);
                            system_prompt = active_soul.system_prompt.clone();
                            context.active_soul_id = active_soul_id.clone();
                            persist_agent_os_state(&mut context, &agent_state);
                            context_manager.save(&context).await?;
                            println!(
                                "✅ Soul switched to '{}' ({} tools, max_iter={})",
                                active_soul_id,
                                resolved_count,
                                active_soul.max_iterations
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

                let (effective_input, task_class, supervisor_task_id) =
                    if let Some(queued_task) = queued_execution.take() {
                        println!(
                            "▶ Running queued task {} ({})",
                            queued_task.id,
                            queued_task.class.as_str()
                        );
                        (
                            queued_task.prompt.clone(),
                            queued_task.class.clone(),
                            Some(queued_task.id.clone()),
                        )
                    } else {
                        let class = classify_supervised_task_class(&input);
                        let running_background = task_manager
                            .list_tasks()
                            .await
                            .into_iter()
                            .filter(|task| task.status == hypr_claw_tasks::TaskStatus::Running)
                            .collect::<Vec<_>>();
                        if has_running_background_conflict(&input, &running_background) {
                            println!(
                                "⚠️  Conflict detected with running background tasks (exclusive desktop interaction)."
                            );
                            if prompt_yes_no(
                                "Queue this task and continue background work? [Y/n] ",
                                true,
                            )? {
                                let queued_id = enqueue_supervised_task(
                                    &mut agent_state,
                                    input.clone(),
                                    class.clone(),
                                );
                                persist_agent_os_state(&mut context, &agent_state);
                                context_manager.save(&context).await?;
                                println!("🗂 Queued as {}", queued_id);
                                continue;
                            }
                        }

                        let task_id =
                            start_supervised_task(&mut agent_state, input.clone(), class.clone());
                        (
                            input.clone(),
                            class,
                            Some(task_id),
                        )
                    };

                if agent_state.soul_auto {
                    if let Some(candidate_soul_id) =
                        auto_select_soul_id(&effective_input, &available_souls)
                    {
                        if candidate_soul_id != active_soul_id {
                            if let Ok((profile, resolved_tools)) =
                                switch_soul(&candidate_soul_id, &runtime_registry, &agent_tools).await
                            {
                                let resolved_count = resolved_tools.len();
                                active_soul = profile;
                                active_soul_id = candidate_soul_id;
                                active_allowed_tools = resolved_tools;
                                agent_loop.set_max_iterations(active_soul.max_iterations);
                                system_prompt = active_soul.system_prompt.clone();
                                context.active_soul_id = active_soul_id.clone();
                                println!(
                                    "🧠 Auto soul: '{}' ({} tools, max_iter={})",
                                    active_soul_id,
                                    resolved_count,
                                    active_soul.max_iterations
                                );
                            }
                        }
                    }
                }

                context.recent_history.push(hypr_claw_memory::types::HistoryEntry {
                    timestamp: chrono::Utc::now().timestamp(),
                    role: "user".to_string(),
                    content: format!(
                        "[thread:{}] {}",
                        agent_state.active_thread_id, effective_input
                    ),
                    token_count: None,
                });
                touch_active_thread(&mut agent_state);
                context.current_plan = Some(plan_for_input(&effective_input));
                persist_agent_os_state(&mut context, &agent_state);
                context_manager.save(&context).await?;

                let task_session_key = thread_session_key(&session_key, &agent_state.active_thread_id);
                let focused_tools = focused_tools_for_input(&effective_input, &active_allowed_tools);
                let use_focused = !focused_tools.is_empty() && focused_tools.len() < active_allowed_tools.len();
                if use_focused {
                    runtime_registry.set_allowed_tools(focused_tools.clone());
                }

                let class_budget = execution_budget_for_class(&task_class);
                let effective_max_iterations = active_soul
                    .max_iterations
                    .min(class_budget.max_iterations)
                    .max(1);
                agent_loop.set_max_iterations(effective_max_iterations);

                println!(
                    "plan> analyze -> execute tools -> report  [soul={} class={} iter={}]",
                    active_soul_id,
                    task_class.as_str(),
                    agent_loop.max_iterations()
                );

                let mut turn_system_prompt = augment_system_prompt_for_turn(
                    &system_prompt,
                    &agent_state.onboarding.system_profile,
                    &active_allowed_tools,
                );

                let mut run_result = tokio::select! {
                    res = agent_loop.run(&task_session_key, &agent_name, &turn_system_prompt, &effective_input) => res,
                    _ = interrupt.notified() => Err(hypr_claw_runtime::RuntimeError::LLMError(
                        "Interrupted by user".to_string()
                    )),
                };

                if use_focused {
                    runtime_registry.set_allowed_tools(active_allowed_tools.clone());
                }

                if let Err(err) = &run_result {
                    let err_msg = err.to_string();
                    let is_tool_enforcement_error = err_msg.contains("Tool invocation required but not performed");
                    let is_provider_argument_error =
                        err_msg.contains("INVALID_ARGUMENT") || err_msg.contains("400 Bad Request");
                    let is_max_iterations_error = err_msg.contains("Max iterations");
                    let is_tool_execution_error =
                        err_msg.contains("Tool error:")
                            || err_msg.contains("dispatcher error")
                            || err_msg.contains("tool failed");

                    if is_tool_enforcement_error {
                        if let Some(candidate_soul_id) =
                            auto_select_soul_id(&effective_input, &available_souls)
                        {
                            if candidate_soul_id != active_soul_id {
                                if let Ok((profile, resolved_tools)) =
                                    switch_soul(&candidate_soul_id, &runtime_registry, &agent_tools).await
                                {
                                    active_soul = profile;
                                    active_soul_id = candidate_soul_id;
                                    active_allowed_tools = resolved_tools;
                                    agent_loop.set_max_iterations(
                                        active_soul
                                            .max_iterations
                                            .min(class_budget.max_iterations)
                                            .max(1),
                                    );
                                    system_prompt = active_soul.system_prompt.clone();
                                    context.active_soul_id = active_soul_id.clone();
                                    println!(
                                        "🧠 Recovery soul switch: {} (max_iter={})",
                                        active_soul_id,
                                        active_soul.max_iterations
                                    );
                                    turn_system_prompt = augment_system_prompt_for_turn(
                                        &system_prompt,
                                        &agent_state.onboarding.system_profile,
                                        &active_allowed_tools,
                                    );
                                }
                            }
                        }
                        let retry_prompt = format!(
                            "{}\n\nExecute this now using available tools. Do not answer with explanation-only text. If one tool fails, choose another available tool and continue until completion or clear blocker.",
                            effective_input
                        );
                        run_result = tokio::select! {
                            res = agent_loop.run(&task_session_key, &agent_name, &turn_system_prompt, &retry_prompt) => res,
                            _ = interrupt.notified() => Err(hypr_claw_runtime::RuntimeError::LLMError(
                                "Interrupted by user".to_string()
                            )),
                        };
                    } else if is_provider_argument_error {
                        let emergency_tools = emergency_tool_subset(&effective_input, &active_allowed_tools);
                        if !emergency_tools.is_empty() && emergency_tools.len() < active_allowed_tools.len() {
                            runtime_registry.set_allowed_tools(emergency_tools);
                            run_result = tokio::select! {
                                res = agent_loop.run(&task_session_key, &agent_name, &turn_system_prompt, &effective_input) => res,
                                _ = interrupt.notified() => Err(hypr_claw_runtime::RuntimeError::LLMError(
                                    "Interrupted by user".to_string()
                                )),
                            };
                            runtime_registry.set_allowed_tools(active_allowed_tools.clone());
                        }
                    } else if is_max_iterations_error || is_tool_execution_error {
                        let recovery_prompt = format!(
                            "{}\n\nRecovery mode:\n1) keep working with available tools\n2) if a tool fails, pick an alternative backend/tool\n3) end with final status and exact remaining blocker only if no alternative worked.",
                            effective_input
                        );
                        run_result = tokio::select! {
                            res = agent_loop.run(&task_session_key, &agent_name, &turn_system_prompt, &recovery_prompt) => res,
                            _ = interrupt.notified() => Err(hypr_claw_runtime::RuntimeError::LLMError(
                                "Interrupted by user".to_string()
                            )),
                        };
                    }
                }

                agent_loop.set_max_iterations(active_soul.max_iterations);

                match run_result {
                    Ok(response) => {
                        if let Some(task_id) = &supervisor_task_id {
                            mark_supervised_task_completed(&mut agent_state, task_id);
                        }
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
                        if let Some(task_id) = &supervisor_task_id {
                            mark_supervised_task_failed(&mut agent_state, task_id, e.to_string());
                        }
                        mark_plan_failed(&mut context, &e.to_string());
                        persist_agent_os_state(&mut context, &agent_state);
                        context_manager.save(&context).await?;
                        if e.to_string().contains("Interrupted by user") {
                            println!("⏹ Request interrupted by user.\n");
                        } else {
                            eprintln!("❌ Error: {}\n", e);
                        }
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

fn normalize_runtime_tool_name(name: &str) -> String {
    match name {
        "screen.capture" | "capture.screen" => "desktop.capture_screen".to_string(),
        "screen.ocr" | "ocr.screen" => "desktop.ocr_screen".to_string(),
        "screen.find_text" | "text.find_on_screen" => "desktop.find_text".to_string(),
        "screen.click_text" | "text.click_on_screen" => "desktop.click_text".to_string(),
        "gmail.open" => "desktop.open_gmail".to_string(),
        "browser.open_url" => "desktop.open_url".to_string(),
        "browser.search" => "desktop.search_web".to_string(),
        "app.open" | "app.launch" => "desktop.launch_app".to_string(),
        "process.spawn" => "proc.spawn".to_string(),
        "process.kill" => "proc.kill".to_string(),
        "process.list" => "proc.list".to_string(),
        other => other.to_string(),
    }
}

fn build_tui_snapshot(
    provider_name: &str,
    model: &str,
    session_key: &str,
    user_id: &str,
    active_soul_id: &str,
    agent_state: &AgentOsState,
    context: &hypr_claw_memory::types::ContextData,
    task_list: &[hypr_claw_tasks::TaskInfo],
    soul_count: usize,
    action_feed: &Arc<Mutex<Vec<String>>>,
) -> tui::TuiSnapshot {
    let running = task_list
        .iter()
        .filter(|t| t.status == hypr_claw_tasks::TaskStatus::Running)
        .count();
    let done = task_list
        .iter()
        .filter(|t| t.status == hypr_claw_tasks::TaskStatus::Completed)
        .count();
    let failed = task_list
        .iter()
        .filter(|t| t.status == hypr_claw_tasks::TaskStatus::Failed)
        .count();
    let supervisor_queued = agent_state
        .supervisor
        .tasks
        .iter()
        .filter(|t| t.status == SupervisedTaskStatus::Queued)
        .count();
    let supervisor_running = agent_state
        .supervisor
        .tasks
        .iter()
        .filter(|t| t.status == SupervisedTaskStatus::Running)
        .count();
    let supervisor_done = agent_state
        .supervisor
        .tasks
        .iter()
        .filter(|t| t.status == SupervisedTaskStatus::Completed)
        .count();
    let supervisor_failed = agent_state
        .supervisor
        .tasks
        .iter()
        .filter(|t| t.status == SupervisedTaskStatus::Failed)
        .count();
    let supervisor_cancelled = agent_state
        .supervisor
        .tasks
        .iter()
        .filter(|t| t.status == SupervisedTaskStatus::Cancelled)
        .count();

    let threads = agent_state
        .task_threads
        .iter()
        .map(|t| tui::ThreadRow {
            id: t.id.clone(),
            title: t.title.clone(),
            active: t.id == agent_state.active_thread_id,
            archived: t.archived,
        })
        .collect::<Vec<_>>();

    let tasks = task_list
        .iter()
        .map(|t| tui::TaskRow {
            id: t.id.clone(),
            state: format!("{:?}", t.status).to_uppercase(),
            progress_percent: (t.progress.clamp(0.0, 1.0) * 100.0).round() as u16,
            description: t.description.clone(),
        })
        .collect::<Vec<_>>();
    let mut supervisor_tasks = agent_state
        .supervisor
        .tasks
        .iter()
        .filter(|t| {
            t.status == SupervisedTaskStatus::Queued || t.status == SupervisedTaskStatus::Running
        })
        .cloned()
        .collect::<Vec<_>>();
    supervisor_tasks.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    let supervisor_tasks = supervisor_tasks
        .into_iter()
        .take(12)
        .map(|t| tui::SupervisorTaskRow {
            id: t.id,
            state: match t.status {
                SupervisedTaskStatus::Queued => "queued".to_string(),
                SupervisedTaskStatus::Running => "running".to_string(),
                SupervisedTaskStatus::Completed => "done".to_string(),
                SupervisedTaskStatus::Failed => "failed".to_string(),
                SupervisedTaskStatus::Cancelled => "cancel".to_string(),
            },
            class: t.class.as_str().to_string(),
            prompt: t.prompt,
        })
        .collect::<Vec<_>>();

    let plan = context
        .current_plan
        .as_ref()
        .map(|p| {
            format!(
                "{} [{} step {}/{}]",
                p.goal,
                p.status,
                p.current_step + 1,
                p.steps.len()
            )
        })
        .unwrap_or_else(|| "none".to_string());

    let action_feed = action_feed
        .lock()
        .ok()
        .map(|g| g.clone())
        .unwrap_or_default();
    let recent_messages = context
        .recent_history
        .iter()
        .rev()
        .take(12)
        .map(|h| format!("{}: {}", h.role, h.content))
        .collect::<Vec<_>>();

    tui::TuiSnapshot {
        provider: provider_name.to_string(),
        model: model.to_string(),
        user: format!("{} ({})", display_name(agent_state, user_id), user_id),
        session: session_key.to_string(),
        soul: active_soul_id.to_string(),
        thread: agent_state.active_thread_id.clone(),
        souls_count: soul_count,
        task_counts: (task_list.len(), running, done, failed),
        supervisor_counts: (
            supervisor_queued,
            supervisor_running,
            supervisor_done,
            supervisor_failed,
            supervisor_cancelled,
        ),
        supervisor_auto_run: agent_state.supervisor.auto_run,
        history_len: context.recent_history.len(),
        facts_len: context.facts.len(),
        approval_counts: (
            context.approval_history.len(),
            context.pending_approvals.len(),
        ),
        token_usage: (
            context.token_usage.total_input,
            context.token_usage.total_output,
            context.token_usage.by_session,
        ),
        plan,
        threads,
        tasks,
        supervisor_tasks,
        action_feed,
        recent_messages,
    }
}

const UI_RESET: &str = "\x1b[0m";
const UI_BOLD: &str = "\x1b[1m";
const UI_DIM: &str = "\x1b[2m";
const UI_ACCENT: &str = "\x1b[38;5;39m";
const UI_INFO: &str = "\x1b[38;5;81m";
const UI_SUCCESS: &str = "\x1b[38;5;42m";
const UI_WARN: &str = "\x1b[38;5;214m";
const UI_DANGER: &str = "\x1b[38;5;203m";

fn use_color() -> bool {
    std::env::var_os("NO_COLOR").is_none()
}

fn paint(text: &str, style: &str) -> String {
    if use_color() {
        format!("{style}{text}{UI_RESET}")
    } else {
        text.to_string()
    }
}

fn ui_title(text: &str) -> String {
    paint(text, UI_BOLD)
}

fn ui_accent(text: &str) -> String {
    paint(text, UI_ACCENT)
}

fn ui_dim(text: &str) -> String {
    paint(text, UI_DIM)
}

fn ui_info(text: &str) -> String {
    paint(text, UI_INFO)
}

fn ui_success(text: &str) -> String {
    paint(text, UI_SUCCESS)
}

fn ui_warn(text: &str) -> String {
    paint(text, UI_WARN)
}

fn ui_danger(text: &str) -> String {
    paint(text, UI_DANGER)
}

fn status_badge(status: &hypr_claw_tasks::TaskStatus) -> String {
    match status {
        hypr_claw_tasks::TaskStatus::Running => ui_info("RUN"),
        hypr_claw_tasks::TaskStatus::Pending => ui_warn("PEND"),
        hypr_claw_tasks::TaskStatus::Completed => ui_success("DONE"),
        hypr_claw_tasks::TaskStatus::Failed => ui_danger("FAIL"),
        hypr_claw_tasks::TaskStatus::Cancelled => ui_warn("STOP"),
    }
}

fn progress_bar(progress: f32, width: usize) -> String {
    let p = progress.clamp(0.0, 1.0);
    let filled = (p * width as f32).round() as usize;
    let filled = filled.min(width);
    let mut bar = String::new();
    bar.push('[');
    bar.push_str(&"█".repeat(filled));
    bar.push_str(&"░".repeat(width.saturating_sub(filled)));
    bar.push(']');
    bar
}

fn short_model_name(model: &str) -> String {
    model.split('/').next_back().unwrap_or(model).to_string()
}

fn format_timestamp(ts: i64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp(ts, 0)
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
        .unwrap_or_else(|| "n/a".to_string())
}

fn truncate_for_table(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        return value.to_string();
    }
    let mut out: String = value.chars().take(max.saturating_sub(3)).collect();
    out.push_str("...");
    out
}

fn sanitize_user_input_line(raw: &str) -> String {
    let mut value = raw.trim().to_string();
    for _ in 0..4 {
        if let Some(rest) = value.strip_prefix("tui> ") {
            value = rest.trim().to_string();
            continue;
        }
        if value.starts_with("hypr[") {
            if let Some((_, rest)) = value.rsplit_once("]>") {
                let cleaned = rest.trim();
                if !cleaned.is_empty() {
                    value = cleaned.to_string();
                    continue;
                }
            }
        }
        break;
    }
    value
}

fn print_console_bootstrap(
    provider_name: &str,
    model: &str,
    agent_name: &str,
    display_name: &str,
    user_id: &str,
    session_key: &str,
    active_soul_id: &str,
    active_thread_id: &str,
) {
    println!(
        "{}",
        ui_accent("╔════════════════════════════════════════════════════════════════════╗")
    );
    println!(
        "{}",
        ui_title("║                  Hypr-Claw OS Assistant Console                   ║")
    );
    println!(
        "{}",
        ui_accent("╠════════════════════════════════════════════════════════════════════╣")
    );
    println!(
        "║ Provider : {:<54} ║",
        truncate_for_table(provider_name, 54)
    );
    println!("║ Model    : {:<54} ║", truncate_for_table(model, 54));
    println!(
        "║ User     : {:<54} ║",
        truncate_for_table(&format!("{} ({})", display_name, user_id), 54)
    );
    println!("║ Session  : {:<54} ║", truncate_for_table(session_key, 54));
    println!(
        "║ Profile  : {:<54} ║",
        truncate_for_table(
            &format!(
                "agent={}  soul={}  thread={}",
                agent_name, active_soul_id, active_thread_id
            ),
            54
        )
    );
    println!(
        "{}",
        ui_accent("╠════════════════════════════════════════════════════════════════════╣")
    );
    println!(
        "{}",
        ui_dim(&format!(
            "║ Commands: {:<58}║",
            truncate_for_table(
                "help, status, dashboard, /tui, /models, soul, /task, queue...",
                58
            )
        ))
    );
    println!(
        "{}",
        ui_accent("╚════════════════════════════════════════════════════════════════════╝")
    );
    println!();
}

fn print_help() {
    println!("\n{}", ui_title("Hypr-Claw Command Reference"));
    println!("  {}", ui_accent("Core"));
    println!("    help                  Show this help");
    println!("    status                Runtime state snapshot");
    println!("    dashboard | /dashboard|dash  Extended operational dashboard");
    println!("    tui | /tui            Open full-screen command center");
    println!("      (inside TUI: :q close, :r refresh, :exit quit app)");
    println!("    repl | /repl          Return to standard prompt mode");
    println!("    scan                  Re-run standard/deep system learning scan");
    println!("    trust on|off          Toggle trusted full-auto execution mode");
    println!("    clear                 Clear terminal");
    println!("    interrupt             Show interrupt hint (Ctrl+C)");
    println!("    exit | quit           Exit agent");
    println!("  {}", ui_accent("Models"));
    println!("    /models               Interactive model switch");
    println!("    /models list          List provider models");
    println!("    /models set <id>      Set model directly");
    println!("  {}", ui_accent("Souls"));
    println!("    soul list             List installed soul profiles");
    println!("    soul switch <id>      Switch active soul");
    println!("    soul auto on|off      Toggle intent-based soul routing");
    println!("  {}", ui_accent("Tasks"));
    println!("    tasks                 Show background task table");
    println!("    /task new <title>     Create task thread");
    println!("    /task list            List task threads");
    println!("    /task switch <id>     Switch active thread");
    println!("    /task close <id>      Archive thread");
    println!("    queue                 Show supervisor queue");
    println!("    queue add <prompt>    Add task prompt to queue");
    println!("    queue run             Run next queued task");
    println!("    queue clear           Cancel queued items");
    println!("    queue auto on|off     Toggle auto-run for queued tasks");
    println!("  {}", ui_accent("System"));
    println!("    profile               Show learned system profile");
    println!("    scan                  Re-run system scan");
    println!("    approve <task_id>     Approval helper");
    println!();
}

fn print_tasks_panel(task_list: &[hypr_claw_tasks::TaskInfo]) {
    println!("\n{}", ui_title("Background Task Monitor"));
    println!(
        "  {:<14} {:<8} {:>6}  {:<18} {}",
        "ID", "STATE", "PROG%", "PROGRESS", "DESCRIPTION"
    );
    println!("  {}", ui_dim(&"─".repeat(88)));

    if task_list.is_empty() {
        println!(
            "  {:<14} {:<8} {:>6}  {:<18} {}",
            "-",
            "-",
            "-",
            progress_bar(0.0, 16),
            ui_dim("no tasks")
        );
        return;
    }

    let mut sorted = task_list.to_vec();
    sorted.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    for task in sorted {
        let prog = task.progress.clamp(0.0, 1.0);
        let bar = progress_bar(prog, 16);
        println!(
            "  {:<14} {:<8} {:>5.0}%  {:<18} {}",
            truncate_for_table(&task.id, 14),
            status_badge(&task.status),
            task.progress * 100.0,
            bar,
            truncate_for_table(&task.description, 44),
        );
    }
}

fn print_status_panel(
    session_key: &str,
    agent_name: &str,
    display_name: &str,
    active_soul_id: &str,
    model: &str,
    soul_auto: bool,
    active_thread_id: &str,
    agent_state: &AgentOsState,
    context: &hypr_claw_memory::types::ContextData,
    task_list: &[hypr_claw_tasks::TaskInfo],
) {
    let running = task_list
        .iter()
        .filter(|t| t.status == hypr_claw_tasks::TaskStatus::Running)
        .count();
    let completed = task_list
        .iter()
        .filter(|t| t.status == hypr_claw_tasks::TaskStatus::Completed)
        .count();
    let failed = task_list
        .iter()
        .filter(|t| t.status == hypr_claw_tasks::TaskStatus::Failed)
        .count();
    let supervisor_queued = agent_state
        .supervisor
        .tasks
        .iter()
        .filter(|t| t.status == SupervisedTaskStatus::Queued)
        .count();
    let supervisor_running = agent_state
        .supervisor
        .tasks
        .iter()
        .filter(|t| t.status == SupervisedTaskStatus::Running)
        .count();

    println!("\n{}", ui_title("Runtime Status"));
    println!("  Session      : {}", ui_dim(session_key));
    println!(
        "  Agent/User   : {} / {}",
        ui_info(agent_name),
        ui_info(display_name)
    );
    println!(
        "  Soul         : {} (auto: {})",
        ui_accent(active_soul_id),
        if soul_auto { "on" } else { "off" }
    );
    println!(
        "  Trust Mode   : {}",
        if agent_state.onboarding.trusted_full_auto {
            ui_success("full-auto")
        } else {
            ui_warn("standard")
        }
    );
    println!(
        "  Scan Depth   : {}",
        if agent_state.onboarding.deep_scan_completed {
            ui_success("deep")
        } else {
            ui_dim("standard")
        }
    );
    println!("  Model        : {}", ui_info(model));
    println!("  Thread       : {}", ui_dim(active_thread_id));
    println!(
        "  Threads      : {} active / {} total",
        agent_state
            .task_threads
            .iter()
            .filter(|t| !t.archived)
            .count(),
        agent_state.task_threads.len()
    );
    println!(
        "  Tasks        : {} running / {} completed / {} failed",
        ui_info(&running.to_string()),
        ui_success(&completed.to_string()),
        if failed > 0 {
            ui_danger(&failed.to_string())
        } else {
            ui_success(&failed.to_string())
        }
    );
    println!(
        "  Supervisor   : {} queued / {} running (auto: {})",
        supervisor_queued,
        supervisor_running,
        if agent_state.supervisor.auto_run {
            "on"
        } else {
            "off"
        }
    );
    println!(
        "  Memory       : history={} facts={} approvals={} pending={}",
        context.recent_history.len(),
        context.facts.len(),
        context.approval_history.len(),
        context.pending_approvals.len()
    );
    println!(
        "  Tokens       : input={} output={} session={}",
        context.token_usage.total_input,
        context.token_usage.total_output,
        context.token_usage.by_session
    );
    if let Some(plan) = &context.current_plan {
        println!(
            "  Plan         : {} [{} step {}/{}]",
            truncate_for_table(&plan.goal, 36),
            plan.status,
            plan.current_step.saturating_add(1),
            plan.steps.len()
        );
    } else {
        println!("  Plan         : none");
    }
}

fn print_runtime_dashboard(
    provider_name: &str,
    model: &str,
    session_key: &str,
    agent_name: &str,
    user_id: &str,
    active_soul_id: &str,
    agent_state: &AgentOsState,
    context: &hypr_claw_memory::types::ContextData,
    task_list: &[hypr_claw_tasks::TaskInfo],
    soul_count: usize,
) {
    println!(
        "\n{}",
        ui_accent("════════════════════════════════  Dashboard  ════════════════════════════════")
    );
    println!(
        "{} {} | {} {}",
        ui_dim("Provider:"),
        ui_info(provider_name),
        ui_dim("Model:"),
        ui_info(model)
    );
    println!(
        "{} {} ({}) | {} {} | {} {}",
        ui_dim("Identity:"),
        ui_info(&display_name(agent_state, user_id)),
        user_id,
        ui_dim("Agent:"),
        ui_info(agent_name),
        ui_dim("Session:"),
        ui_dim(session_key)
    );
    println!(
        "{} {} (auto: {}) | {} {} | {} {}",
        ui_dim("Soul:"),
        ui_accent(active_soul_id),
        if agent_state.soul_auto { "on" } else { "off" },
        ui_dim("Available souls:"),
        soul_count,
        ui_dim("Active thread:"),
        ui_dim(&agent_state.active_thread_id)
    );
    println!(
        "{} {} | {} {}",
        ui_dim("Trust mode:"),
        if agent_state.onboarding.trusted_full_auto {
            ui_success("full-auto")
        } else {
            ui_warn("standard")
        },
        ui_dim("Scan:"),
        if agent_state.onboarding.deep_scan_completed {
            ui_success("deep")
        } else {
            ui_dim("standard")
        }
    );

    let running = task_list
        .iter()
        .filter(|t| t.status == hypr_claw_tasks::TaskStatus::Running)
        .count();
    let done = task_list
        .iter()
        .filter(|t| t.status == hypr_claw_tasks::TaskStatus::Completed)
        .count();
    let failed = task_list
        .iter()
        .filter(|t| t.status == hypr_claw_tasks::TaskStatus::Failed)
        .count();
    let supervisor_queued = agent_state
        .supervisor
        .tasks
        .iter()
        .filter(|t| t.status == SupervisedTaskStatus::Queued)
        .count();
    let supervisor_running = agent_state
        .supervisor
        .tasks
        .iter()
        .filter(|t| t.status == SupervisedTaskStatus::Running)
        .count();

    println!(
        "{} {} total | {} running | {} done | {} failed",
        ui_dim("Tasks:"),
        task_list.len(),
        running,
        done,
        failed
    );
    println!(
        "{} {} queued | {} running | auto {}",
        ui_dim("Supervisor:"),
        supervisor_queued,
        supervisor_running,
        if agent_state.supervisor.auto_run {
            "on"
        } else {
            "off"
        }
    );

    if let Some(plan) = &context.current_plan {
        println!(
            "{} {} [{}] step {}/{}",
            ui_dim("Plan:"),
            truncate_for_table(&plan.goal, 56),
            plan.status,
            plan.current_step.saturating_add(1),
            plan.steps.len()
        );
    } else {
        println!("{} none", ui_dim("Plan:"));
    }

    let top_tools = {
        let mut items: Vec<(&String, &u64)> = context.tool_stats.by_tool.iter().collect();
        items.sort_by(|a, b| b.1.cmp(a.1));
        items.into_iter().take(5).collect::<Vec<_>>()
    };
    if top_tools.is_empty() {
        println!("{} no recorded calls", ui_dim("Tool Usage:"));
    } else {
        let formatted = top_tools
            .iter()
            .map(|(tool, count)| format!("{}({})", tool, count))
            .collect::<Vec<_>>()
            .join(", ");
        println!(
            "{} total={} failures={} top=[{}]",
            ui_dim("Tool Usage:"),
            context.tool_stats.total_calls,
            context.tool_stats.failures,
            formatted
        );
    }

    if agent_state.onboarding.last_scan_at.is_some() {
        let distro = agent_state
            .onboarding
            .system_profile
            .pointer("/platform/distro_name")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let kernel = agent_state
            .onboarding
            .system_profile
            .pointer("/platform/kernel")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let ws = agent_state
            .onboarding
            .system_profile
            .pointer("/desktop/active_workspace")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        println!(
            "{} {} | kernel {} | workspace {} | scanned {}",
            ui_dim("Host Profile:"),
            distro,
            kernel,
            ws,
            format_timestamp(agent_state.onboarding.last_scan_at.unwrap_or(0))
        );
    }

    println!(
        "{} history={} facts={} approvals={} pending={} long_summary={} chars",
        ui_dim("Memory:"),
        context.recent_history.len(),
        context.facts.len(),
        context.approval_history.len(),
        context.pending_approvals.len(),
        context.long_term_summary.chars().count()
    );
    println!(
        "{}",
        ui_accent("══════════════════════════════════════════════════════════════════════════════")
    );
}

fn model_priority(model_id: &str) -> usize {
    match model_id {
        "z-ai/glm4.7" => 0,
        "z-ai/glm5" => 1,
        "moonshotai/kimi-k2.5" => 2,
        "moonshotai/kimi-k2-instruct-0905" => 3,
        "qwen/qwen3-coder-480b-a35b-instruct" => 4,
        "meta/llama-4-maverick-17b-128e-instruct" => 5,
        _ => 100,
    }
}

fn filter_agentic_models(models: &[String]) -> Vec<String> {
    let blocked_tokens = [
        "embed",
        "embedding",
        "guard",
        "safety",
        "reward",
        "retriever",
        "parse",
        "clip",
        "deplot",
        "kosmos",
        "paligemma",
        "vila",
        "streampetr",
    ];

    let mut filtered: Vec<String> = models
        .iter()
        .filter(|id| {
            let lower = id.to_lowercase();
            !blocked_tokens.iter().any(|token| lower.contains(token))
        })
        .cloned()
        .collect();

    filtered.sort_by(|a, b| {
        model_priority(a)
            .cmp(&model_priority(b))
            .then_with(|| a.cmp(b))
    });
    filtered
}

fn print_model_recommendations(provider: &LLMProvider, current_model: &str) {
    match provider {
        LLMProvider::Nvidia => {
            println!("\nRecommended NVIDIA models for agentic tasks:");
            let recommendations = [
                "z-ai/glm4.7",
                "z-ai/glm5",
                "moonshotai/kimi-k2.5",
                "qwen/qwen3-coder-480b-a35b-instruct",
                "meta/llama-4-maverick-17b-128e-instruct",
            ];
            for model in recommendations {
                let marker = if model == current_model { "*" } else { " " };
                println!("  {} {}", marker, model);
            }
            println!("Use '/models set <model_id>' to switch.");
        }
        _ => {
            println!("Model recommendations are currently tuned for NVIDIA provider.");
        }
    }
}

async fn apply_model_switch<S, L, D, R, Sum>(
    model_id: &str,
    agent_loop: &hypr_claw_runtime::AgentLoop<S, L, D, R, Sum>,
    config: &mut Config,
    context_manager: &hypr_claw_memory::ContextManager,
    context: &mut hypr_claw_memory::types::ContextData,
) -> Result<(), String>
where
    S: hypr_claw_runtime::SessionStore,
    L: hypr_claw_runtime::LockManager,
    D: hypr_claw_runtime::ToolDispatcher,
    R: hypr_claw_runtime::ToolRegistry,
    Sum: hypr_claw_runtime::Summarizer,
{
    agent_loop.set_model(model_id).map_err(|e| e.to_string())?;
    config.model = model_id.to_string();
    config.save().map_err(|e| e.to_string())?;

    if !context.system_state.is_object() {
        context.system_state = json!({});
    }
    if let Some(obj) = context.system_state.as_object_mut() {
        obj.insert("active_model".to_string(), json!(model_id));
    }
    context_manager
        .save(context)
        .await
        .map_err(|e| e.to_string())?;

    println!("✅ Model switched to {}", model_id);
    if model_id == "z-ai/glm4.7" {
        println!("ℹ️  GLM-4.7 profile enabled (agentic terminal tuning).");
    }
    Ok(())
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
enum SupervisedTaskClass {
    Question,
    Action,
    Investigation,
}

impl SupervisedTaskClass {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Question => "question",
            Self::Action => "action",
            Self::Investigation => "investigation",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
enum SupervisedTaskStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SupervisedTask {
    id: String,
    prompt: String,
    class: SupervisedTaskClass,
    status: SupervisedTaskStatus,
    created_at: i64,
    updated_at: i64,
    error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SupervisorState {
    #[serde(default = "default_supervisor_auto_run")]
    auto_run: bool,
    #[serde(default = "default_supervisor_next_id")]
    next_id: u64,
    #[serde(default)]
    tasks: Vec<SupervisedTask>,
}

fn default_supervisor_auto_run() -> bool {
    true
}

fn default_supervisor_next_id() -> u64 {
    1
}

impl Default for SupervisorState {
    fn default() -> Self {
        Self {
            auto_run: default_supervisor_auto_run(),
            next_id: default_supervisor_next_id(),
            tasks: Vec::new(),
        }
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
    #[serde(default)]
    supervisor: SupervisorState,
}

impl Default for AgentOsState {
    fn default() -> Self {
        Self {
            onboarding: OnboardingState::default(),
            soul_auto: true,
            task_threads: vec![TaskThread::new("task-1".to_string(), "Main".to_string())],
            active_thread_id: "task-1".to_string(),
            supervisor: SupervisorState::default(),
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
    #[serde(default)]
    deep_scan_completed: bool,
    #[serde(default)]
    trusted_full_auto: bool,
}

impl Default for OnboardingState {
    fn default() -> Self {
        Self {
            completed: false,
            preferred_name: String::new(),
            profile_confirmed: false,
            system_profile: json!({}),
            last_scan_at: None,
            deep_scan_completed: false,
            trusted_full_auto: false,
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

fn full_auto_flag_path() -> &'static str {
    "./data/full_auto_mode.flag"
}

fn set_full_auto_mode(enabled: bool) -> io::Result<()> {
    let path = full_auto_flag_path();
    if enabled {
        std::fs::write(path, b"enabled")
    } else if std::path::Path::new(path).exists() {
        std::fs::remove_file(path)
    } else {
        Ok(())
    }
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

    state.onboarding.trusted_full_auto = prompt_yes_no(
        "Enable trusted full-auto mode (single consent for local actions)? [y/N] ",
        false,
    )?;
    let _ = set_full_auto_mode(state.onboarding.trusted_full_auto);

    if prompt_yes_no("Allow first-time system study scan? [Y/n] ", true)? {
        let deep_scan = prompt_yes_no(
            "Run deep system learning scan (home/.config/packages/hyprland)? [Y/n] ",
            true,
        )?;
        println!(
            "🔎 Running {} scan...",
            if deep_scan { "deep" } else { "standard" }
        );
        state.onboarding.system_profile = if deep_scan {
            collect_deep_system_profile(user_id).await
        } else {
            collect_system_profile(user_id).await
        };
        state.onboarding.deep_scan_completed = deep_scan;
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
    println!(
        "  Deep scan data: {}",
        if profile.pointer("/deep_scan").is_some() {
            "yes"
        } else {
            "no"
        }
    );
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

fn profile_needs_capability_refresh(profile: &Value) -> bool {
    profile.pointer("/capabilities").is_none() || profile.pointer("/paths/downloads").is_none()
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
        "code-oss",
        "codium",
        "codex",
        "kiro-cli",
        "firefox",
        "chromium",
        "google-chrome-stable",
        "kitty",
        "wezterm",
        "thunderbird",
        "discord",
        "slack",
        "telegram-desktop",
        "swww",
        "hyprpaper",
        "caelestia",
        "grim",
        "hyprshot",
        "tesseract",
        "wtype",
        "ydotool",
        "wlrctl",
        "hyprctl",
        "flatpak",
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

    let command_enabled = |name: &str| -> bool {
        commands
            .get(name)
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    };

    let wallpaper_backends = [
        ("swww", "swww"),
        ("hyprpaper", "hyprpaper"),
        ("caelestia", "caelestia"),
    ]
    .iter()
    .filter_map(|(cmd, label)| command_enabled(cmd).then_some((*label).to_string()))
    .collect::<Vec<String>>();

    let screenshot_backends = [("grim", "grim"), ("hyprshot", "hyprshot")]
        .iter()
        .filter_map(|(cmd, label)| command_enabled(cmd).then_some((*label).to_string()))
        .collect::<Vec<String>>();

    let input_backends = [
        ("wtype", "wtype"),
        ("ydotool", "ydotool"),
        ("wlrctl", "wlrctl"),
    ]
    .iter()
    .filter_map(|(cmd, label)| command_enabled(cmd).then_some((*label).to_string()))
    .collect::<Vec<String>>();

    let home = std::env::var("HOME").unwrap_or_default();
    let downloads = format!("{home}/Downloads");
    let pictures = format!("{home}/Pictures");
    let documents = format!("{home}/Documents");

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
        "commands": commands,
        "paths": {
            "home": home,
            "downloads": downloads,
            "pictures": pictures,
            "documents": documents
        },
        "capabilities": {
            "wallpaper_backends": wallpaper_backends,
            "screenshot_backends": screenshot_backends,
            "input_backends": input_backends,
            "ocr_available": command_enabled("tesseract")
        }
    })
}

async fn collect_deep_system_profile(user_id: &str) -> Value {
    let mut base = collect_system_profile(user_id).await;
    let home = base
        .pointer("/user/home")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();

    let deep = tokio::task::spawn_blocking(move || collect_deep_scan_sync(&home))
        .await
        .unwrap_or_else(|_| json!({"error": "deep scan failed"}));

    if let Some(obj) = base.as_object_mut() {
        obj.insert("deep_scan".to_string(), deep);
    }
    base
}

fn collect_deep_scan_sync(home: &str) -> Value {
    let home_path = std::path::PathBuf::from(home);
    let config_path = home_path.join(".config");
    let hypr_path = config_path.join("hypr");

    let top_home = list_directory_entries(&home_path, 220);
    let config_entries = list_directory_entries(&config_path, 260);
    let projects = find_project_roots(&home_path, 2, 320);
    let (hypr_main, hypr_binds, hypr_exec_once, hypr_workspace_rules) =
        parse_hypr_config(&hypr_path);
    let (pkg_explicit_count, pkg_explicit_sample) = pacman_query_lines(&["-Qqe"], 160);
    let (pkg_aur_count, pkg_aur_sample) = pacman_query_lines(&["-Qm"], 120);
    let (launchers, launcher_commands) = desktop_entries(&home_path, 320);
    let (history_top, history_recent) = shell_history_summary(&home_path, 4500);

    json!({
        "scanned_at": chrono::Utc::now().timestamp(),
        "home_inventory": {
            "top_level": top_home,
            "config_entries": config_entries,
            "project_roots": projects
        },
        "hyprland": {
            "main_config": hypr_main,
            "binds_sample": hypr_binds,
            "exec_once_sample": hypr_exec_once,
            "workspace_rules_sample": hypr_workspace_rules
        },
        "packages": {
            "pacman_explicit_count": pkg_explicit_count,
            "pacman_explicit_sample": pkg_explicit_sample,
            "aur_count": pkg_aur_count,
            "aur_sample": pkg_aur_sample
        },
        "desktop_apps": {
            "launchers_sample": launchers,
            "launcher_commands_sample": launcher_commands
        },
        "usage": {
            "shell_history_top": history_top,
            "recent_commands_sample": history_recent
        }
    })
}

fn list_directory_entries(path: &std::path::Path, limit: usize) -> Vec<Value> {
    let Ok(read_dir) = std::fs::read_dir(path) else {
        return Vec::new();
    };
    let mut rows = Vec::new();
    for entry in read_dir.flatten().take(limit) {
        let file_name = entry.file_name().to_string_lossy().to_string();
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        rows.push(json!({
            "name": file_name,
            "kind": if meta.is_dir() { "dir" } else { "file" },
            "bytes": if meta.is_file() { meta.len() } else { 0 }
        }));
    }
    rows.sort_by(|a, b| {
        let an = a.get("name").and_then(|v| v.as_str()).unwrap_or_default();
        let bn = b.get("name").and_then(|v| v.as_str()).unwrap_or_default();
        an.cmp(bn)
    });
    rows
}

fn find_project_roots(home: &std::path::Path, max_depth: usize, max_dirs: usize) -> Vec<String> {
    let mut stack = vec![(home.to_path_buf(), 0usize)];
    let mut visited = 0usize;
    let mut roots = Vec::new();

    while let Some((dir, depth)) = stack.pop() {
        if visited >= max_dirs {
            break;
        }
        visited += 1;
        if dir.join(".git").exists() {
            roots.push(dir.to_string_lossy().to_string());
            if roots.len() >= 120 {
                break;
            }
        }
        if depth >= max_depth {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten().take(80) {
            let p = entry.path();
            if !p.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') && name != ".config" {
                continue;
            }
            stack.push((p, depth + 1));
        }
    }

    roots.sort();
    roots
}

fn parse_hypr_config(
    hypr_dir: &std::path::Path,
) -> (String, Vec<String>, Vec<String>, Vec<String>) {
    let main = hypr_dir.join("hyprland.conf");
    if !main.exists() {
        return (String::new(), Vec::new(), Vec::new(), Vec::new());
    }
    let content = std::fs::read_to_string(&main).unwrap_or_default();
    let mut binds = Vec::new();
    let mut exec_once = Vec::new();
    let mut workspace_rules = Vec::new();

    for line in content.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if line.starts_with('#') {
            continue;
        }
        if (line.starts_with("bind") || line.starts_with("binde") || line.starts_with("bindm"))
            && binds.len() < 120
        {
            binds.push(line.to_string());
        }
        if line.starts_with("exec-once") && exec_once.len() < 80 {
            exec_once.push(line.to_string());
        }
        if line.starts_with("workspace") && workspace_rules.len() < 80 {
            workspace_rules.push(line.to_string());
        }
    }

    (
        main.to_string_lossy().to_string(),
        binds,
        exec_once,
        workspace_rules,
    )
}

fn pacman_query_lines(args: &[&str], sample_limit: usize) -> (usize, Vec<String>) {
    let output = std::process::Command::new("pacman").args(args).output();
    let Ok(output) = output else {
        return (0, Vec::new());
    };
    if !output.status.success() {
        return (0, Vec::new());
    }
    let lines = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect::<Vec<String>>();
    let count = lines.len();
    let sample = lines
        .into_iter()
        .take(sample_limit)
        .collect::<Vec<String>>();
    (count, sample)
}

fn desktop_entries(home: &std::path::Path, limit: usize) -> (Vec<Value>, Vec<String>) {
    let mut desktop_files = Vec::new();
    for dir in [
        std::path::PathBuf::from("/usr/share/applications"),
        home.join(".local/share/applications"),
    ] {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("desktop") {
                    desktop_files.push(path);
                }
            }
        }
    }
    desktop_files.sort();
    desktop_files.truncate(limit);

    let mut launchers = Vec::new();
    let mut commands = std::collections::BTreeSet::new();
    for file in desktop_files {
        if let Some((name, exec, command_token)) = parse_desktop_file(&file) {
            launchers.push(json!({
                "name": name,
                "exec": exec,
                "desktop_file": file.to_string_lossy().to_string()
            }));
            commands.insert(command_token);
        }
    }

    (
        launchers.into_iter().take(220).collect(),
        commands.into_iter().take(220).collect(),
    )
}

fn parse_desktop_file(file: &std::path::Path) -> Option<(String, String, String)> {
    let content = std::fs::read_to_string(file).ok()?;
    let mut name = None::<String>;
    let mut exec = None::<String>;
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with("Name=") && name.is_none() {
            name = Some(line.trim_start_matches("Name=").to_string());
        } else if line.starts_with("Exec=") && exec.is_none() {
            exec = Some(line.trim_start_matches("Exec=").to_string());
        }
        if name.is_some() && exec.is_some() {
            break;
        }
    }
    let name = name?;
    let exec = exec?;
    let cmd = exec
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .trim_matches('"')
        .replace("%u", "")
        .replace("%U", "")
        .replace("%f", "")
        .replace("%F", "");
    if cmd.is_empty() {
        return None;
    }
    Some((name, exec, cmd))
}

fn shell_history_summary(home: &std::path::Path, max_lines: usize) -> (Vec<Value>, Vec<String>) {
    let files = [
        home.join(".bash_history"),
        home.join(".zsh_history"),
        home.join(".local/share/fish/fish_history"),
    ];
    let mut all = Vec::new();

    for file in files {
        if !file.exists() {
            continue;
        }
        let content = std::fs::read_to_string(&file).unwrap_or_default();
        for raw in content.lines().take(max_lines) {
            let line = normalize_history_line(raw);
            if !line.is_empty() {
                all.push(line);
            }
        }
    }

    let recent = all.iter().rev().take(30).rev().cloned().collect::<Vec<_>>();
    let mut freq = std::collections::HashMap::<String, usize>::new();
    for line in &all {
        let cmd = line.split_whitespace().next().unwrap_or("").to_string();
        if !cmd.is_empty() {
            *freq.entry(cmd).or_insert(0) += 1;
        }
    }
    let mut items = freq.into_iter().collect::<Vec<(String, usize)>>();
    items.sort_by(|a, b| b.1.cmp(&a.1));
    let top = items
        .into_iter()
        .take(30)
        .map(|(cmd, count)| json!({"cmd": cmd, "count": count}))
        .collect::<Vec<Value>>();

    (top, recent)
}

fn normalize_history_line(line: &str) -> String {
    let trimmed = line.trim();
    if trimmed.starts_with(": ") && trimmed.contains(';') {
        if let Some((_, cmd)) = trimmed.split_once(';') {
            return cmd.trim().to_string();
        }
    }
    if let Some(cmd) = trimmed.strip_prefix("- cmd: ") {
        return cmd.trim().to_string();
    }
    trimmed.to_string()
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

#[derive(Debug, Clone, Copy)]
struct ExecutionBudget {
    max_iterations: usize,
}

fn classify_supervised_task_class(input: &str) -> SupervisedTaskClass {
    let lower = input.to_lowercase();
    let investigation_tokens = [
        "diagnose",
        "why",
        "analyze",
        "investigate",
        "debug",
        "compare",
        "inspect",
        "check",
        "status",
        "trace",
        "benchmark",
    ];
    if investigation_tokens
        .iter()
        .any(|token| lower.contains(token))
    {
        return SupervisedTaskClass::Investigation;
    }

    let action_tokens = [
        "open",
        "create",
        "delete",
        "remove",
        "move",
        "copy",
        "write",
        "edit",
        "run",
        "build",
        "install",
        "switch",
        "focus",
        "close",
        "search",
        "reply",
        "send",
        "play",
        "pause",
        "lockscreen",
        "wallpaper",
        "volume",
    ];
    if action_tokens.iter().any(|token| lower.contains(token)) {
        return SupervisedTaskClass::Action;
    }

    SupervisedTaskClass::Question
}

fn execution_budget_for_class(class: &SupervisedTaskClass) -> ExecutionBudget {
    match class {
        SupervisedTaskClass::Question => ExecutionBudget { max_iterations: 8 },
        SupervisedTaskClass::Action => ExecutionBudget { max_iterations: 16 },
        SupervisedTaskClass::Investigation => ExecutionBudget { max_iterations: 24 },
    }
}

fn next_supervised_task_id(state: &mut AgentOsState) -> String {
    let id = format!("sup-{}", state.supervisor.next_id);
    state.supervisor.next_id += 1;
    id
}

fn start_supervised_task(
    state: &mut AgentOsState,
    prompt: String,
    class: SupervisedTaskClass,
) -> String {
    let now = chrono::Utc::now().timestamp();
    let id = next_supervised_task_id(state);
    state.supervisor.tasks.push(SupervisedTask {
        id: id.clone(),
        prompt,
        class,
        status: SupervisedTaskStatus::Running,
        created_at: now,
        updated_at: now,
        error: None,
    });
    id
}

fn enqueue_supervised_task(
    state: &mut AgentOsState,
    prompt: String,
    class: SupervisedTaskClass,
) -> String {
    let now = chrono::Utc::now().timestamp();
    let id = next_supervised_task_id(state);
    state.supervisor.tasks.push(SupervisedTask {
        id: id.clone(),
        prompt,
        class,
        status: SupervisedTaskStatus::Queued,
        created_at: now,
        updated_at: now,
        error: None,
    });
    id
}

fn start_next_queued_supervised_task(state: &mut AgentOsState) -> Option<SupervisedTask> {
    let now = chrono::Utc::now().timestamp();
    for task in state.supervisor.tasks.iter_mut() {
        if task.status == SupervisedTaskStatus::Queued {
            task.status = SupervisedTaskStatus::Running;
            task.updated_at = now;
            return Some(task.clone());
        }
    }
    None
}

fn mark_supervised_task_completed(state: &mut AgentOsState, task_id: &str) {
    let now = chrono::Utc::now().timestamp();
    if let Some(task) = state
        .supervisor
        .tasks
        .iter_mut()
        .find(|task| task.id == task_id)
    {
        task.status = SupervisedTaskStatus::Completed;
        task.updated_at = now;
        task.error = None;
    }
}

fn mark_supervised_task_failed(state: &mut AgentOsState, task_id: &str, error: String) {
    let now = chrono::Utc::now().timestamp();
    if let Some(task) = state
        .supervisor
        .tasks
        .iter_mut()
        .find(|task| task.id == task_id)
    {
        task.status = SupervisedTaskStatus::Failed;
        task.updated_at = now;
        task.error = Some(error);
    }
}

fn cancel_queued_supervised_tasks(state: &mut AgentOsState) -> usize {
    let now = chrono::Utc::now().timestamp();
    let mut changed = 0usize;
    for task in state.supervisor.tasks.iter_mut() {
        if task.status == SupervisedTaskStatus::Queued {
            task.status = SupervisedTaskStatus::Cancelled;
            task.updated_at = now;
            changed += 1;
        }
    }
    changed
}

fn print_supervisor_queue(state: &AgentOsState) {
    println!("\n🧰 Supervisor Queue");
    println!(
        "  auto-run: {}",
        if state.supervisor.auto_run {
            "on"
        } else {
            "off"
        }
    );
    if state.supervisor.tasks.is_empty() {
        println!("  empty");
        println!();
        return;
    }

    let mut tasks = state.supervisor.tasks.clone();
    tasks.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    for task in tasks.iter().take(20) {
        println!(
            "  {:<8} {:<13} {:<14} {}",
            truncate_for_table(&task.id, 8),
            format!("{:?}", task.status).to_lowercase(),
            task.class.as_str(),
            truncate_for_table(&task.prompt, 52)
        );
    }
    if tasks.len() > 20 {
        println!("  ... and {} more", tasks.len() - 20);
    }
    println!();
}

fn requires_desktop_exclusive(input: &str) -> bool {
    let lower = input.to_lowercase();
    [
        "open",
        "click",
        "type",
        "press",
        "workspace",
        "switch",
        "focus",
        "telegram",
        "gmail",
        "browser",
        "screen",
        "ocr",
        "wallpaper",
        "music",
        "volume",
        "lockscreen",
    ]
    .iter()
    .any(|token| lower.contains(token))
}

fn has_running_background_conflict(
    input: &str,
    running_tasks: &[hypr_claw_tasks::TaskInfo],
) -> bool {
    if !requires_desktop_exclusive(input) {
        return false;
    }
    !running_tasks.is_empty()
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

fn augment_system_prompt_for_turn(
    base_prompt: &str,
    profile: &Value,
    allowed_tools: &HashSet<String>,
) -> String {
    let distro = profile
        .pointer("/platform/distro_name")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let kernel = profile
        .pointer("/platform/kernel")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let workspace = profile
        .pointer("/desktop/active_workspace")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let list_from_profile = |path: &str| -> String {
        profile
            .pointer(path)
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(str::to_string)
                    .collect::<Vec<String>>()
                    .join(", ")
            })
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "none".to_string())
    };

    let command_available = |name: &str| -> bool {
        profile
            .pointer(&format!("/commands/{name}"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    };

    let launcher_hints = profile
        .pointer("/deep_scan/desktop_apps/launcher_commands_sample")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(str::to_string)
                .take(40)
                .collect::<Vec<String>>()
                .join(", ")
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "none".to_string());

    let vscode_hint = ["code", "codium", "code-oss", "vscodium", "code-insiders"]
        .iter()
        .find(|name| command_available(name))
        .map(|s| (*s).to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let project_hints = profile
        .pointer("/deep_scan/home_inventory/project_roots")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .take(20)
                .collect::<Vec<&str>>()
                .join(", ")
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "none".to_string());

    let mut tools = allowed_tools.iter().cloned().collect::<Vec<String>>();
    tools.sort();

    format!(
        "{}\n\nRuntime context:\n- distro: {}\n- kernel: {}\n- active_workspace: {}\n- wallpaper_backends: {}\n- screenshot_backends: {}\n- input_backends: {}\n- vscode_hint: {}\n- launcher_commands: {}\n- known_projects: {}\n- known_downloads_dir: {}\n- allowed_tools_now: {}\n\nExecution policy:\n1) Perform actions using tools, not explanation.\n2) If a tool fails, choose an alternative backend/tool and continue.\n3) Use runtime hints for app launch commands (launcher_commands/vscode_hint).\n4) End with concise result and what changed.\n5) Only stop as blocked after trying alternatives available in runtime context.",
        base_prompt,
        distro,
        kernel,
        workspace,
        list_from_profile("/capabilities/wallpaper_backends"),
        list_from_profile("/capabilities/screenshot_backends"),
        list_from_profile("/capabilities/input_backends"),
        vscode_hint,
        launcher_hints,
        project_hints,
        profile
            .pointer("/paths/downloads")
            .and_then(|v| v.as_str())
            .unwrap_or(""),
        tools.join(", ")
    )
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
                "code", "compile", "build", "run", "execute", "refactor", "debug", "python",
                "rust", "script", "file", "folder", "project",
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
            "communication_assistant",
            ["telegram", "whatsapp", "discord", "slack", "message"].as_slice(),
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

    None
}

// Adapter for ToolDispatcher
struct RuntimeDispatcherAdapter {
    inner: Arc<hypr_claw_tools::ToolDispatcherImpl>,
    action_feed: Arc<Mutex<Vec<String>>>,
}

impl RuntimeDispatcherAdapter {
    fn new(
        inner: Arc<hypr_claw_tools::ToolDispatcherImpl>,
        action_feed: Arc<Mutex<Vec<String>>>,
    ) -> Self {
        Self { inner, action_feed }
    }

    fn push_action(&self, line: String) {
        if let Ok(mut feed) = self.action_feed.lock() {
            feed.push(line);
            if feed.len() > 256 {
                let drop_count = feed.len() - 256;
                feed.drain(0..drop_count);
            }
        }
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
        let normalized_tool_name = normalize_runtime_tool_name(tool_name);
        let started = std::time::Instant::now();
        let line = format!(
            "→ action: {} {}",
            normalized_tool_name,
            truncate_for_table(&input.to_string(), 96)
        );
        self.push_action(line.clone());
        println!("{line}");
        if normalized_tool_name != tool_name {
            let alias = format!(
                "  info: remapped non-standard tool '{}' -> '{}'",
                tool_name, normalized_tool_name
            );
            self.push_action(alias.clone());
            println!("{alias}");
        }
        let result = self
            .inner
            .dispatch(
                session_key.to_string(),
                normalized_tool_name.clone(),
                input.clone(),
            )
            .await;

        match result {
            Ok(tool_result) => {
                if tool_result.success {
                    let line = format!(
                        "✓ action: {} completed in {}ms",
                        normalized_tool_name,
                        started.elapsed().as_millis()
                    );
                    self.push_action(line.clone());
                    println!("{line}");
                    Ok(tool_result.output.unwrap_or(serde_json::json!({})))
                } else {
                    let detail = tool_result
                        .error
                        .clone()
                        .unwrap_or_else(|| "Unknown error".to_string());
                    let line = format!(
                        "✗ action: {} failed in {}ms - {}",
                        normalized_tool_name,
                        started.elapsed().as_millis(),
                        truncate_for_table(&detail, 84)
                    );
                    self.push_action(line.clone());
                    println!("{line}");
                    Err(hypr_claw_runtime::RuntimeError::ToolError(detail))
                }
            }
            Err(e) => {
                let detail = e.to_string();
                let line = format!(
                    "✗ action: {} dispatcher error in {}ms - {}",
                    normalized_tool_name,
                    started.elapsed().as_millis(),
                    truncate_for_table(&detail, 84)
                );
                self.push_action(line.clone());
                println!("{line}");
                Err(hypr_claw_runtime::RuntimeError::ToolError(detail))
            }
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
