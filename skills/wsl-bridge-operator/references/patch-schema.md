# ConfigPatch

All writes should be represented as a structured ConfigPatch and dry-run before apply.

```json
{
  "version": "phase3.ai-patch.v1",
  "reason": "Describe the user intent",
  "proxy": {},
  "hosts": {},
  "rules": {},
  "settings": {}
}
```

Use client-local IDs for objects created and referenced in the same patch.
