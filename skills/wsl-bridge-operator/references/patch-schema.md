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

Targeting rules:

- For existing Proxy / Hosts objects, use stable `id` values from `wsl-bridge://state/proxy` or `wsl-bridge://state/hosts`.
- Use `listenerRef` / `routeRef` / `upstreamRef` / `groupRef` only for objects created earlier in the same patch.
- Do not use `create` to modify an existing object. Use `update`.
- Do not use `create` with an already occupied listener port. That will correctly fail as a port conflict.

Proxy examples:

Rename an existing listener:

```json
{
  "version": "phase3.ai-patch.v1",
  "reason": "Rename existing listener",
  "proxy": {
    "listeners": {
      "update": [
        {
          "id": "listener-id-from-state-proxy",
          "name": "Renamed Listener"
        }
      ]
    }
  }
}
```

Delete an existing route:

```json
{
  "version": "phase3.ai-patch.v1",
  "reason": "Delete old route",
  "proxy": {
    "routes": {
      "delete": [
        {
          "id": "route-id-from-state-proxy"
        }
      ]
    }
  }
}
```

Create a listener, route, and upstream in one patch:

```json
{
  "version": "phase3.ai-patch.v1",
  "reason": "Create a new proxy chain",
  "proxy": {
    "listeners": {
      "create": [
        {
          "clientId": "listener-1",
          "name": "Demo Listener",
          "bindAddress": "127.0.0.1",
          "port": 18081,
          "protocol": "http"
        }
      ]
    },
    "routes": {
      "create": [
        {
          "clientId": "route-1",
          "listenerRef": "listener-1",
          "serverNames": ["demo.local"],
          "pathPrefix": "/",
          "isDefault": true
        }
      ]
    },
    "upstreams": {
      "create": [
        {
          "clientId": "upstream-1",
          "routeRef": "route-1",
          "targetType": "wsl",
          "targetRef": "Ubuntu",
          "targetPort": 3000,
          "protocol": "http"
        }
      ]
    }
  }
}
```
