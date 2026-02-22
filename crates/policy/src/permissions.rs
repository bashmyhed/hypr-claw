use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum PermissionTier {
    Read,
    Write,
    Execute,
    SystemCritical,
}

pub struct PermissionEngine {
    blocked_patterns: Vec<String>,
}

impl PermissionEngine {
    pub fn new() -> Self {
        Self {
            blocked_patterns: Self::default_blocked_patterns(),
        }
    }

    fn default_blocked_patterns() -> Vec<String> {
        vec![
            "rm -rf".to_string(),
            "dd if=".to_string(),
            "mkfs".to_string(),
            "format".to_string(),
            "shutdown".to_string(),
            "reboot".to_string(),
            "init 0".to_string(),
            "init 6".to_string(),
            ":(){ :|:& };:".to_string(), // fork bomb
        ]
    }

    pub fn check_permission(&self, command: &str, tier: PermissionTier) -> PermissionResult {
        // Check blocked patterns
        for pattern in &self.blocked_patterns {
            if command.contains(pattern) {
                return PermissionResult::Denied(format!("Blocked pattern: {}", pattern));
            }
        }

        // System critical operations require approval
        if tier == PermissionTier::SystemCritical {
            return PermissionResult::RequiresApproval;
        }

        PermissionResult::Allowed
    }

    pub fn add_blocked_pattern(&mut self, pattern: String) {
        self.blocked_patterns.push(pattern);
    }
}

impl Default for PermissionEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PermissionResult {
    Allowed,
    RequiresApproval,
    Denied(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blocked_patterns() {
        let engine = PermissionEngine::new();

        let result = engine.check_permission("rm -rf /", PermissionTier::Execute);
        assert!(matches!(result, PermissionResult::Denied(_)));
    }

    #[test]
    fn test_system_critical() {
        let engine = PermissionEngine::new();

        let result = engine.check_permission("safe_command", PermissionTier::SystemCritical);
        assert_eq!(result, PermissionResult::RequiresApproval);
    }

    #[test]
    fn test_allowed_command() {
        let engine = PermissionEngine::new();

        let result = engine.check_permission("ls -la", PermissionTier::Read);
        assert_eq!(result, PermissionResult::Allowed);
    }
}
