use async_trait::async_trait;
use hypr_claw_tools::{PermissionEngine as PermissionEngineTrait, PermissionDecision, PermissionRequest};
use crate::infra::permission_engine::PermissionEngine;
use std::collections::HashMap;

#[async_trait]
impl PermissionEngineTrait for PermissionEngine {
    async fn check(&self, request: PermissionRequest) -> PermissionDecision {
        // Convert to infra types
        let input_map: HashMap<String, serde_json::Value> = request.input
            .as_object()
            .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default();

        let infra_request = crate::infra::contracts::PermissionRequest {
            session_key: request.session_key,
            tool_name: request.tool_name,
            input: input_map,
            permission_level: crate::infra::contracts::PermissionLevel::SAFE,
        };

        let decision = self.check(&infra_request);

        match decision {
            crate::infra::contracts::PermissionDecision::ALLOW => PermissionDecision::Allow,
            crate::infra::contracts::PermissionDecision::DENY => {
                PermissionDecision::Deny("Permission denied".to_string())
            }
            crate::infra::contracts::PermissionDecision::REQUIRE_APPROVAL => {
                PermissionDecision::RequireApproval("Approval required".to_string())
            }
        }
    }
}
