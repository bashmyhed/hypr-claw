use async_trait::async_trait;
use serde_json::Value;

/// Permission decision result
#[derive(Debug, Clone)]
pub enum PermissionDecision {
    Allow,
    Deny(String),
    RequireApproval(String),
}

/// Permission request
#[derive(Debug, Clone)]
pub struct PermissionRequest {
    pub session_key: String,
    pub tool_name: String,
    pub input: Value,
    pub timestamp: String,
}

/// Permission engine trait
#[async_trait]
pub trait PermissionEngine: Send + Sync {
    async fn check(&self, request: PermissionRequest) -> PermissionDecision;
}

/// Audit logger trait
#[async_trait]
pub trait AuditLogger: Send + Sync {
    async fn log(&self, entry: Value);
}
