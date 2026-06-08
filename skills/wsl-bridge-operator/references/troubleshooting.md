# Troubleshooting

## Proxy 502 or Empty Response

Check in order:

1. Listener is running.
2. Route matched the request server name and path.
3. Upstream resolved to the expected host and port.
4. Upstream service accepts the browser protocol, including WebSocket upgrades when needed.
5. Recent logs do not show upstream connect or response translation errors.

## WSL Target Not Reachable

Check the distro is running, the service is listening on the expected port, and the app resolved the WSL target correctly.
