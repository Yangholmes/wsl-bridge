# Proxy Recipes

## Expose a WSL WebSocket Service

1. Inspect `wsl-bridge://state/proxy`.
2. Check whether the desired listener port is already used.
3. Create a Listener with HTTP protocol if the public endpoint is `ws://`.
4. Create a WSL Upstream with `protocol: "ws"`, target distro, and target port.
5. Create a Route with the expected path prefix.
6. Dry-run the ConfigPatch.
7. Apply only after the user accepts exposure warnings.
8. Run connectivity tests for listener, route, and upstream.

## HTTPS

HTTPS requires certificate configuration. Prefer dry-run and validation before enabling an HTTPS Listener.
