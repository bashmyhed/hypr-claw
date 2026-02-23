#!/usr/bin/env python3
import re
import sys

TOOL_SCHEMA_METHOD = '''
        fn get_tool_schemas(&self, _agent_id: &str) -> Vec<serde_json::Value> {
            vec![
                json!({
                    "type": "function",
                    "function": {
                        "name": "echo",
                        "description": "Echo a message",
                        "parameters": {
                            "type": "object",
                            "properties": {
                                "message": {"type": "string"}
                            },
                            "required": ["message"]
                        }
                    }
                })
            ]
        }'''

files = [
    "hypr-claw-app/src/commands/run.rs",
    "hypr-claw-app/tests/session_persistence_test.rs",
    "hypr-claw-runtime/src/runtime_controller.rs",
    "hypr-claw-runtime/tests/stress_test.rs",
    "hypr-claw-runtime/tests/integration_runtime.rs",
    "hypr-claw-runtime/tests/failure_simulation.rs",
    "hypr-claw-runtime/tests/test_production_hardening.rs",
    "hypr-claw-runtime/tests/lock_permit_safety.rs",
    "hypr-claw-runtime/tests/failure_scenarios.rs",
    "hypr-claw-runtime/tests/startup_integrity.rs",
]

for filepath in files:
    try:
        with open(filepath, 'r') as f:
            content = f.read()
        
        # Find pattern: get_active_tools method followed by closing brace
        pattern = r'(fn get_active_tools\(&self[^}]+\})\s*\n(\s*)\}'
        
        def replacer(match):
            method = match.group(1)
            indent = match.group(2)
            return method + '\n' + TOOL_SCHEMA_METHOD + '\n' + indent + '}'
        
        new_content = re.sub(pattern, replacer, content)
        
        if new_content != content:
            with open(filepath, 'w') as f:
                f.write(new_content)
            print(f"✓ Updated {filepath}")
        else:
            print(f"- Skipped {filepath} (no changes needed)")
    except FileNotFoundError:
        print(f"✗ File not found: {filepath}")
    except Exception as e:
        print(f"✗ Error processing {filepath}: {e}")
