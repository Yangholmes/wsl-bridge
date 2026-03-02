use std::process::Command;

use wsl_bridge_shared::{FirewallPolicy, ProxyRule, RuleType};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FirewallMode {
    Disabled,
    BestEffort,
    Enforced,
}

impl FirewallMode {
    pub fn from_env_value(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "enforced" => Self::Enforced,
            "best_effort" | "besteffort" => Self::BestEffort,
            _ => Self::Disabled,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FirewallRuleRuntime {
    pub names: Vec<String>,
}

pub fn apply_firewall(
    mode: FirewallMode,
    rule: &ProxyRule,
    policy: &FirewallPolicy,
) -> Result<FirewallRuleRuntime, String> {
    if mode == FirewallMode::Disabled {
        return Ok(FirewallRuleRuntime { names: Vec::new() });
    }

    #[cfg(not(windows))]
    {
        let _ = (rule, policy);
        return if mode == FirewallMode::Enforced {
            Err("firewall enforcement is only supported on Windows".to_owned())
        } else {
            Ok(FirewallRuleRuntime { names: Vec::new() })
        };
    }

    #[cfg(windows)]
    {
        let protocol = match rule.rule_type {
            RuleType::UdpFwd => "UDP",
            _ => "TCP",
        };
        let direction = match policy.direction.to_ascii_lowercase().as_str() {
            "inbound" => "in",
            "outbound" => "out",
            "in" => "in",
            "out" => "out",
            _ => "in",
        };
        let action = match policy.action.to_ascii_lowercase().as_str() {
            "allow" => "allow",
            "block" => "block",
            "bypass" => "bypass",
            _ => "allow",
        };

        let mut names = Vec::new();
        let mut apply_profile = |profile: &str| -> Result<(), String> {
            let name = format!(
                "WSLBridge-{}-{}-{}-{}",
                rule.id, profile, protocol, rule.listen_port
            );
            let status = Command::new("netsh")
                .args([
                    "advfirewall",
                    "firewall",
                    "add",
                    "rule",
                    &format!("name={name}"),
                    &format!("dir={direction}"),
                    &format!("action={action}"),
                    &format!("protocol={protocol}"),
                    &format!("localport={}", rule.listen_port),
                    &format!("profile={profile}"),
                ])
                .output()
                .map_err(|err| format!("run netsh add rule failed: {err}"))?;

            if !status.status.success() {
                return Err(format!("netsh add rule failed for profile={profile}"));
            }
            names.push(name);
            Ok(())
        };

        let mut errors = Vec::new();
        if policy.allow_domain {
            if let Err(err) = apply_profile("domain") {
                errors.push(err);
            }
        }
        if policy.allow_private {
            if let Err(err) = apply_profile("private") {
                errors.push(err);
            }
        }
        if policy.allow_public {
            if let Err(err) = apply_profile("public") {
                errors.push(err);
            }
        }

        if errors.is_empty() || mode == FirewallMode::BestEffort {
            Ok(FirewallRuleRuntime { names })
        } else {
            Err(errors.join("; "))
        }
    }
}

pub fn cleanup_firewall(mode: FirewallMode, names: &[String]) -> Result<(), String> {
    if mode == FirewallMode::Disabled || names.is_empty() {
        return Ok(());
    }

    #[cfg(not(windows))]
    {
        let _ = names;
        return if mode == FirewallMode::Enforced {
            Err("firewall cleanup is only supported on Windows".to_owned())
        } else {
            Ok(())
        };
    }

    #[cfg(windows)]
    {
        let mut errors = Vec::new();
        for name in names {
            match Command::new("netsh")
                .args([
                    "advfirewall",
                    "firewall",
                    "delete",
                    "rule",
                    &format!("name={name}"),
                ])
                .output()
            {
                Ok(output) if output.status.success() => {}
                Ok(_) => errors.push(format!("delete firewall rule failed: {name}")),
                Err(err) => errors.push(format!("run netsh delete rule failed for {name}: {err}")),
            }
        }

        if errors.is_empty() || mode == FirewallMode::BestEffort {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }
}
