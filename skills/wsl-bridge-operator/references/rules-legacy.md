# Rules Legacy

Rules is a legacy module after Phase3.

- New `tcp_fwd` and `http_proxy` rules should not be created in Rules.
- New `udp_fwd` and `socks5_proxy` rules remain valid in Rules.
- Existing `tcp_fwd` and `http_proxy` rules may be migrated to Proxy.
- Migration should be previewed before applying.
