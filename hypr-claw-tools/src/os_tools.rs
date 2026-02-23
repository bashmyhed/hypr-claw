//! Structured OS capability tool wrappers.

use crate::error::ToolError;
use crate::execution_context::ExecutionContext;
use crate::os_capabilities::{filesystem, hyprland, process, system};
use crate::tools::base::{Tool, ToolResult};
use crate::traits::PermissionTier;
use async_trait::async_trait;
use serde_json::{json, Value};

fn required_str<'a>(input: &'a Value, field: &str) -> Result<&'a str, ToolError> {
    input[field]
        .as_str()
        .ok_or_else(|| ToolError::ValidationError(format!("Missing or invalid '{field}'")))
}

fn required_u32(input: &Value, field: &str) -> Result<u32, ToolError> {
    let n = input[field]
        .as_u64()
        .ok_or_else(|| ToolError::ValidationError(format!("Missing or invalid '{field}'")))?;
    u32::try_from(n)
        .map_err(|_| ToolError::ValidationError(format!("'{field}' out of range")))
}

pub struct FsCreateDirTool;
pub struct FsDeleteTool;
pub struct FsMoveTool;
pub struct FsCopyTool;
pub struct FsReadTool;
pub struct FsWriteTool;
pub struct FsListTool;

#[async_trait]
impl Tool for FsCreateDirTool {
    fn name(&self) -> &'static str { "fs.create_dir" }
    fn description(&self) -> &'static str { "Create a directory at path" }
    fn permission_tier(&self) -> PermissionTier { PermissionTier::Write }
    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": { "path": {"type": "string"} },
            "required": ["path"],
            "additionalProperties": false
        })
    }
    async fn execute(&self, _ctx: ExecutionContext, input: Value) -> Result<ToolResult, ToolError> {
        let path = required_str(&input, "path")?;
        filesystem::create_dir(path).await.map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        Ok(ToolResult { success: true, output: Some(json!({"created": path})), error: None })
    }
}

#[async_trait]
impl Tool for FsDeleteTool {
    fn name(&self) -> &'static str { "fs.delete" }
    fn description(&self) -> &'static str { "Delete a file or directory" }
    fn permission_tier(&self) -> PermissionTier { PermissionTier::Write }
    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": { "path": {"type": "string"} },
            "required": ["path"],
            "additionalProperties": false
        })
    }
    async fn execute(&self, _ctx: ExecutionContext, input: Value) -> Result<ToolResult, ToolError> {
        let path = required_str(&input, "path")?;
        filesystem::delete(path).await.map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        Ok(ToolResult { success: true, output: Some(json!({"deleted": path})), error: None })
    }
}

#[async_trait]
impl Tool for FsMoveTool {
    fn name(&self) -> &'static str { "fs.move" }
    fn description(&self) -> &'static str { "Move/rename a file or directory" }
    fn permission_tier(&self) -> PermissionTier { PermissionTier::Write }
    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "from": {"type": "string"},
                "to": {"type": "string"}
            },
            "required": ["from", "to"],
            "additionalProperties": false
        })
    }
    async fn execute(&self, _ctx: ExecutionContext, input: Value) -> Result<ToolResult, ToolError> {
        let from = required_str(&input, "from")?;
        let to = required_str(&input, "to")?;
        filesystem::move_path(from, to).await.map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        Ok(ToolResult { success: true, output: Some(json!({"from": from, "to": to})), error: None })
    }
}

#[async_trait]
impl Tool for FsCopyTool {
    fn name(&self) -> &'static str { "fs.copy" }
    fn description(&self) -> &'static str { "Copy a file" }
    fn permission_tier(&self) -> PermissionTier { PermissionTier::Write }
    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "from": {"type": "string"},
                "to": {"type": "string"}
            },
            "required": ["from", "to"],
            "additionalProperties": false
        })
    }
    async fn execute(&self, _ctx: ExecutionContext, input: Value) -> Result<ToolResult, ToolError> {
        let from = required_str(&input, "from")?;
        let to = required_str(&input, "to")?;
        filesystem::copy_file(from, to).await.map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        Ok(ToolResult { success: true, output: Some(json!({"from": from, "to": to})), error: None })
    }
}

#[async_trait]
impl Tool for FsReadTool {
    fn name(&self) -> &'static str { "fs.read" }
    fn description(&self) -> &'static str { "Read file contents" }
    fn permission_tier(&self) -> PermissionTier { PermissionTier::Read }
    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": { "path": {"type": "string"} },
            "required": ["path"],
            "additionalProperties": false
        })
    }
    async fn execute(&self, _ctx: ExecutionContext, input: Value) -> Result<ToolResult, ToolError> {
        let path = required_str(&input, "path")?;
        let content = filesystem::read(path).await.map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        Ok(ToolResult { success: true, output: Some(json!({"path": path, "content": content})), error: None })
    }
}

#[async_trait]
impl Tool for FsWriteTool {
    fn name(&self) -> &'static str { "fs.write" }
    fn description(&self) -> &'static str { "Write file contents" }
    fn permission_tier(&self) -> PermissionTier { PermissionTier::Write }
    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {"type": "string"},
                "content": {"type": "string"}
            },
            "required": ["path", "content"],
            "additionalProperties": false
        })
    }
    async fn execute(&self, _ctx: ExecutionContext, input: Value) -> Result<ToolResult, ToolError> {
        let path = required_str(&input, "path")?;
        let content = required_str(&input, "content")?;
        filesystem::write(path, content).await.map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        Ok(ToolResult { success: true, output: Some(json!({"written": path})), error: None })
    }
}

#[async_trait]
impl Tool for FsListTool {
    fn name(&self) -> &'static str { "fs.list" }
    fn description(&self) -> &'static str { "List directory contents" }
    fn permission_tier(&self) -> PermissionTier { PermissionTier::Read }
    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": { "path": {"type": "string"} },
            "required": ["path"],
            "additionalProperties": false
        })
    }
    async fn execute(&self, _ctx: ExecutionContext, input: Value) -> Result<ToolResult, ToolError> {
        let path = required_str(&input, "path")?;
        let entries = filesystem::list(path).await.map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        let entries: Vec<String> = entries
            .into_iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();
        Ok(ToolResult { success: true, output: Some(json!({"path": path, "entries": entries})), error: None })
    }
}

pub struct ProcSpawnTool;
pub struct ProcKillTool;
pub struct ProcListTool;

#[async_trait]
impl Tool for ProcSpawnTool {
    fn name(&self) -> &'static str { "proc.spawn" }
    fn description(&self) -> &'static str { "Spawn a process" }
    fn permission_tier(&self) -> PermissionTier { PermissionTier::Execute }
    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": {"type": "string"},
                "args": {"type": "array", "items": {"type": "string"}}
            },
            "required": ["command"],
            "additionalProperties": false
        })
    }
    async fn execute(&self, _ctx: ExecutionContext, input: Value) -> Result<ToolResult, ToolError> {
        let command = required_str(&input, "command")?;
        let args_vec: Vec<String> = input["args"]
            .as_array()
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
            .unwrap_or_default();
        let arg_refs: Vec<&str> = args_vec.iter().map(String::as_str).collect();
        let pid = process::spawn(command, &arg_refs).await.map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        Ok(ToolResult { success: true, output: Some(json!({"pid": pid})), error: None })
    }
}

#[async_trait]
impl Tool for ProcKillTool {
    fn name(&self) -> &'static str { "proc.kill" }
    fn description(&self) -> &'static str { "Kill a process by PID" }
    fn permission_tier(&self) -> PermissionTier { PermissionTier::Execute }
    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": { "pid": {"type": "number"} },
            "required": ["pid"],
            "additionalProperties": false
        })
    }
    async fn execute(&self, _ctx: ExecutionContext, input: Value) -> Result<ToolResult, ToolError> {
        let pid = required_u32(&input, "pid")?;
        process::kill(pid).await.map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        Ok(ToolResult { success: true, output: Some(json!({"killed": pid})), error: None })
    }
}

#[async_trait]
impl Tool for ProcListTool {
    fn name(&self) -> &'static str { "proc.list" }
    fn description(&self) -> &'static str { "List running processes" }
    fn permission_tier(&self) -> PermissionTier { PermissionTier::Read }
    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "limit": {"type": "number"}
            },
            "additionalProperties": false
        })
    }
    async fn execute(&self, _ctx: ExecutionContext, input: Value) -> Result<ToolResult, ToolError> {
        let limit = input["limit"].as_u64().unwrap_or(25) as usize;
        let mut procs = process::list().await.map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        procs.truncate(limit);
        Ok(ToolResult { success: true, output: Some(json!({"processes": procs})), error: None })
    }
}

pub struct HyprWorkspaceSwitchTool;
pub struct HyprWorkspaceMoveWindowTool;
pub struct HyprWindowFocusTool;
pub struct HyprWindowCloseTool;
pub struct HyprWindowMoveTool;
pub struct HyprExecTool;

#[async_trait]
impl Tool for HyprWorkspaceSwitchTool {
    fn name(&self) -> &'static str { "hypr.workspace.switch" }
    fn description(&self) -> &'static str { "Switch active workspace in Hyprland" }
    fn permission_tier(&self) -> PermissionTier { PermissionTier::Execute }
    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": { "workspace_id": {"type": "number"} },
            "required": ["workspace_id"],
            "additionalProperties": false
        })
    }
    async fn execute(&self, _ctx: ExecutionContext, input: Value) -> Result<ToolResult, ToolError> {
        let workspace_id = required_u32(&input, "workspace_id")?;
        hyprland::workspace_switch(workspace_id).await.map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        Ok(ToolResult { success: true, output: Some(json!({"workspace": workspace_id})), error: None })
    }
}

#[async_trait]
impl Tool for HyprWorkspaceMoveWindowTool {
    fn name(&self) -> &'static str { "hypr.workspace.move_window" }
    fn description(&self) -> &'static str { "Move window to another workspace" }
    fn permission_tier(&self) -> PermissionTier { PermissionTier::Execute }
    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "window_id": {"type": "string"},
                "workspace_id": {"type": "number"}
            },
            "required": ["window_id", "workspace_id"],
            "additionalProperties": false
        })
    }
    async fn execute(&self, _ctx: ExecutionContext, input: Value) -> Result<ToolResult, ToolError> {
        let window_id = required_str(&input, "window_id")?;
        let workspace_id = required_u32(&input, "workspace_id")?;
        hyprland::workspace_move_window(window_id, workspace_id)
            .await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        Ok(ToolResult { success: true, output: Some(json!({"window_id": window_id, "workspace_id": workspace_id})), error: None })
    }
}

#[async_trait]
impl Tool for HyprWindowFocusTool {
    fn name(&self) -> &'static str { "hypr.window.focus" }
    fn description(&self) -> &'static str { "Focus a window" }
    fn permission_tier(&self) -> PermissionTier { PermissionTier::Execute }
    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": { "window_id": {"type": "string"} },
            "required": ["window_id"],
            "additionalProperties": false
        })
    }
    async fn execute(&self, _ctx: ExecutionContext, input: Value) -> Result<ToolResult, ToolError> {
        let window_id = required_str(&input, "window_id")?;
        hyprland::window_focus(window_id).await.map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        Ok(ToolResult { success: true, output: Some(json!({"focused": window_id})), error: None })
    }
}

#[async_trait]
impl Tool for HyprWindowCloseTool {
    fn name(&self) -> &'static str { "hypr.window.close" }
    fn description(&self) -> &'static str { "Close a window" }
    fn permission_tier(&self) -> PermissionTier { PermissionTier::Execute }
    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": { "window_id": {"type": "string"} },
            "required": ["window_id"],
            "additionalProperties": false
        })
    }
    async fn execute(&self, _ctx: ExecutionContext, input: Value) -> Result<ToolResult, ToolError> {
        let window_id = required_str(&input, "window_id")?;
        hyprland::window_close(window_id).await.map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        Ok(ToolResult { success: true, output: Some(json!({"closed": window_id})), error: None })
    }
}

#[async_trait]
impl Tool for HyprWindowMoveTool {
    fn name(&self) -> &'static str { "hypr.window.move" }
    fn description(&self) -> &'static str { "Move a window to workspace" }
    fn permission_tier(&self) -> PermissionTier { PermissionTier::Execute }
    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "window_id": {"type": "string"},
                "workspace_id": {"type": "number"}
            },
            "required": ["window_id", "workspace_id"],
            "additionalProperties": false
        })
    }
    async fn execute(&self, _ctx: ExecutionContext, input: Value) -> Result<ToolResult, ToolError> {
        let window_id = required_str(&input, "window_id")?;
        let workspace_id = required_u32(&input, "workspace_id")?;
        hyprland::window_move(window_id, workspace_id).await.map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        Ok(ToolResult { success: true, output: Some(json!({"window_id": window_id, "workspace_id": workspace_id})), error: None })
    }
}

#[async_trait]
impl Tool for HyprExecTool {
    fn name(&self) -> &'static str { "hypr.exec" }
    fn description(&self) -> &'static str { "Execute a command via Hyprland dispatcher" }
    fn permission_tier(&self) -> PermissionTier { PermissionTier::Execute }
    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": { "command": {"type": "string"} },
            "required": ["command"],
            "additionalProperties": false
        })
    }
    async fn execute(&self, _ctx: ExecutionContext, input: Value) -> Result<ToolResult, ToolError> {
        let command = required_str(&input, "command")?;
        hyprland::exec(command).await.map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        Ok(ToolResult { success: true, output: Some(json!({"executed": command})), error: None })
    }
}

pub struct WallpaperSetTool;
pub struct SystemShutdownTool;
pub struct SystemRebootTool;
pub struct SystemBatteryTool;
pub struct SystemMemoryTool;

#[async_trait]
impl Tool for WallpaperSetTool {
    fn name(&self) -> &'static str { "wallpaper.set" }
    fn description(&self) -> &'static str { "Set desktop wallpaper" }
    fn permission_tier(&self) -> PermissionTier { PermissionTier::Write }
    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": { "image_path": {"type": "string"} },
            "required": ["image_path"],
            "additionalProperties": false
        })
    }
    async fn execute(&self, _ctx: ExecutionContext, input: Value) -> Result<ToolResult, ToolError> {
        let image_path = required_str(&input, "image_path")?;
        system::wallpaper_set(image_path).await.map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        Ok(ToolResult { success: true, output: Some(json!({"wallpaper": image_path})), error: None })
    }
}

#[async_trait]
impl Tool for SystemShutdownTool {
    fn name(&self) -> &'static str { "system.shutdown" }
    fn description(&self) -> &'static str { "Shutdown the system" }
    fn permission_tier(&self) -> PermissionTier { PermissionTier::SystemCritical }
    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        })
    }
    async fn execute(&self, _ctx: ExecutionContext, _input: Value) -> Result<ToolResult, ToolError> {
        system::shutdown().await.map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        Ok(ToolResult { success: true, output: Some(json!({"shutdown": true})), error: None })
    }
}

#[async_trait]
impl Tool for SystemRebootTool {
    fn name(&self) -> &'static str { "system.reboot" }
    fn description(&self) -> &'static str { "Reboot the system" }
    fn permission_tier(&self) -> PermissionTier { PermissionTier::SystemCritical }
    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        })
    }
    async fn execute(&self, _ctx: ExecutionContext, _input: Value) -> Result<ToolResult, ToolError> {
        system::reboot().await.map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        Ok(ToolResult { success: true, output: Some(json!({"reboot": true})), error: None })
    }
}

#[async_trait]
impl Tool for SystemBatteryTool {
    fn name(&self) -> &'static str { "system.battery" }
    fn description(&self) -> &'static str { "Get battery percentage" }
    fn permission_tier(&self) -> PermissionTier { PermissionTier::Read }
    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        })
    }
    async fn execute(&self, _ctx: ExecutionContext, _input: Value) -> Result<ToolResult, ToolError> {
        let percent = system::battery_level().await.map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        Ok(ToolResult { success: true, output: Some(json!({"battery_percent": percent})), error: None })
    }
}

#[async_trait]
impl Tool for SystemMemoryTool {
    fn name(&self) -> &'static str { "system.memory" }
    fn description(&self) -> &'static str { "Get memory usage information" }
    fn permission_tier(&self) -> PermissionTier { PermissionTier::Read }
    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        })
    }
    async fn execute(&self, _ctx: ExecutionContext, _input: Value) -> Result<ToolResult, ToolError> {
        let info = system::memory_info().await.map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        Ok(ToolResult { success: true, output: Some(json!({"memory": info})), error: None })
    }
}
