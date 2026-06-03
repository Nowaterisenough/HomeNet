pub mod aliyun;

use crate::config::add_log;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::net::{IpAddr, Ipv6Addr};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInterfaceInfo {
    pub name: String,
    pub ipv4: Vec<String>,
    pub ipv6: Vec<String>,
    pub has_global_ipv6: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Ipv6AddressCandidate {
    pub address: String,
    pub interface_name: String,
    pub preferred: bool,
    pub temporary: bool,
    pub deprecated: bool,
    pub detached: bool,
    pub duplicated: bool,
    pub tentative: bool,
    pub skip_as_source: bool,
}

/// Try to get the public IPv4 address via external services.
pub async fn get_public_ipv4() -> String {
    let services = [
        "https://api.ipify.org",
        "https://ipv4.icanhazip.com",
        "https://checkip.amazonaws.com",
    ];

    let client = reqwest::Client::new();
    for url in &services {
        match client.get(*url).timeout(std::time::Duration::from_secs(5)).send().await {
            Ok(resp) => {
                if let Ok(body) = resp.text().await {
                    let ip = body.trim().to_string();
                    if !ip.is_empty() && ip.len() < 50 {
                        return ip;
                    }
                }
            }
            Err(_) => continue,
        }
    }
    String::new()
}

/// Return local network interfaces grouped with their IPv4 and IPv6 addresses.
pub fn list_network_interfaces() -> Vec<NetworkInterfaceInfo> {
    let mut grouped: BTreeMap<String, (Vec<String>, Vec<String>)> = BTreeMap::new();
    let stable_candidates = local_ipv6_candidates();
    let stable_addresses = stable_candidate_set(&stable_candidates);
    let has_candidate_metadata = !stable_candidates.is_empty();

    match local_ip_address::list_afinet_netifas() {
        Ok(interfaces) => {
            for (name, addr) in interfaces {
                let entry = grouped.entry(name).or_default();
                match addr {
                    IpAddr::V4(ipv4) => entry.0.push(ipv4.to_string()),
                    IpAddr::V6(ipv6) => entry.1.push(ipv6.to_string()),
                }
            }
        }
        Err(e) => {
            add_log("warn", "网络", &format!("枚举网络接口失败：{}", e));
            return Vec::new();
        }
    }

    grouped
        .into_iter()
        .map(|(name, (mut ipv4, mut ipv6))| {
            ipv4.sort();
            ipv4.dedup();
            ipv6.sort();
            ipv6.dedup();
            let interface_key = normalize_interface_name(&name);
            let has_global_ipv6 = ipv6.iter().any(|addr| {
                if has_candidate_metadata {
                    addr.parse::<Ipv6Addr>()
                        .is_ok_and(|ipv6| stable_addresses.contains(&(interface_key.clone(), ipv6)))
                } else {
                    is_global_unicast_ipv6(addr)
                }
            });
            NetworkInterfaceInfo {
                name,
                ipv4,
                ipv6,
                has_global_ipv6,
            }
        })
        .collect()
}

/// Try to get a global unicast IPv6 address from one interface, or all interfaces
/// when `interface_name` is empty.
pub fn get_local_ipv6_for_interface(interface_name: &str) -> String {
    let selected = interface_name.trim();
    let candidates = local_ipv6_candidates();
    if !candidates.is_empty() {
        let interface_found = selected.is_empty()
            || candidates
                .iter()
                .any(|candidate| interface_matches(&candidate.interface_name, selected));

        if let Some(candidate) = select_stable_ipv6_candidate(&candidates, selected) {
            add_log(
                "debug",
                "DDNS",
                &format!(
                    "Found stable public IPv6: {} ({})",
                    candidate.address, candidate.interface_name
                ),
            );
            return candidate.address.clone();
        }

        if selected.is_empty() {
            add_log("debug", "DDNS", "No stable public IPv6 address found");
        } else if interface_found {
            add_log(
                "debug",
                "DDNS",
                &format!(
                    "Bound interface {} has no stable public IPv6 address",
                    selected
                ),
            );
        } else {
            add_log(
                "warn",
                "DDNS",
                &format!("鏈壘鍒扮粦瀹氱綉鍗★細{}", selected),
            );
        }
        return String::new();
    }

    match local_ip_address::list_afinet_netifas() {
        Ok(interfaces) => {
            let mut interface_found = selected.is_empty();
            for (name, addr) in &interfaces {
                if !selected.is_empty() && name != selected {
                    continue;
                }
                interface_found = true;
                if let IpAddr::V6(ipv6) = addr {
                    if is_global_unicast_ipv6_addr(ipv6) {
                        let ip_str = ipv6.to_string();
                        add_log("debug", "DDNS", &format!("发现公网 IPv6：{}（{}）", ip_str, name));
                        return ip_str;
                    }
                }
            }

            if selected.is_empty() {
                add_log("debug", "DDNS", "未发现公网 IPv6 地址");
            } else if interface_found {
                add_log(
                    "debug",
                    "DDNS",
                    &format!("绑定网卡 {} 未发现公网 IPv6 地址", selected),
                );
            } else {
                add_log("warn", "DDNS", &format!("未找到绑定网卡：{}", selected));
            }
        }
        Err(e) => {
            add_log("warn", "网络", &format!("枚举网络接口失败：{}", e));
        }
    }
    String::new()
}

/// Check if an IPv6 address is a global unicast address (2000::/3).
pub(crate) fn is_global_unicast_ipv6(addr: &str) -> bool {
    addr.parse::<Ipv6Addr>()
        .map(|ipv6| is_global_unicast_ipv6_addr(&ipv6))
        .unwrap_or(false)
}

fn is_global_unicast_ipv6_addr(addr: &Ipv6Addr) -> bool {
    (addr.segments()[0] & 0xe000) == 0x2000
}

pub(crate) fn select_stable_ipv6_candidate<'a>(
    candidates: &'a [Ipv6AddressCandidate],
    interface_name: &str,
) -> Option<&'a Ipv6AddressCandidate> {
    let selected = interface_name.trim();
    candidates
        .iter()
        .filter(|candidate| {
            selected.is_empty() || interface_matches(&candidate.interface_name, selected)
        })
        .find(|candidate| is_stable_global_ipv6_candidate(candidate))
}

pub(crate) fn is_stable_global_ipv6_candidate(candidate: &Ipv6AddressCandidate) -> bool {
    candidate.preferred
        && is_global_unicast_ipv6(&candidate.address)
        && !candidate.temporary
        && !candidate.deprecated
        && !candidate.detached
        && !candidate.duplicated
        && !candidate.tentative
        && !candidate.skip_as_source
}

pub(crate) fn stable_local_ipv6_candidate_set() -> Option<BTreeSet<(String, Ipv6Addr)>> {
    let candidates = local_ipv6_candidates();
    if candidates.is_empty() {
        None
    } else {
        Some(stable_candidate_set(&candidates))
    }
}

fn stable_candidate_set(candidates: &[Ipv6AddressCandidate]) -> BTreeSet<(String, Ipv6Addr)> {
    candidates
        .iter()
        .filter(|candidate| is_stable_global_ipv6_candidate(candidate))
        .filter_map(|candidate| {
            candidate
                .address
                .parse::<Ipv6Addr>()
                .ok()
                .map(|address| (normalize_interface_name(&candidate.interface_name), address))
        })
        .collect()
}

fn local_ipv6_candidates() -> Vec<Ipv6AddressCandidate> {
    #[cfg(windows)]
    {
        return windows_ipv6_candidates();
    }

    #[cfg(target_os = "macos")]
    {
        return macos_ipv6_candidates();
    }

    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        Vec::new()
    }
}

#[cfg(windows)]
fn windows_ipv6_candidates() -> Vec<Ipv6AddressCandidate> {
    let script = "$ErrorActionPreference='SilentlyContinue'; Get-NetIPAddress -AddressFamily IPv6 | Select-Object IPAddress,InterfaceAlias,@{Name='AddressState';Expression={$_.AddressState.ToString()}},@{Name='PrefixOrigin';Expression={$_.PrefixOrigin.ToString()}},@{Name='SuffixOrigin';Expression={$_.SuffixOrigin.ToString()}},SkipAsSource | ConvertTo-Json -Compress";
    let output = powershell_output(script);

    output
        .ok()
        .filter(|output| output.status.success())
        .map(|output| parse_windows_net_ip_address_json(&String::from_utf8_lossy(&output.stdout)))
        .unwrap_or_default()
}

#[cfg(windows)]
fn powershell_args(script: &str) -> [&str; 5] {
    ["-NoProfile", "-WindowStyle", "Hidden", "-Command", script]
}

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

#[cfg(windows)]
fn powershell_output(script: &str) -> std::io::Result<std::process::Output> {
    use std::os::windows::process::CommandExt;

    std::process::Command::new("powershell")
        .args(powershell_args(script))
        .creation_flags(CREATE_NO_WINDOW)
        .output()
}

#[cfg(target_os = "macos")]
fn macos_ipv6_candidates() -> Vec<Ipv6AddressCandidate> {
    std::process::Command::new("ifconfig")
        .arg("-a")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| {
            parse_macos_ifconfig_ipv6_candidates(&String::from_utf8_lossy(&output.stdout))
        })
        .unwrap_or_default()
}

#[cfg(any(target_os = "macos", test))]
pub(crate) fn parse_macos_ifconfig_ipv6_candidates(value: &str) -> Vec<Ipv6AddressCandidate> {
    let mut candidates = Vec::new();
    let mut current_interface = String::new();

    for line in value.lines() {
        if let Some(interface_name) = parse_macos_ifconfig_interface_name(line) {
            current_interface = interface_name;
            continue;
        }

        if current_interface.is_empty() {
            continue;
        }

        let trimmed = line.trim();
        let tokens = trimmed.split_whitespace().collect::<Vec<_>>();
        if tokens.first() != Some(&"inet6") {
            continue;
        }
        let Some(raw_address) = tokens.get(1) else {
            continue;
        };
        let address = raw_address
            .split('%')
            .next()
            .unwrap_or_default()
            .trim()
            .to_string();
        if address.parse::<Ipv6Addr>().is_err() {
            continue;
        }

        let has_flag = |flag: &str| {
            tokens.iter().skip(2).any(|token| {
                token
                    .trim_matches(|ch: char| !ch.is_ascii_alphanumeric())
                    .eq_ignore_ascii_case(flag)
            })
        };

        candidates.push(Ipv6AddressCandidate {
            address,
            interface_name: current_interface.clone(),
            preferred: !has_flag("deprecated") && !has_flag("detached") && !has_flag("duplicated"),
            temporary: has_flag("temporary"),
            deprecated: has_flag("deprecated"),
            detached: has_flag("detached"),
            duplicated: has_flag("duplicated") || has_flag("duplicate"),
            tentative: has_flag("tentative"),
            skip_as_source: false,
        });
    }

    candidates
}

#[cfg(any(target_os = "macos", test))]
fn parse_macos_ifconfig_interface_name(line: &str) -> Option<String> {
    if line.starts_with(char::is_whitespace) {
        return None;
    }

    let (name, _) = line.split_once(':')?;
    let name = name.trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

#[cfg(any(windows, test))]
pub(crate) fn parse_windows_net_ip_address_json(value: &str) -> Vec<Ipv6AddressCandidate> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let rows = match serde_json::from_str::<serde_json::Value>(trimmed) {
        Ok(serde_json::Value::Array(rows)) => rows,
        Ok(serde_json::Value::Object(row)) => vec![serde_json::Value::Object(row)],
        _ => Vec::new(),
    };

    rows.into_iter()
        .filter_map(|row| {
            let row = row.as_object()?;
            let address = row
                .get("IPAddress")
                .map(json_value_to_string)
                .unwrap_or_default();
            if address.parse::<Ipv6Addr>().is_err() {
                return None;
            }

            let interface_name = row
                .get("InterfaceAlias")
                .map(json_value_to_string)
                .unwrap_or_default();
            let address_state = row
                .get("AddressState")
                .map(json_value_to_string)
                .unwrap_or_default();
            let suffix_origin = row
                .get("SuffixOrigin")
                .map(json_value_to_string)
                .unwrap_or_default();
            let skip_as_source = row.get("SkipAsSource").is_some_and(json_value_to_bool);
            let state = address_state.trim();

            Some(Ipv6AddressCandidate {
                address,
                interface_name,
                preferred: state.is_empty() || state.eq_ignore_ascii_case("Preferred"),
                temporary: suffix_origin.eq_ignore_ascii_case("Random"),
                deprecated: state.eq_ignore_ascii_case("Deprecated"),
                detached: state.eq_ignore_ascii_case("Invalid"),
                duplicated: state.eq_ignore_ascii_case("Duplicate"),
                tentative: state.eq_ignore_ascii_case("Tentative"),
                skip_as_source,
            })
        })
        .collect()
}

#[cfg(any(windows, test))]
fn json_value_to_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(value) => value.clone(),
        serde_json::Value::Number(value) => value.to_string(),
        serde_json::Value::Bool(value) => value.to_string(),
        _ => String::new(),
    }
}

#[cfg(any(windows, test))]
fn json_value_to_bool(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Bool(value) => *value,
        serde_json::Value::String(value) => value.eq_ignore_ascii_case("true"),
        serde_json::Value::Number(value) => value.as_i64().is_some_and(|value| value != 0),
        _ => false,
    }
}

fn normalize_interface_name(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn interface_matches(actual: &str, expected: &str) -> bool {
    normalize_interface_name(actual) == normalize_interface_name(expected)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(address: &str) -> Ipv6AddressCandidate {
        Ipv6AddressCandidate {
            address: address.to_string(),
            interface_name: "en0".to_string(),
            preferred: true,
            ..Ipv6AddressCandidate::default()
        }
    }

    #[test]
    fn stable_ipv6_selection_rejects_temporary_and_deprecated_addresses() {
        let mut temporary = candidate("2408:8200::100");
        temporary.temporary = true;
        let mut deprecated = candidate("2408:8200::101");
        deprecated.deprecated = true;
        let stable = candidate("2408:8200::102");

        assert_eq!(
            select_stable_ipv6_candidate(&[temporary, deprecated, stable], "en0")
                .map(|item| item.address.as_str()),
            Some("2408:8200::102")
        );
    }

    #[test]
    fn stable_ipv6_selection_rejects_windows_skip_as_source() {
        let mut skipped = candidate("2408:8200::200");
        skipped.skip_as_source = true;
        let stable = candidate("2408:8200::201");

        assert_eq!(
            select_stable_ipv6_candidate(&[skipped, stable], "en0")
                .map(|item| item.address.as_str()),
            Some("2408:8200::201")
        );
    }

    #[test]
    fn stable_ipv6_selection_rejects_windows_random_suffix_origin() {
        let json = r#"[
{"IPAddress":"2408:8200::30","InterfaceAlias":"Ethernet","AddressState":"Preferred","SuffixOrigin":"Random","SkipAsSource":false},
{"IPAddress":"2408:8200::31","InterfaceAlias":"Ethernet","AddressState":"Preferred","SuffixOrigin":"Link","SkipAsSource":false}
]"#;

        let candidates = parse_windows_net_ip_address_json(json);

        assert_eq!(
            select_stable_ipv6_candidate(&candidates, "Ethernet").map(|item| item.address.as_str()),
            Some("2408:8200::31")
        );
        assert!(candidates
            .iter()
            .any(|item| item.address == "2408:8200::30" && item.temporary));
    }

    #[test]
    fn parses_macos_ifconfig_ipv6_flags_for_stability() {
        let output = r#"
en0: flags=8863<UP,BROADCAST,SMART,RUNNING,SIMPLEX,MULTICAST> mtu 1500
    inet6 fe80::1234%en0 prefixlen 64 secured scopeid 0xb
    inet6 2408:8200::10 prefixlen 64 autoconf secured
    inet6 2408:8200::11 prefixlen 64 autoconf temporary
    inet6 2408:8200::12 prefixlen 64 deprecated autoconf temporary
awdl0: flags=8943<UP,BROADCAST,RUNNING,PROMISC,SIMPLEX,MULTICAST> mtu 1484
    inet6 2408:8200::13 prefixlen 64 autoconf secured
"#;

        let candidates = parse_macos_ifconfig_ipv6_candidates(output);

        assert_eq!(
            select_stable_ipv6_candidate(&candidates, "en0").map(|item| item.address.as_str()),
            Some("2408:8200::10")
        );
        assert!(candidates
            .iter()
            .any(|item| item.address == "2408:8200::11" && item.temporary));
        assert!(candidates
            .iter()
            .any(|item| item.address == "2408:8200::12" && item.deprecated));
    }

    #[test]
    fn parses_windows_ip_address_json_for_stability() {
        let json = r#"[
{"IPAddress":"2408:8200::20","InterfaceAlias":"Ethernet","AddressState":"Preferred","PrefixOrigin":"RouterAdvertisement","SuffixOrigin":"Link","SkipAsSource":true},
{"IPAddress":"2408:8200::21","InterfaceAlias":"Ethernet","AddressState":"Tentative","PrefixOrigin":"RouterAdvertisement","SuffixOrigin":"Link","SkipAsSource":false},
{"IPAddress":"2408:8200::22","InterfaceAlias":"Ethernet","AddressState":"Preferred","PrefixOrigin":"RouterAdvertisement","SuffixOrigin":"Link","SkipAsSource":false}
]"#;

        let candidates = parse_windows_net_ip_address_json(json);

        assert_eq!(
            select_stable_ipv6_candidate(&candidates, "Ethernet").map(|item| item.address.as_str()),
            Some("2408:8200::22")
        );
        assert!(candidates
            .iter()
            .any(|item| item.address == "2408:8200::20" && item.skip_as_source));
        assert!(candidates
            .iter()
            .any(|item| item.address == "2408:8200::21" && item.tentative));
    }
}
