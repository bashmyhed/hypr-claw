pub mod error;
pub mod execution_context;
pub mod sandbox;
pub mod tools;
pub mod registry;
pub mod dispatcher;
pub mod permission_adapter;
pub mod audit_adapter;
pub mod traits;
pub mod os_capabilities;
pub mod os_tools;

pub use dispatcher::ToolDispatcherImpl;
pub use registry::ToolRegistryImpl;
pub use error::ToolError;
pub use tools::{Tool, ToolResult};
pub use execution_context::ExecutionContext;
pub use traits::{PermissionEngine, AuditLogger, PermissionDecision, PermissionRequest, PermissionTier};
