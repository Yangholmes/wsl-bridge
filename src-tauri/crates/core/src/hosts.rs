use std::env;
use std::fs;
use std::io;
use std::net::IpAddr;
use std::path::{Path, PathBuf};

use wsl_bridge_shared::HostsEntryInput;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedHostsLine {
    pub ip: String,
    pub domain: String,
    pub comment: Option<String>,
}

pub fn parse_hosts_text(text: &str) -> Vec<ParsedHostsLine> {
    let mut items = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let (body, comment) = match trimmed.split_once('#') {
            Some((body, comment)) => (body.trim(), Some(comment.trim().to_owned())),
            None => (trimmed, None),
        };

        if body.is_empty() {
            continue;
        }

        let mut parts = body.split_whitespace();
        let Some(ip) = parts.next() else {
            continue;
        };
        if ip.parse::<IpAddr>().is_err() {
            continue;
        }

        for domain in parts {
            if domain.trim().is_empty() {
                continue;
            }
            items.push(ParsedHostsLine {
                ip: ip.to_owned(),
                domain: domain.trim().to_owned(),
                comment: comment.clone().filter(|value| !value.is_empty()),
            });
        }
    }

    items
}

pub fn render_hosts_text(entries: &[HostsEntryInput]) -> String {
    let mut sorted = entries.to_vec();
    sorted.sort_by_key(|item| item.order_index);

    let mut lines = Vec::new();
    for entry in sorted.into_iter().filter(|item| item.enabled) {
        let mut line = format!("{} {}", entry.ip.trim(), entry.domain.trim());
        if let Some(comment) = entry
            .comment
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            line.push_str(" # ");
            line.push_str(comment);
        }
        lines.push(line);
    }

    if lines.is_empty() {
        String::new()
    } else {
        format!("{}\n", lines.join("\n"))
    }
}

pub fn resolve_system_hosts_path() -> PathBuf {
    if let Ok(explicit) = env::var("WSL_BRIDGE_HOSTS_PATH") {
        return PathBuf::from(explicit);
    }

    #[cfg(windows)]
    {
        if let Ok(windir) = env::var("WINDIR") {
            return PathBuf::from(windir)
                .join("System32")
                .join("drivers")
                .join("etc")
                .join("hosts");
        }
        PathBuf::from(r"C:\Windows\System32\drivers\etc\hosts")
    }

    #[cfg(not(windows))]
    {
        PathBuf::from("/etc/hosts")
    }
}

pub fn read_hosts_file(path: &Path) -> io::Result<Vec<ParsedHostsLine>> {
    let text = fs::read_to_string(path)?;
    Ok(parse_hosts_text(&text))
}

pub fn write_hosts_file(path: &Path, content: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let tmp_path = path.with_extension("tmp");
    fs::write(&tmp_path, content)?;
    if path.exists() {
        let _ = fs::remove_file(path);
    }
    fs::rename(tmp_path, path)
}

#[cfg(test)]
mod tests {
    use wsl_bridge_shared::HostsEntryInput;

    use super::{parse_hosts_text, render_hosts_text};

    #[test]
    fn parse_hosts_text_supports_multi_domain_and_comments() {
        let text = r#"
127.0.0.1 localhost api.local # local aliases
::1 ipv6.local
# comment only
bad-row
"#;

        let items = parse_hosts_text(text);
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].ip, "127.0.0.1");
        assert_eq!(items[0].domain, "localhost");
        assert_eq!(items[0].comment.as_deref(), Some("local aliases"));
        assert_eq!(items[1].domain, "api.local");
        assert_eq!(items[2].ip, "::1");
    }

    #[test]
    fn render_hosts_text_skips_disabled_entries() {
        let text = render_hosts_text(&[
            HostsEntryInput {
                id: None,
                ip: "127.0.0.1".to_owned(),
                domain: "localhost".to_owned(),
                comment: Some("loopback".to_owned()),
                enabled: true,
                order_index: 1,
            },
            HostsEntryInput {
                id: None,
                ip: "127.0.0.1".to_owned(),
                domain: "disabled.local".to_owned(),
                comment: None,
                enabled: false,
                order_index: 2,
            },
        ]);

        assert!(text.contains("127.0.0.1 localhost # loopback"));
        assert!(!text.contains("disabled.local"));
    }
}
