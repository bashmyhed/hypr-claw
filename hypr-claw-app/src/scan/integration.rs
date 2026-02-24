use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Notify;

use crate::scan::*;
use crate::scan::parsers::{GitParser, HyprlandParser, ParserRegistry, ShellParser, partition_results};

/// Run integrated system scan with new scan engine
pub async fn run_integrated_scan(
    user_id: &str,
    deep_scan: bool,
) -> Result<Value, Box<dyn std::error::Error>> {
    // Start with basic system profile
    let mut profile = collect_basic_system_profile(user_id).await;

    if deep_scan {
        // Discover home structure
        let user_dirs = UserDirectories::discover();
        let discovered = discover_home_structure(&user_dirs.home);

        println!("\n🏠 Discovered {} directories in home", discovered.len());

        // Build policy interactively
        let policy = ScanPolicy::build_interactively(&discovered)?;

        // Calibrate resources
        let monitor = ResourceMonitor::auto_calibrate();
        monitor.print_calibration();

        // Run scan
        println!("\n🔎 Starting deep scan...");
        let interrupt = Arc::new(Notify::new());

        let mut scan_results = Vec::new();
        for path in &policy.included_paths {
            if !path.exists() {
                continue;
            }
            let result = scan_directory(path, &policy, &monitor, interrupt.clone()).await?;
            scan_results.push(result);
        }

        // Aggregate results
        let total_files: usize = scan_results.iter().map(|r| r.stats.files_scanned).sum();
        let total_dirs: usize = scan_results.iter().map(|r| r.stats.dirs_scanned).sum();
        let total_bytes: u64 = scan_results.iter().map(|r| r.stats.bytes_processed).sum();

        println!("\n✅ Scan complete!");
        println!("  Files: {}", total_files);
        println!("  Directories: {}", total_dirs);
        println!("  Size: {} MB", total_bytes / 1024 / 1024);

        // Build deep scan data with config parsing
        let deep_data = build_deep_scan_data(&scan_results, &user_dirs, &policy);
        if let Some(obj) = profile.as_object_mut() {
            obj.insert("deep_scan".to_string(), deep_data);
        }
    }

    Ok(profile)
}

async fn collect_basic_system_profile(user_id: &str) -> Value {
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

    let home = std::env::var("HOME").unwrap_or_default();

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
            "home": home.clone(),
            "shell": std::env::var("SHELL").unwrap_or_default(),
            "hostname": hostname
        },
        "desktop": {
            "session": std::env::var("XDG_SESSION_TYPE").unwrap_or_default(),
            "desktop_env": std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default(),
            "hyprland_available": hyprland_available,
            "active_workspace": active_workspace,
        },
        "paths": {
            "home": home,
        }
    })
}

fn build_deep_scan_data(
    scan_results: &[ScanResult],
    user_dirs: &UserDirectories,
    policy: &ScanPolicy,
) -> Value {
    let mut config_files = Vec::new();
    let mut script_files = Vec::new();
    let mut source_files = Vec::new();
    let mut project_dirs = Vec::new();

    // Collect files by type
    for result in scan_results {
        for file in &result.scanned_files {
            match &file.file_class {
                FileClass::Config { .. } => {
                    config_files.push(file.path.clone());
                }
                FileClass::Script { language } => {
                    script_files.push(json!({
                        "path": file.path.to_string_lossy(),
                        "language": language,
                        "size": file.size
                    }));
                }
                FileClass::Source { language } => {
                    source_files.push(json!({
                        "path": file.path.to_string_lossy(),
                        "language": language,
                        "size": file.size
                    }));
                }
                _ => {}
            }
        }
    }

    // Find project directories
    let discovered = discover_home_structure(&user_dirs.home);
    for dir in discovered {
        if dir.category == DirectoryCategory::Projects {
            project_dirs.push(dir.path.to_string_lossy().to_string());
        }
    }

    // Parse configs
    println!("\n📝 Parsing config files...");
    let mut registry = ParserRegistry::new();
    registry.register(Box::new(HyprlandParser));
    registry.register(Box::new(ShellParser));
    registry.register(Box::new(GitParser));

    let parse_results = registry.parse_all(&config_files);
    let (parsed, failed) = partition_results(parse_results);

    // Show parse results
    if !parsed.is_empty() {
        println!("  ✅ Parsed: {} configs", parsed.len());
    }
    if !failed.is_empty() {
        println!("  ⚠️  Failed: {} configs", failed.len());
        for (path, error) in failed.iter().take(3) {
            println!("    - {}", error.user_message());
        }
        if failed.len() > 3 {
            println!("    ... and {} more", failed.len() - 3);
        }
    }

    // Serialize parsed configs
    let parsed_configs: Vec<Value> = parsed
        .iter()
        .map(|p| {
            json!({
                "path": p.path.to_string_lossy(),
                "type": format!("{:?}", p.config_type),
                "data": p.data,
                "parse_time_ms": p.parse_time_ms,
            })
        })
        .collect();

    json!({
        "scanned_at": chrono::Utc::now().timestamp(),
        "config_files_found": config_files.len(),
        "script_files": script_files.into_iter().take(50).collect::<Vec<_>>(),
        "source_files": source_files.into_iter().take(100).collect::<Vec<_>>(),
        "project_roots": project_dirs.into_iter().take(50).collect::<Vec<_>>(),
        "parsed_configs": parsed_configs,
        "parse_stats": {
            "total": config_files.len(),
            "parsed": parsed.len(),
            "failed": failed.len(),
        }
    })
}

fn parse_os_release_value(content: &str, key: &str) -> Option<String> {
    for line in content.lines() {
        if let Some(value) = line.strip_prefix(&format!("{}=", key)) {
            return Some(value.trim_matches('"').to_string());
        }
    }
    None
}

async fn command_output(command: &str, args: &[&str]) -> Option<String> {
    let output = tokio::process::Command::new(command)
        .args(args)
        .output()
        .await
        .ok()?;
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    }
}

async fn command_exists(command: &str) -> bool {
    tokio::process::Command::new("which")
        .arg(command)
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}
