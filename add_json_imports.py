#!/usr/bin/env python3
import re

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
        
        # Check if serde_json::json is already imported
        if 'use serde_json::json;' in content or 'serde_json::json!' not in content:
            print(f"- Skipped {filepath} (no json! macro or already imported)")
            continue
        
        # Find the last use statement
        use_pattern = r'(use [^;]+;)\n'
        matches = list(re.finditer(use_pattern, content))
        
        if matches:
            last_use = matches[-1]
            insert_pos = last_use.end()
            new_content = content[:insert_pos] + 'use serde_json::json;\n' + content[insert_pos:]
            
            with open(filepath, 'w') as f:
                f.write(new_content)
            print(f"✓ Added json import to {filepath}")
        else:
            print(f"✗ Could not find use statements in {filepath}")
    except Exception as e:
        print(f"✗ Error processing {filepath}: {e}")
