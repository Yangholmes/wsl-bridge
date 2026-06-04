# Concepts

## Proxy

Proxy manages reverse-proxy style traffic. It is composed of Listener, Route, and Upstream objects.

- Listener accepts HTTP or HTTPS traffic on a host and port.
- Route matches server names and path prefixes.
- Upstream defines the target service, such as WSL, Hyper-V, or a static host.

## Hosts

Hosts manages multiple structured hosts groups. Only one group is active and written to the system hosts file.

## Rules

Rules is legacy after Phase3. It should only be used for `udp_fwd` and `socks5_proxy` creation. Existing `tcp_fwd` and `http_proxy` rules should migrate to Proxy.

## Traffic

Traffic monitor aggregates legacy Rules and Proxy upstream traffic over the same time axis.
