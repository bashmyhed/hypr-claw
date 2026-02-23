//! Process management - spawn, kill, list processes

use super::{OsError, OsResult};
use tokio::process::Command;
use tokio::task;
use sysinfo::System;

/// Spawn a process
pub async fn spawn(command: &str, args: &[&str]) -> OsResult<u32> {
    let child = Command::new(command)
        .args(args)
        .spawn()?;
    
    Ok(child.id().ok_or_else(|| OsError::OperationFailed("Failed to get process ID".to_string()))?)
}

/// Kill a process by PID
pub async fn kill(pid: u32) -> OsResult<()> {
    let output = Command::new("kill")
        .args(&["-9", &pid.to_string()])
        .output()
        .await?;
    
    if !output.status.success() {
        return Err(OsError::OperationFailed(
            String::from_utf8_lossy(&output.stderr).to_string()
        ));
    }
    
    Ok(())
}

/// List running processes
pub async fn list() -> OsResult<Vec<ProcessInfo>> {
    task::spawn_blocking(|| {
        let mut system = System::new_all();
        system.refresh_all();

        let processes: Vec<ProcessInfo> = system
            .processes()
            .iter()
            .map(|(pid, process)| ProcessInfo {
                pid: pid.as_u32(),
                name: process.name().to_string(),
                cpu_usage: process.cpu_usage(),
                memory: process.memory(),
            })
            .collect();

        Ok(processes)
    })
    .await
    .map_err(|e| OsError::OperationFailed(e.to_string()))?
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub cpu_usage: f32,
    pub memory: u64,
}
