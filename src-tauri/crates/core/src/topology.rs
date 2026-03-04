use chrono::{DateTime, Utc};
use serde::Serialize;
use std::net::IpAddr;

#[cfg(windows)]
use serde::Deserialize;
#[cfg(windows)]
use std::{env, fs, path::PathBuf, process::Command};

use wsl_bridge_shared::{AdapterInfo, HyperVVmInfo, TargetKind, WslInfo};

#[derive(Debug, Clone)]
pub struct HyperVScanResult {
    pub items: Vec<HyperVVmInfo>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HyperVProbeStep {
    pub source: String,
    pub executable: String,
    pub ok: bool,
    pub status_code: i32,
    pub parsed_vm_names: Vec<String>,
    pub raw_stdout: String,
    pub raw_stderr: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct HyperVProbeDebug {
    pub timestamp: DateTime<Utc>,
    pub selected_vm_names: Vec<String>,
    pub steps: Vec<HyperVProbeStep>,
}

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

pub fn list_wsl_instances() -> Vec<WslInfo> {
    #[cfg(windows)]
    {
        let networking_mode = read_wsl_networking_mode();
        let output = match run_command_bytes("wsl.exe", &["--list", "--quiet"]) {
            Some(value) => value,
            None => return Vec::new(),
        };

        let mut items = Vec::new();
        let text = decode_command_output(&output);
        for raw in text.lines() {
            let distro = clean_text(raw);
            if distro.is_empty() {
                continue;
            }
            let ip = resolve_wsl_ip(&distro);
            items.push(WslInfo {
                distro,
                networking_mode: networking_mode.clone(),
                ip,
            });
        }
        return items;
    }

    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

pub fn scan_hyperv() -> HyperVScanResult {
    #[cfg(windows)]
    {
        let capture = match run_powershell_capture(hyperv_json_script()) {
            Some(value) => value,
            None => {
                return HyperVScanResult {
                    items: Vec::new(),
                    error: Some("无法启动 PowerShell，无法读取 Hyper-V 虚拟机列表。".to_owned()),
                }
            }
        };

        if !capture.ok {
            return HyperVScanResult {
                items: Vec::new(),
                error: Some(normalize_hyperv_error(&capture)),
            };
        }

        match parse_hyperv_json_output(&capture.stdout) {
            Ok(mut items) => {
                items.sort_by(|a, b| a.vm_name.to_lowercase().cmp(&b.vm_name.to_lowercase()));
                HyperVScanResult { items, error: None }
            }
            Err(err) => HyperVScanResult {
                items: Vec::new(),
                error: Some(err),
            },
        }
    }

    #[cfg(not(windows))]
    {
        HyperVScanResult {
            items: Vec::new(),
            error: None,
        }
    }
}

pub fn list_hyperv_vms() -> Vec<HyperVVmInfo> {
    scan_hyperv().items
}

pub fn debug_hyperv_probe() -> HyperVProbeDebug {
    #[cfg(windows)]
    {
        let mut steps = Vec::new();
        let mut selected_vm_names = Vec::new();

        let json_capture = run_powershell_capture(hyperv_json_script());
        let mut json_names = Vec::new();
        if let Some(capture) = &json_capture {
            if capture.ok {
                if let Ok(items) = parse_hyperv_json_output(&capture.stdout) {
                    json_names = items
                        .into_iter()
                        .map(|item| item.vm_name)
                        .collect::<Vec<_>>();
                }
            }
        }
        if !json_names.is_empty() {
            selected_vm_names = json_names.clone();
        }
        steps.push(to_probe_step("hyperv_json", json_capture, json_names));

        let table_capture = run_powershell_capture(hyperv_table_script());
        steps.push(to_probe_step(
            "Get-VM | Get-VMNetworkAdapter | Format-Table VMName, SwitchName, MacAddress, IPAddresses",
            table_capture,
            Vec::new(),
        ));

        return HyperVProbeDebug {
            timestamp: Utc::now(),
            selected_vm_names,
            steps,
        };
    }

    #[cfg(not(windows))]
    {
        HyperVProbeDebug {
            timestamp: Utc::now(),
            selected_vm_names: Vec::new(),
            steps: vec![HyperVProbeStep {
                source: "not_windows".to_owned(),
                executable: String::new(),
                ok: false,
                status_code: -1,
                parsed_vm_names: Vec::new(),
                raw_stdout: String::new(),
                raw_stderr: "Hyper-V probe is only available on Windows".to_owned(),
            }],
        }
    }
}

pub fn resolve_dynamic_target_host(target_kind: TargetKind, target_ref: &str) -> Option<String> {
    let key = target_ref.trim();
    if key.is_empty() {
        return None;
    }

    match target_kind {
        TargetKind::Wsl => list_wsl_instances()
            .into_iter()
            .find(|item| item.distro.eq_ignore_ascii_case(key))
            .and_then(|item| item.ip),
        TargetKind::Hyperv => list_hyperv_vms()
            .into_iter()
            .find(|item| item.vm_name.eq_ignore_ascii_case(key))
            .and_then(|item| item.ip),
        TargetKind::Static => None,
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

#[cfg(windows)]
fn read_wsl_networking_mode() -> String {
    let mut path = env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    path.push(".wslconfig");

    let content = match fs::read_to_string(&path) {
        Ok(value) => value,
        Err(_) => return "nat".to_owned(),
    };

    for raw in content.lines() {
        let line = raw.trim();
        if line.starts_with('#') || line.starts_with(';') || line.is_empty() {
            continue;
        }
        let lowered = line.to_ascii_lowercase();
        if !lowered.starts_with("networkingmode") {
            continue;
        }
        if let Some((_, value)) = line.split_once('=') {
            let value = value.trim();
            if !value.is_empty() {
                return value.to_owned();
            }
        }
    }

    "nat".to_owned()
}

#[cfg(windows)]
fn resolve_wsl_ip(distro: &str) -> Option<String> {
    if let Some(raw) = run_command_bytes("wsl.exe", &["-d", distro, "hostname", "-I"]) {
        let text = decode_command_output(&raw);
        if let Some(ip) = first_ip_token(&text) {
            return Some(ip);
        }
    }

    if let Some(raw) = run_command_bytes(
        "wsl.exe",
        &[
            "-d",
            distro,
            "sh",
            "-lc",
            "ip -4 -o addr show scope global | awk '{print $4}' | cut -d/ -f1",
        ],
    ) {
        let text = decode_command_output(&raw);
        if let Some(ip) = first_ip_token(&text) {
            return Some(ip);
        }
    }

    None
}

#[cfg(windows)]
#[derive(Debug, Clone)]
struct PowerShellCapture {
    executable: String,
    ok: bool,
    status_code: i32,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

#[cfg(windows)]
fn to_probe_step(
    source: &str,
    capture: Option<PowerShellCapture>,
    parsed_vm_names: Vec<String>,
) -> HyperVProbeStep {
    match capture {
        Some(capture) => HyperVProbeStep {
            source: source.to_owned(),
            executable: capture.executable,
            ok: capture.ok,
            status_code: capture.status_code,
            parsed_vm_names,
            raw_stdout: decode_command_output(&capture.stdout),
            raw_stderr: decode_command_output(&capture.stderr),
        },
        None => HyperVProbeStep {
            source: source.to_owned(),
            executable: String::new(),
            ok: false,
            status_code: -1,
            parsed_vm_names: Vec::new(),
            raw_stdout: String::new(),
            raw_stderr: "unable to execute PowerShell".to_owned(),
        },
    }
}

#[cfg(not(windows))]
fn to_probe_step(
    source: &str,
    _capture: Option<()>,
    parsed_vm_names: Vec<String>,
) -> HyperVProbeStep {
    HyperVProbeStep {
        source: source.to_owned(),
        executable: String::new(),
        ok: false,
        status_code: -1,
        parsed_vm_names,
        raw_stdout: String::new(),
        raw_stderr: String::new(),
    }
}

#[cfg(windows)]
fn normalize_hyperv_error(capture: &PowerShellCapture) -> String {
    let stdout = decode_command_output(&capture.stdout);
    let stderr = decode_command_output(&capture.stderr);
    let joined = format!("{stdout}\n{stderr}").to_ascii_lowercase();
    if joined.contains("elevation_required")
        || joined.contains("access is denied")
        || joined.contains("administrator")
        || joined.contains("权限")
    {
        return "Hyper-V 查询需要管理员权限，请以管理员身份启动应用后重试。".to_owned();
    }

    let text = if !stderr.trim().is_empty() {
        clean_text(&stderr)
    } else {
        clean_text(&stdout)
    };
    if text.is_empty() {
        format!(
            "Hyper-V 查询失败（status={}），请确认 Hyper-V 模块可用且当前进程具备权限。",
            capture.status_code
        )
    } else {
        format!("Hyper-V 查询失败：{text}")
    }
}

#[cfg(windows)]
fn powershell_candidates() -> Vec<String> {
    let mut candidates = vec![
        "powershell.exe".to_owned(),
        "powershell".to_owned(),
        "pwsh.exe".to_owned(),
        "pwsh".to_owned(),
    ];
    if let Some(system_root) = env::var_os("SystemRoot") {
        let system_root = PathBuf::from(system_root);
        candidates.push(
            system_root
                .join("System32")
                .join("WindowsPowerShell")
                .join("v1.0")
                .join("powershell.exe")
                .to_string_lossy()
                .to_string(),
        );
        candidates.push(
            system_root
                .join("Sysnative")
                .join("WindowsPowerShell")
                .join("v1.0")
                .join("powershell.exe")
                .to_string_lossy()
                .to_string(),
        );
    }
    candidates
}

#[cfg(windows)]
fn run_powershell_capture(script: &str) -> Option<PowerShellCapture> {
    let mut last_error = None;
    for executable in powershell_candidates() {
        match Command::new(executable.as_str())
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .output()
        {
            Ok(output) => {
                return Some(PowerShellCapture {
                    executable,
                    ok: output.status.success(),
                    status_code: output.status.code().unwrap_or(-1),
                    stdout: output.stdout,
                    stderr: output.stderr,
                });
            }
            Err(err) => {
                last_error = Some(PowerShellCapture {
                    executable,
                    ok: false,
                    status_code: -1,
                    stdout: Vec::new(),
                    stderr: err.to_string().into_bytes(),
                });
            }
        }
    }
    last_error
}

#[cfg(windows)]
fn run_command_bytes(executable: &str, args: &[&str]) -> Option<Vec<u8>> {
    let output = Command::new(executable).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(output.stdout)
}

fn decode_command_output(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }
    if let Some(text) = decode_utf16(bytes) {
        return text;
    }
    String::from_utf8_lossy(bytes).into_owned()
}

fn decode_utf16(bytes: &[u8]) -> Option<String> {
    if bytes.len() < 2 || bytes.len() % 2 != 0 {
        return None;
    }

    let is_le_bom = bytes[0] == 0xff && bytes[1] == 0xfe;
    let is_be_bom = bytes[0] == 0xfe && bytes[1] == 0xff;

    let sample_len = bytes.len().min(64);
    let sample = &bytes[..sample_len];
    let odd_zeros = sample
        .iter()
        .skip(1)
        .step_by(2)
        .filter(|&&b| b == 0)
        .count();
    let even_zeros = sample.iter().step_by(2).filter(|&&b| b == 0).count();
    let looks_utf16_like =
        sample.len() >= 8 && (odd_zeros >= sample.len() / 8 || even_zeros >= sample.len() / 8);

    if !is_le_bom && !is_be_bom && !looks_utf16_like {
        return None;
    }

    let mut units = Vec::with_capacity(bytes.len() / 2);
    if is_be_bom {
        for chunk in bytes[2..].chunks_exact(2) {
            units.push(u16::from_be_bytes([chunk[0], chunk[1]]));
        }
    } else {
        let start = if is_le_bom { 2 } else { 0 };
        for chunk in bytes[start..].chunks_exact(2) {
            units.push(u16::from_le_bytes([chunk[0], chunk[1]]));
        }
    }

    String::from_utf16(&units).ok()
}

fn clean_text(text: &str) -> String {
    text.trim()
        .trim_matches('\u{feff}')
        .replace('\u{0000}', "")
        .trim()
        .to_owned()
}

fn first_ip_token(text: &str) -> Option<String> {
    let mut first_any = None::<String>;
    for raw in text.split_whitespace() {
        let token = raw.trim_matches(|c: char| c == ',' || c == ';');
        if token.is_empty() {
            continue;
        }
        if let Ok(ip) = token.parse::<IpAddr>() {
            if ip.is_ipv4() {
                return Some(ip.to_string());
            }
            if first_any.is_none() {
                first_any = Some(ip.to_string());
            }
        }
    }
    first_any
}

#[cfg(windows)]
#[derive(Debug, Deserialize)]
struct HyperVAdapterRow {
    #[serde(rename = "VMName")]
    vm_name: String,
    #[serde(rename = "SwitchName", default)]
    switch_name: Option<String>,
    #[serde(rename = "IPAddresses", default)]
    ip_addresses: Option<StringOrList>,
}

#[cfg(windows)]
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum StringOrList {
    One(String),
    Many(Vec<String>),
}

#[cfg(windows)]
impl StringOrList {
    fn into_vec(self) -> Vec<String> {
        match self {
            StringOrList::One(value) => vec![value],
            StringOrList::Many(values) => values,
        }
    }
}

#[cfg(windows)]
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum JsonOneOrMany<T> {
    One(T),
    Many(Vec<T>),
}

#[cfg(windows)]
fn parse_hyperv_json_output(stdout: &[u8]) -> Result<Vec<HyperVVmInfo>, String> {
    let text = clean_text(&decode_command_output(stdout));
    if text.is_empty() || text.eq_ignore_ascii_case("null") {
        return Ok(Vec::new());
    }

    let parsed = serde_json::from_str::<JsonOneOrMany<HyperVAdapterRow>>(&text)
        .map_err(|err| format!("Hyper-V 输出 JSON 解析失败: {err}"))?;
    let rows = match parsed {
        JsonOneOrMany::One(row) => vec![row],
        JsonOneOrMany::Many(rows) => rows,
    };

    let mut result = std::collections::BTreeMap::<String, HyperVVmInfo>::new();
    for row in rows {
        let vm_name = clean_text(&row.vm_name);
        if vm_name.is_empty() {
            continue;
        }
        let switch = row
            .switch_name
            .map(|value| clean_text(&value))
            .filter(|v| !v.is_empty());
        let ip = row
            .ip_addresses
            .map(StringOrList::into_vec)
            .and_then(|items| first_ipv4_from_values(&items));

        let entry = result.entry(vm_name.clone()).or_insert(HyperVVmInfo {
            vm_name,
            v_switch: None,
            ip: None,
        });
        if entry.v_switch.is_none() {
            entry.v_switch = switch;
        }
        if entry.ip.is_none() {
            entry.ip = ip;
        }
    }

    Ok(result.into_values().collect::<Vec<_>>())
}

#[cfg(windows)]
fn first_ipv4_from_values(values: &[String]) -> Option<String> {
    for value in values {
        if let Some(ip) = first_ip_token(value) {
            if ip.parse::<IpAddr>().ok().is_some_and(|addr| addr.is_ipv4()) {
                return Some(ip);
            }
        }
    }
    None
}

#[cfg(windows)]
fn hyperv_json_script() -> &'static str {
    r#"
$ErrorActionPreference='Stop'
$isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $isAdmin) { throw 'ELEVATION_REQUIRED: Hyper-V query requires administrator privileges.' }
Import-Module Hyper-V -ErrorAction Stop
Get-VM | Get-VMNetworkAdapter | Select-Object VMName, SwitchName, MacAddress, IPAddresses | ConvertTo-Json -Compress -Depth 8
"#
}

#[cfg(windows)]
fn hyperv_table_script() -> &'static str {
    r#"
$ErrorActionPreference='Stop'
$isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $isAdmin) { throw 'ELEVATION_REQUIRED: Hyper-V query requires administrator privileges.' }
Import-Module Hyper-V -ErrorAction Stop
Get-VM | Get-VMNetworkAdapter | Format-Table VMName, SwitchName, MacAddress, IPAddresses | Out-String -Width 4096
"#
}

#[cfg(test)]
mod tests {
    #[cfg(windows)]
    use super::parse_hyperv_json_output;
    use super::{clean_text, decode_command_output, first_ip_token};

    #[test]
    fn decode_utf16le_output() {
        let bytes = b"U\0b\0u\0n\0t\0u\0\r\0\n\0d\0e\0b\0i\0a\0n\0\r\0\n\0";
        let text = decode_command_output(bytes);
        assert_eq!(text, "Ubuntu\r\ndebian\r\n");
    }

    #[test]
    fn clean_text_removes_bom_and_nul() {
        let cleaned = clean_text("\u{feff}U\0b\0u\0n\0t\0u\0");
        assert_eq!(cleaned, "Ubuntu");
    }

    #[test]
    fn first_ip_token_prefers_ipv4() {
        let ip = first_ip_token("fd7a:115c:a1e0::1 172.24.1.2").expect("ip");
        assert_eq!(ip, "172.24.1.2");
    }

    #[cfg(windows)]
    #[test]
    fn parse_hyperv_json_output_array() {
        let raw = br#"[{"VMName":"vm-a","SwitchName":"Default Switch","MacAddress":"00155D111111","IPAddresses":["172.17.2.2","fe80::1"]},{"VMName":"vm-b","SwitchName":"Default Switch","MacAddress":"00155D222222","IPAddresses":"10.0.0.8"}]"#;
        let rows = parse_hyperv_json_output(raw).expect("parse");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].vm_name, "vm-a");
        assert_eq!(rows[0].ip.as_deref(), Some("172.17.2.2"));
        assert_eq!(rows[1].vm_name, "vm-b");
        assert_eq!(rows[1].ip.as_deref(), Some("10.0.0.8"));
    }
}
