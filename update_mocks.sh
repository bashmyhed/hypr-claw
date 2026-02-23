#!/bin/bash

# Script to add get_tool_schemas to all ToolRegistry implementations

FILES=(
    "hypr-claw-app/src/commands/run.rs"
    "hypr-claw-app/tests/session_persistence_test.rs"
    "hypr-claw-runtime/src/agent_loop.rs"
    "hypr-claw-runtime/src/runtime_controller.rs"
    "hypr-claw-runtime/tests/stress_test.rs"
    "hypr-claw-runtime/tests/integration_runtime.rs"
    "hypr-claw-runtime/tests/failure_simulation.rs"
    "hypr-claw-runtime/tests/test_production_hardening.rs"
    "hypr-claw-runtime/tests/lock_permit_safety.rs"
    "hypr-claw-runtime/tests/failure_scenarios.rs"
    "hypr-claw-runtime/tests/startup_integrity.rs"
)

for file in "${FILES[@]}"; do
    if [ -f "$file" ]; then
        echo "Processing $file"
    fi
done
