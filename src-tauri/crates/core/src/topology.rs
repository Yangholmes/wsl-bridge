use std::net::IpAddr;

use wsl_bridge_shared::AdapterInfo;

pub fn list_adapters() -> Vec<AdapterInfo> {
    #[cfg(windows)]
    {
        let mut adapters = Vec::new();
        if let Ok(items) = ipconfig::get_adapters() {
            for item in items {
                let mut ipv4 = Vec::new();
                let mut ipv6 = Vec::new();
                for addr in item.ip_addresses() {
                    match addr {
                        IpAddr::V4(v4) => ipv4.push(v4.to_string()),
                        IpAddr::V6(v6) => ipv6.push(v6.to_string()),
                    }
                }
                adapters.push(AdapterInfo {
                    id: item.adapter_name().to_owned(),
                    name: item.friendly_name().to_owned(),
                    ipv4,
                    ipv6,
                });
            }
        }
        return adapters;
    }

    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

pub fn resolve_nic_ip(nic_id: &str) -> Option<IpAddr> {
    #[cfg(windows)]
    {
        let adapters = ipconfig::get_adapters().ok()?;
        for item in adapters {
            if item.adapter_name() != nic_id && item.friendly_name() != nic_id {
                continue;
            }
            for ip in item.ip_addresses() {
                if ip.is_ipv4() {
                    return Some(*ip);
                }
            }
            for ip in item.ip_addresses() {
                if ip.is_ipv6() {
                    return Some(*ip);
                }
            }
            return None;
        }
        None
    }

    #[cfg(not(windows))]
    {
        let _ = nic_id;
        None
    }
}
