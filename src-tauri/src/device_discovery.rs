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
    let neighbor_devices = discover_neighbor_devices();
    if neighbor_devices.is_empty() {
        discover_local_interface_devices()
    } else {
        neighbor_devices
    }
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

#[cfg(not(windows))]
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
                source: "windows-neighbor".to_string(),
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

fn discover_local_interface_devices() -> Vec<LanDevice> {
    let now = Utc::now().to_rfc3339();
    let mut devices_by_name: HashMap<String, LanDevice> = HashMap::new();
    let mut ordered_names = Vec::new();

    let Ok(interfaces) = list_afinet_netifas() else {
        return Vec::new();
    };

    for (name, ip) in interfaces {
        let device = devices_by_name.entry(name.clone()).or_insert_with(|| {
            ordered_names.push(name.clone());
            LanDevice {
                id: format!("interface-{}", sanitize_id_part(&name)),
                display_name: name.clone(),
                hostname: name.clone(),
                mac: String::new(),
                ipv4: Vec::new(),
                ipv6: Vec::new(),
                global_ipv6: Vec::new(),
                online: true,
                source: "local-interface".to_string(),
                last_seen: now.clone(),
            }
        });

        add_ip_to_device(device, &ip.to_string());
    }

    ordered_names
        .into_iter()
        .filter_map(|name| devices_by_name.remove(&name))
        .collect()
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
}
