use chrono::Utc;
use local_ip_address::list_afinet_netifas;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LanDevice {
    pub id: String,
    pub display_name: String,
    pub hostname: String,
    pub mac: String,
    pub ipv4: Vec<String>,
    pub ipv6: Vec<String>,
    pub global_ipv6: Vec<String>,
    pub online: bool,
    pub source: String,
    pub last_seen: String,
}

#[derive(Debug, Clone)]
struct NeighborRow {
    ip_address: String,
    link_layer_address: String,
    state: String,
    interface_alias: String,
    _address_family: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct RawNeighborRow {
    #[serde(rename = "IPAddress", default)]
    ip_address: Value,
    #[serde(rename = "LinkLayerAddress", default)]
    link_layer_address: Value,
    #[serde(rename = "State", default)]
    state: Value,
    #[serde(rename = "InterfaceAlias", default)]
    interface_alias: Value,
    #[serde(rename = "AddressFamily", default)]
    address_family: Option<Value>,
}

impl<'de> Deserialize<'de> for NeighborRow {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawNeighborRow::deserialize(deserializer)?;
        Ok(Self {
            ip_address: value_to_string(&raw.ip_address),
            link_layer_address: value_to_string(&raw.link_layer_address),
            state: value_to_string(&raw.state),
            interface_alias: value_to_string(&raw.interface_alias),
            _address_family: raw.address_family,
        })
    }
}

impl NeighborRow {
    #[cfg(test)]
    fn new(
        ip_address: &str,
        link_layer_address: &str,
        state: &str,
        interface_alias: &str,
        address_family: Option<&str>,
    ) -> Self {
        Self {
            ip_address: ip_address.to_string(),
            link_layer_address: link_layer_address.to_string(),
            state: state.to_string(),
            interface_alias: interface_alias.to_string(),
            _address_family: address_family.map(|value| Value::String(value.to_string())),
        }
    }

    fn normalized_mac(&self) -> Option<String> {
        normalize_mac(&self.link_layer_address)
    }

    fn is_usable(&self) -> bool {
        !self.state.eq_ignore_ascii_case("unreachable")
            && self.ip_address.parse::<IpAddr>().is_ok_and(is_usable_ip)
            && self.normalized_mac().is_some_and(|mac| is_usable_mac(&mac))
    }
}

pub fn discover_lan_devices() -> Vec<LanDevice> {
    let local_devices = discover_local_interface_devices();
    let neighbor_devices = discover_neighbor_devices();
    merge_local_and_neighbor_devices(local_devices, neighbor_devices)
}

pub fn is_global_ipv6(value: &str) -> bool {
    match value.parse::<Ipv6Addr>() {
        Ok(addr) => {
            let first_segment = addr.segments()[0];
            (0x2000..=0x3fff).contains(&first_segment)
        }
        Err(_) => false,
    }
}

#[cfg(windows)]
fn discover_neighbor_devices() -> Vec<LanDevice> {
    use std::process::Command;

    let script = "Get-NetNeighbor -AddressFamily IPv4,IPv6 | Where-Object { $_.IPAddress -and $_.LinkLayerAddress -and $_.State -ne 'Unreachable' } | Select-Object IPAddress,LinkLayerAddress,State,InterfaceAlias,AddressFamily | ConvertTo-Json -Compress";
    let output = Command::new("powershell")
        .args(["-NoProfile", "-Command", script])
        .output();

    match output {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            merge_neighbor_rows(parse_powershell_neighbor_json(&stdout))
        }
        _ => Vec::new(),
    }
}

#[cfg(target_os = "macos")]
fn discover_neighbor_devices() -> Vec<LanDevice> {
    use std::process::Command;

    let arp_output = Command::new("arp").arg("-an").output();
    let ndp_output = Command::new("ndp").arg("-an").output();
    let arp_stdout = arp_output
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).to_string())
        .unwrap_or_default();
    let ndp_stdout = ndp_output
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).to_string())
        .unwrap_or_default();

    merge_neighbor_rows_with_source(
        parse_macos_neighbor_tables(&arp_stdout, &ndp_stdout),
        "macos-neighbor",
    )
}

#[cfg(all(not(windows), not(target_os = "macos")))]
fn discover_neighbor_devices() -> Vec<LanDevice> {
    Vec::new()
}

fn parse_powershell_neighbor_json(value: &str) -> Vec<NeighborRow> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    match serde_json::from_str::<Value>(trimmed) {
        Ok(Value::Array(rows)) => rows
            .into_iter()
            .filter_map(|row| serde_json::from_value::<NeighborRow>(row).ok())
            .filter(NeighborRow::is_usable)
            .collect(),
        Ok(Value::Object(_)) => serde_json::from_str::<NeighborRow>(trimmed)
            .ok()
            .filter(NeighborRow::is_usable)
            .into_iter()
            .collect(),
        _ => Vec::new(),
    }
}

fn merge_neighbor_rows(rows: Vec<NeighborRow>) -> Vec<LanDevice> {
    merge_neighbor_rows_with_source(rows, "windows-neighbor")
}

fn merge_neighbor_rows_with_source(rows: Vec<NeighborRow>, source: &str) -> Vec<LanDevice> {
    let now = Utc::now().to_rfc3339();
    let mut devices_by_key: HashMap<String, LanDevice> = HashMap::new();
    let mut ordered_keys = Vec::new();

    for row in rows.into_iter().filter(NeighborRow::is_usable) {
        let normalized_mac = row.normalized_mac();
        let key = normalized_mac
            .as_ref()
            .map(|mac| format!("mac:{mac}"))
            .unwrap_or_else(|| format!("ip:{}", row.ip_address));

        let device = devices_by_key.entry(key.clone()).or_insert_with(|| {
            ordered_keys.push(key.clone());
            let mac = normalized_mac.clone().unwrap_or_default();
            let display_name = if !mac.is_empty() {
                mac.clone()
            } else if !row.interface_alias.trim().is_empty() {
                row.interface_alias.clone()
            } else {
                row.ip_address.clone()
            };

            LanDevice {
                id: if !mac.is_empty() {
                    format!("mac-{}", mac.replace(':', "-"))
                } else {
                    format!("ip-{}", sanitize_id_part(&row.ip_address))
                },
                display_name,
                hostname: String::new(),
                mac,
                ipv4: Vec::new(),
                ipv6: Vec::new(),
                global_ipv6: Vec::new(),
                online: false,
                source: source.to_string(),
                last_seen: now.clone(),
            }
        });

        device.online = true;
        add_ip_to_device(device, &row.ip_address);
    }

    ordered_keys
        .into_iter()
        .filter_map(|key| devices_by_key.remove(&key))
        .collect()
}

#[cfg(any(target_os = "macos", test))]
fn parse_macos_neighbor_tables(arp_output: &str, ndp_output: &str) -> Vec<NeighborRow> {
    parse_macos_arp_output(arp_output)
        .into_iter()
        .chain(parse_macos_ndp_output(ndp_output))
        .filter(NeighborRow::is_usable)
        .collect()
}

#[cfg(any(target_os = "macos", test))]
fn parse_macos_arp_output(value: &str) -> Vec<NeighborRow> {
    value.lines().filter_map(parse_macos_arp_line).collect()
}

#[cfg(any(target_os = "macos", test))]
fn parse_macos_arp_line(line: &str) -> Option<NeighborRow> {
    let line = line.trim();
    let ip_address = extract_parenthesized_ip(line)?;
    if ip_address.parse::<Ipv4Addr>().is_err() {
        return None;
    }

    let tokens: Vec<&str> = line.split_whitespace().collect();
    let at_index = tokens.iter().position(|token| *token == "at")?;
    let link_layer_address = tokens.get(at_index + 1)?;
    let interface_alias = token_after(&tokens, "on").unwrap_or_default();

    Some(NeighborRow {
        ip_address,
        link_layer_address: link_layer_address.to_string(),
        state: "Reachable".to_string(),
        interface_alias,
        _address_family: Some(Value::String("IPv4".to_string())),
    })
}

#[cfg(any(target_os = "macos", test))]
fn parse_macos_ndp_output(value: &str) -> Vec<NeighborRow> {
    value.lines().filter_map(parse_macos_ndp_line).collect()
}

#[cfg(any(target_os = "macos", test))]
fn parse_macos_ndp_line(line: &str) -> Option<NeighborRow> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }

    if line.contains(" at ") {
        return parse_macos_parenthesized_ndp_line(line);
    }

    let mut tokens = line.split_whitespace();
    let ip_address = strip_ipv6_zone(tokens.next()?);
    if ip_address.parse::<Ipv6Addr>().is_err() {
        return None;
    }

    let link_layer_address = tokens.next()?;
    let interface_alias = tokens.next().unwrap_or_default().to_string();

    Some(NeighborRow {
        ip_address,
        link_layer_address: link_layer_address.to_string(),
        state: "Reachable".to_string(),
        interface_alias,
        _address_family: Some(Value::String("IPv6".to_string())),
    })
}

#[cfg(any(target_os = "macos", test))]
fn parse_macos_parenthesized_ndp_line(line: &str) -> Option<NeighborRow> {
    let ip_address = strip_ipv6_zone(&extract_parenthesized_ip(line)?);
    if ip_address.parse::<Ipv6Addr>().is_err() {
        return None;
    }

    let tokens: Vec<&str> = line.split_whitespace().collect();
    let at_index = tokens.iter().position(|token| *token == "at")?;
    let link_layer_address = tokens.get(at_index + 1)?;
    let interface_alias = token_after(&tokens, "on").unwrap_or_default();

    Some(NeighborRow {
        ip_address,
        link_layer_address: link_layer_address.to_string(),
        state: "Reachable".to_string(),
        interface_alias,
        _address_family: Some(Value::String("IPv6".to_string())),
    })
}

#[cfg(any(target_os = "macos", test))]
fn extract_parenthesized_ip(line: &str) -> Option<String> {
    let start = line.find('(')? + 1;
    let end = line[start..].find(')')? + start;
    Some(line[start..end].to_string())
}

#[cfg(any(target_os = "macos", test))]
fn token_after(tokens: &[&str], target: &str) -> Option<String> {
    tokens
        .iter()
        .position(|token| *token == target)
        .and_then(|index| tokens.get(index + 1))
        .map(|value| value.to_string())
}

#[cfg(any(target_os = "macos", test))]
fn strip_ipv6_zone(value: &str) -> String {
    value
        .split_once('%')
        .map(|(ip, _)| ip)
        .unwrap_or(value)
        .to_string()
}

fn merge_local_and_neighbor_devices(
    local_devices: Vec<LanDevice>,
    neighbor_devices: Vec<LanDevice>,
) -> Vec<LanDevice> {
    if local_devices.is_empty() {
        return neighbor_devices;
    }
    if neighbor_devices.is_empty() {
        return local_devices;
    }

    let local_ips = local_devices
        .iter()
        .flat_map(|device| device.ipv4.iter().chain(device.ipv6.iter()))
        .cloned()
        .collect::<Vec<_>>();

    local_devices
        .into_iter()
        .chain(neighbor_devices.into_iter().filter(|device| {
            !device
                .ipv4
                .iter()
                .chain(device.ipv6.iter())
                .any(|ip| local_ips.contains(ip))
        }))
        .collect()
}

fn discover_local_interface_devices() -> Vec<LanDevice> {
    let now = Utc::now().to_rfc3339();

    let Ok(interfaces) = list_afinet_netifas() else {
        return Vec::new();
    };
    let local_macs = local_interface_mac_map();

    let mut device = LanDevice {
        id: "local-machine".to_string(),
        display_name: "本机设备".to_string(),
        hostname: "本机设备".to_string(),
        mac: String::new(),
        ipv4: Vec::new(),
        ipv6: Vec::new(),
        global_ipv6: Vec::new(),
        online: true,
        source: "local-interface".to_string(),
        last_seen: now,
    };

    for (name, ip) in interfaces {
        if is_usable_ip(ip) {
            add_ip_to_device(&mut device, &ip.to_string());
        }
        if device.mac.is_empty() {
            if let Some(mac) = local_macs.get(&name).filter(|mac| is_usable_mac(mac)) {
                device.mac = mac.clone();
            }
        }
        if !name.trim().is_empty() && device.display_name == "本机设备" {
            device.display_name = format!("本机设备（{}）", name.trim());
            device.hostname = device.display_name.clone();
        }
    }

    if device.ipv4.is_empty() && device.ipv6.is_empty() {
        Vec::new()
    } else {
        vec![device]
    }
}

#[cfg(windows)]
fn local_interface_mac_map() -> HashMap<String, String> {
    use std::process::Command;

    let script =
        "Get-NetAdapter | Select-Object Name,MacAddress | ConvertTo-Json -Compress";
    let output = Command::new("powershell")
        .args(["-NoProfile", "-Command", script])
        .output();

    let stdout = output
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).to_string())
        .unwrap_or_default();

    parse_local_interface_mac_json(&stdout)
}

#[cfg(not(windows))]
fn local_interface_mac_map() -> HashMap<String, String> {
    HashMap::new()
}

fn parse_local_interface_mac_json(value: &str) -> HashMap<String, String> {
    let mut result = HashMap::new();
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return result;
    }

    let rows = match serde_json::from_str::<Value>(trimmed) {
        Ok(Value::Array(rows)) => rows,
        Ok(Value::Object(row)) => vec![Value::Object(row)],
        _ => Vec::new(),
    };

    for row in rows {
        let name = row
            .get("Name")
            .or_else(|| row.get("name"))
            .map(value_to_string)
            .unwrap_or_default();
        let mac = row
            .get("MacAddress")
            .or_else(|| row.get("macAddress"))
            .map(value_to_string)
            .and_then(|value| normalize_mac(&value));
        if !name.trim().is_empty() {
            if let Some(mac) = mac.filter(|mac| is_usable_mac(mac)) {
                result.insert(name, mac);
            }
        }
    }

    result
}

fn add_ip_to_device(device: &mut LanDevice, ip: &str) {
    match ip.parse::<IpAddr>() {
        Ok(IpAddr::V4(_)) => push_unique(&mut device.ipv4, ip.to_string()),
        Ok(IpAddr::V6(_)) => {
            push_unique(&mut device.ipv6, ip.to_string());
            if is_global_ipv6(ip) {
                push_unique(&mut device.global_ipv6, ip.to_string());
            }
        }
        Err(_) => {}
    }
}

fn normalize_mac(value: &str) -> Option<String> {
    let trimmed = value
        .trim()
        .trim_matches(|ch| matches!(ch, '(' | ')' | '[' | ']' | '<' | '>' | ','));

    if trimmed.contains(':') || trimmed.contains('-') {
        let parts: Vec<&str> = trimmed
            .split(|ch| matches!(ch, ':' | '-'))
            .filter(|part| !part.is_empty())
            .collect();

        if parts.len() == 6
            && parts
                .iter()
                .all(|part| part.len() <= 2 && part.chars().all(|ch| ch.is_ascii_hexdigit()))
        {
            let octets: Option<Vec<String>> = parts
                .iter()
                .map(|part| {
                    u8::from_str_radix(part, 16)
                        .ok()
                        .map(|octet| format!("{octet:02x}"))
                })
                .collect();

            if let Some(octets) = octets {
                return Some(octets.join(":"));
            }
        }
    }

    let hex: String = value
        .chars()
        .filter(|ch| ch.is_ascii_hexdigit())
        .map(|ch| ch.to_ascii_lowercase())
        .collect();

    if hex.len() != 12 {
        return None;
    }

    Some(
        hex.as_bytes()
            .chunks(2)
            .map(|chunk| std::str::from_utf8(chunk).unwrap_or_default())
            .collect::<Vec<_>>()
            .join(":"),
    )
}

fn is_usable_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            !ip.is_multicast()
                && !ip.is_unspecified()
                && !ip.is_loopback()
                && ip != Ipv4Addr::BROADCAST
        }
        IpAddr::V6(ip) => !ip.is_multicast() && !ip.is_unspecified() && !ip.is_loopback(),
    }
}

fn is_usable_mac(mac: &str) -> bool {
    !mac.starts_with("33:33:")
        && !mac.starts_with("01:00:5e:")
        && mac != "ff:ff:ff:ff:ff:ff"
        && mac != "00:00:00:00:00:00"
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.contains(&value) {
        values.push(value);
    }
}

fn sanitize_id_part(value: &str) -> String {
    let sanitized: String = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();

    sanitized.trim_matches('-').to_string()
}

fn value_to_string(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Number(value) => value.to_string(),
        Value::Bool(value) => value.to_string(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_device(
        id: &str,
        display_name: &str,
        source: &str,
        ipv4: Vec<&str>,
        ipv6: Vec<&str>,
    ) -> LanDevice {
        let ipv6 = ipv6
            .into_iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>();
        let global_ipv6 = ipv6
            .iter()
            .filter(|value| is_global_ipv6(value))
            .cloned()
            .collect::<Vec<_>>();

        LanDevice {
            id: id.to_string(),
            display_name: display_name.to_string(),
            hostname: display_name.to_string(),
            mac: String::new(),
            ipv4: ipv4.into_iter().map(|value| value.to_string()).collect(),
            ipv6,
            global_ipv6,
            online: true,
            source: source.to_string(),
            last_seen: "2026-05-19T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn classifies_global_ipv6_by_2000_prefix() {
        assert!(is_global_ipv6("2001:db8::1"));
        assert!(is_global_ipv6("3fff:ffff::1"));
        assert!(!is_global_ipv6("fe80::1"));
        assert!(!is_global_ipv6("fc00::1"));
        assert!(!is_global_ipv6("::1"));
        assert!(!is_global_ipv6("192.168.1.10"));
    }

    #[test]
    fn normalizes_common_mac_formats() {
        assert_eq!(
            normalize_mac("AA-BB-CC-DD-EE-FF"),
            Some("aa:bb:cc:dd:ee:ff".to_string())
        );
        assert_eq!(
            normalize_mac("aa:bb:cc:dd:ee:ff"),
            Some("aa:bb:cc:dd:ee:ff".to_string())
        );
        assert_eq!(
            normalize_mac("aabb.ccdd.eeff"),
            Some("aa:bb:cc:dd:ee:ff".to_string())
        );
        assert_eq!(normalize_mac(""), None);
        assert_eq!(normalize_mac("not-a-mac"), None);
    }

    #[test]
    fn normalizes_single_digit_mac_octets() {
        assert_eq!(
            normalize_mac("88:c9:b3:b3:2:58"),
            Some("88:c9:b3:b3:02:58".to_string())
        );
    }

    #[test]
    fn parses_macos_arp_and_ndp_neighbors_by_mac() {
        let arp = r#"
? (192.168.100.143) at 88:c9:b3:b3:2:58 on en0 ifscope [ethernet]
? (192.168.100.255) at ff:ff:ff:ff:ff:ff on en0 ifscope [ethernet]
"#;
        let ndp = r#"
Neighbor                             Linklayer Address  Netif Expire    S Flags
240e:358:abcd:1234:1111:2222:3333:4444 88:c9:b3:b3:2:58 en0 23h59m59s S R
fe80::1%en0                          88:c9:b3:b3:2:58 en0 23h59m59s S R
ff02::1                              33:33:00:00:00:01 en0 permanent  R
"#;

        let devices = merge_neighbor_rows(parse_macos_neighbor_tables(arp, ndp));

        assert_eq!(devices.len(), 1);
        let device = &devices[0];
        assert_eq!(device.mac, "88:c9:b3:b3:02:58");
        assert_eq!(device.ipv4, vec!["192.168.100.143"]);
        assert_eq!(
            device.ipv6,
            vec!["240e:358:abcd:1234:1111:2222:3333:4444", "fe80::1"]
        );
        assert_eq!(
            device.global_ipv6,
            vec!["240e:358:abcd:1234:1111:2222:3333:4444"]
        );
    }

    #[test]
    fn parses_single_powershell_json_object() {
        let json = r#"{
            "IPAddress":"192.168.1.42",
            "LinkLayerAddress":"AA-BB-CC-DD-EE-FF",
            "State":"Reachable",
            "InterfaceAlias":"Ethernet",
            "AddressFamily":"IPv4"
        }"#;

        let rows = parse_powershell_neighbor_json(json);

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].ip_address, "192.168.1.42");
        assert_eq!(
            rows[0].normalized_mac(),
            Some("aa:bb:cc:dd:ee:ff".to_string())
        );
    }

    #[test]
    fn merges_ipv4_and_ipv6_by_mac_and_keeps_only_global_ipv6() {
        let rows = vec![
            NeighborRow::new(
                "192.168.1.42",
                "AA-BB-CC-DD-EE-FF",
                "Reachable",
                "Ethernet",
                Some("IPv4"),
            ),
            NeighborRow::new(
                "fe80::abcd",
                "aa:bb:cc:dd:ee:ff",
                "Stale",
                "Ethernet",
                Some("IPv6"),
            ),
            NeighborRow::new(
                "2408:8200::1234",
                "aa-bb-cc-dd-ee-ff",
                "Reachable",
                "Ethernet",
                Some("IPv6"),
            ),
        ];

        let devices = merge_neighbor_rows(rows);

        assert_eq!(devices.len(), 1);
        let device = &devices[0];
        assert_eq!(device.mac, "aa:bb:cc:dd:ee:ff");
        assert_eq!(device.ipv4, vec!["192.168.1.42"]);
        assert_eq!(device.ipv6, vec!["fe80::abcd", "2408:8200::1234"]);
        assert_eq!(device.global_ipv6, vec!["2408:8200::1234"]);
        assert!(device.online);
        assert_eq!(device.source, "windows-neighbor");
    }

    #[test]
    fn ignores_ipv6_multicast_neighbor_rows() {
        let rows = vec![NeighborRow::new(
            "ff02::1",
            "33-33-00-00-00-01",
            "Reachable",
            "Ethernet",
            Some("IPv6"),
        )];

        assert!(merge_neighbor_rows(rows).is_empty());
    }

    #[test]
    fn ignores_ipv4_broadcast_neighbor_rows() {
        let rows = vec![NeighborRow::new(
            "255.255.255.255",
            "FF-FF-FF-FF-FF-FF",
            "Reachable",
            "Ethernet",
            Some("IPv4"),
        )];

        assert!(merge_neighbor_rows(rows).is_empty());
    }

    #[test]
    fn keeps_normal_private_ipv4_neighbor_rows() {
        let rows = vec![NeighborRow::new(
            "192.168.1.42",
            "AA-BB-CC-DD-EE-FF",
            "Reachable",
            "Ethernet",
            Some("IPv4"),
        )];

        let devices = merge_neighbor_rows(rows);

        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].mac, "aa:bb:cc:dd:ee:ff");
        assert_eq!(devices[0].ipv4, vec!["192.168.1.42"]);
    }

    #[test]
    fn parses_local_interface_mac_json() {
        let rows = parse_local_interface_mac_json(
            r#"[{"Name":"以太网","MacAddress":"AA-BB-CC-DD-EE-FF"}]"#,
        );

        assert_eq!(rows.get("以太网"), Some(&"aa:bb:cc:dd:ee:ff".to_string()));
    }

    #[test]
    fn merged_lan_devices_keep_local_machine_first() {
        let local = test_device(
            "local-machine",
            "本机设备（以太网）",
            "local-interface",
            vec!["192.168.1.20"],
            vec!["240e:35b::20"],
        );
        let router = test_device(
            "mac-aa-bb-cc-dd-ee-ff",
            "aa:bb:cc:dd:ee:ff",
            "windows-neighbor",
            vec!["192.168.1.1"],
            Vec::new(),
        );

        let devices = merge_local_and_neighbor_devices(vec![local], vec![router]);

        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].id, "local-machine");
        assert_eq!(devices[0].source, "local-interface");
        assert_eq!(devices[1].source, "windows-neighbor");
    }

    #[test]
    fn merged_lan_devices_drop_neighbor_duplicate_of_local_ip() {
        let local = test_device(
            "local-machine",
            "本机设备（以太网）",
            "local-interface",
            vec!["192.168.1.20"],
            Vec::new(),
        );
        let duplicate_neighbor = test_device(
            "mac-aa-bb-cc-dd-ee-ff",
            "aa:bb:cc:dd:ee:ff",
            "windows-neighbor",
            vec!["192.168.1.20"],
            Vec::new(),
        );

        let devices =
            merge_local_and_neighbor_devices(vec![local], vec![duplicate_neighbor]);

        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].id, "local-machine");
    }
}
