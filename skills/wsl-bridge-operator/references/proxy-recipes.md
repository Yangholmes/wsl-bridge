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

## Update an Existing Proxy Object

1. Read `wsl-bridge://state/proxy`.
2. Find the stable `id` of the existing Listener, Route, or Upstream.
3. Build a ConfigPatch `update` operation with that `id`.
4. Keep unchanged fields omitted. Do not convert an update into a create.
5. Dry-run before apply.

## Delete an Existing Proxy Object

1. Read `wsl-bridge://state/proxy`.
2. Find the stable `id` of the Listener, Route, or Upstream to remove.
3. Build a ConfigPatch `delete` operation with that `id`.
4. Expect cascade warnings when deleting a Listener or Route.
5. Dry-run before apply.
