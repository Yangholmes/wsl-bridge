---
name: wsl-bridge-operator
description: Use when operating wsl-bridge through MCP, especially for Proxy, Hosts, Rules legacy migration, traffic diagnostics, config patch dry-run, and validation workflows.
---

# wsl-bridge Operator

Use this skill when the user asks an AI agent to inspect, diagnose, configure, or validate wsl-bridge.

## Required Workflow

1. Read `wsl-bridge://ai-guide` and `wsl-bridge://state/summary` before making assumptions about the current app state.
2. Use MCP resources or `inspect_app` to understand Proxy, Hosts, Rules, Traffic, and runtime status.
3. For any configuration change, build a structured `ConfigPatch`.
4. Call `apply_config_patch` with `mode: "dryRun"` before proposing or applying the change.
5. Explain warnings and conflicts to the user.
6. Apply changes only when the user has authorized the operation and the app policy allows it.
7. Run `validate_config` or `test_connectivity` after changes.
8. If validation fails, inspect recent logs and report the failure stage.

## Safety Rules

- Do not directly edit wsl-bridge database files.
- Do not ask the user to manually modify generated configuration when a ConfigPatch workflow exists.
- Do not apply destructive changes without a dry-run summary.
- Treat system hosts writes, listener `0.0.0.0` exposure, certificate changes, and Agent skill installation as sensitive operations.

## First Resources To Read

```text
wsl-bridge://ai-guide
wsl-bridge://capabilities
wsl-bridge://state/summary
wsl-bridge://schemas/config-patch
```

## References

- `references/concepts.md`
- `references/proxy-recipes.md`
- `references/hosts-recipes.md`
- `references/rules-legacy.md`
- `references/troubleshooting.md`
- `references/patch-schema.md`
- `references/safety.md`
