use chrono::Utc;
use local_ip_address::list_afinet_netifas;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::collections::{BTreeSet, HashMap};
use std::io::ErrorKind;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, ToSocketAddrs, UdpSocket};
use std::thread;
use std::time::{Duration, Instant};

const ACTIVE_DISCOVERY_LIMIT: usize = 32;
const NETBIOS_QUERY_TIMEOUT: Duration = Duration::from_millis(450);
const LLMNR_DISCOVERY_WINDOW: Duration = Duration::from_millis(800);
const LLMNR_MULTICAST_ADDR: &str = "224.0.0.252:5355";

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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeviceDiscoveryHint {
    pub device_id: String,
    pub device_mac: String,
    pub device_name: String,
    pub selected_ip: String,
    pub selected_ipv6: String,
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
        is_usable_neighbor_state(&self.state)
            && self.ip_address.parse::<IpAddr>().is_ok_and(is_usable_ip)
            && self.normalized_mac().is_some_and(|mac| is_usable_mac(&mac))
    }
}

pub fn discover_lan_devices_with_hints(hints: &[DeviceDiscoveryHint]) -> Vec<LanDevice> {
    let local_devices = discover_local_interface_devices();
    let neighbor_devices = discover_neighbor_devices(hints);
    merge_local_and_neighbor_devices(local_devices, neighbor_devices)
}

pub fn is_global_ipv6(value: &str) -> bool {
    crate::ddns::is_global_unicast_ipv6(value)
}

#[cfg(windows)]
fn discover_neighbor_devices(hints: &[DeviceDiscoveryHint]) -> Vec<LanDevice> {
    let mut rows = read_windows_neighbor_rows();
    if refresh_stale_global_ipv6_neighbors(&rows) {
        rows = read_windows_neighbor_rows();
    }

    let mut devices = merge_neighbor_rows(rows);
    enrich_neighbor_devices_with_active_discovery(&mut devices, hints);
    devices
}

#[cfg(any(windows, test))]
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
fn discover_neighbor_devices(hints: &[DeviceDiscoveryHint]) -> Vec<LanDevice> {
    refresh_macos_ipv6_neighbors();

    let mut rows = read_macos_neighbor_rows();
    if refresh_stale_global_ipv6_neighbors(&rows) {
        rows = read_macos_neighbor_rows();
    }

    let mut devices = merge_neighbor_rows_with_source(rows, "macos-neighbor");
    enrich_neighbor_devices_with_active_discovery(&mut devices, hints);
    devices
}

#[cfg(windows)]
fn read_windows_neighbor_rows() -> Vec<NeighborRow> {
    let script = "Get-NetNeighbor -AddressFamily IPv4,IPv6 | Where-Object { $_.IPAddress -and $_.LinkLayerAddress -and $_.State -ne 'Unreachable' } | Select-Object IPAddress,LinkLayerAddress,@{Name='State';Expression={$_.State.ToString()}},InterfaceAlias,AddressFamily | ConvertTo-Json -Compress";
    let output = powershell_output(script);

    match output {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            parse_powershell_neighbor_json(&stdout)
        }
        _ => Vec::new(),
    }
}

#[cfg(target_os = "macos")]
fn read_macos_neighbor_rows() -> Vec<NeighborRow> {
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

    parse_macos_neighbor_tables(&arp_stdout, &ndp_stdout)
}

#[cfg(all(not(windows), not(target_os = "macos")))]
fn discover_neighbor_devices(_hints: &[DeviceDiscoveryHint]) -> Vec<LanDevice> {
    Vec::new()
}

#[cfg(any(windows, test))]
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

#[cfg(any(windows, test))]
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
        add_neighbor_ip_to_device(device, &row.ip_address, &row.state);
    }

    ordered_keys
        .into_iter()
        .filter_map(|key| devices_by_key.remove(&key))
        .collect()
}

fn enrich_neighbor_devices_with_active_discovery(
    devices: &mut [LanDevice],
    hints: &[DeviceDiscoveryHint],
) {
    apply_device_discovery_hints(devices, hints);
    refresh_hinted_device_ipv6_candidates(devices, hints);

    let targets = active_discovery_ipv4_targets(devices);
    if targets.is_empty() {
        return;
    }

    let hostnames_by_ip =
        query_netbios_hostnames(&targets.iter().map(|(_, ip)| *ip).collect::<Vec<Ipv4Addr>>());

    let mut llmnr_targets = Vec::new();
    for (device_index, ip) in targets {
        if let Some(hostname) = hostnames_by_ip.get(&ip) {
            set_device_hostname(&mut devices[device_index], hostname);
        }

        if let Some(hint) = matching_device_hint(&devices[device_index], hints) {
            let addresses = resolve_hint_ipv6_addresses(hint);
            add_resolved_ipv6_candidates_to_device(
                &mut devices[device_index],
                addresses,
                Some(hint),
            );
        }

        if let Some(hostname) = normalized_device_hostname(&devices[device_index]) {
            llmnr_targets.push((ip, hostname));
        }
    }

    let ipv6_by_ip = query_llmnr_aaaa(&llmnr_targets);
    for device in devices {
        let device_ipv4 = device
            .ipv4
            .iter()
            .filter_map(|value| value.parse::<Ipv4Addr>().ok())
            .collect::<Vec<_>>();
        for ip in device_ipv4 {
            let Some(addresses) = ipv6_by_ip.get(&ip) else {
                continue;
            };
            add_resolved_ipv6_candidates_to_device(device, addresses.iter().copied(), None);
        }
    }
}

fn refresh_hinted_device_ipv6_candidates(
    devices: &mut [LanDevice],
    hints: &[DeviceDiscoveryHint],
) {
    refresh_hinted_device_ipv6_candidates_with_resolver(
        devices,
        hints,
        resolve_hint_ipv6_addresses,
    );
}

fn refresh_hinted_device_ipv6_candidates_with_resolver<F>(
    devices: &mut [LanDevice],
    hints: &[DeviceDiscoveryHint],
    mut resolve_addresses: F,
) where
    F: FnMut(&DeviceDiscoveryHint) -> Vec<Ipv6Addr>,
{
    for device in devices {
        let Some(hint) = matching_device_hint(device, hints) else {
            continue;
        };
        if hint.device_name.trim().is_empty() {
            continue;
        }

        clear_global_ipv6_candidates(device);
        let addresses = resolve_addresses(hint);
        add_resolved_ipv6_candidates_to_device(device, addresses, Some(hint));
    }
}

fn clear_global_ipv6_candidates(device: &mut LanDevice) {
    device.ipv6.retain(|value| !is_global_ipv6(value));
    device.global_ipv6.clear();
}

fn apply_device_discovery_hints(devices: &mut [LanDevice], hints: &[DeviceDiscoveryHint]) {
    for device in devices {
        let Some(hint) = matching_device_hint(device, hints) else {
            continue;
        };

        set_device_hostname(device, &hint.device_name);
    }
}

fn matching_device_hint<'a>(
    device: &LanDevice,
    hints: &'a [DeviceDiscoveryHint],
) -> Option<&'a DeviceDiscoveryHint> {
    let device_id = device.id.trim();
    if !device_id.is_empty() {
        if let Some(hint) = hints
            .iter()
            .find(|hint| !hint.device_id.trim().is_empty() && hint.device_id.trim() == device_id)
        {
            return Some(hint);
        }
    }

    let device_mac = normalize_mac(&device.mac)?;
    hints
        .iter()
        .find(|hint| normalize_mac(&hint.device_mac).is_some_and(|mac| mac == device_mac))
}

fn resolve_hint_ipv6_addresses(hint: &DeviceDiscoveryHint) -> Vec<Ipv6Addr> {
    let mut addresses = Vec::new();
    for hostname in local_hostname_candidates(&hint.device_name) {
        push_unique_ipv6_values(&mut addresses, resolve_hostname_ipv6(&hostname));
    }
    order_resolved_ipv6_candidates(addresses, Some(hint))
}

fn local_hostname_candidates(hostname: &str) -> Vec<String> {
    let Some(hostname) = normalize_hostname(hostname) else {
        return Vec::new();
    };

    let mut candidates = Vec::new();
    if let Some(base_name) = hostname.strip_suffix("-local") {
        if !base_name.trim().is_empty() && !base_name.contains('.') {
            push_unique_case_insensitive(&mut candidates, format!("{base_name}.local"));
            push_unique_case_insensitive(&mut candidates, base_name.to_string());
        }
    }
    if hostname.contains('.') {
        push_unique_case_insensitive(&mut candidates, hostname);
        return candidates;
    }

    push_unique_case_insensitive(&mut candidates, format!("{hostname}.local"));
    push_unique_case_insensitive(&mut candidates, hostname);
    candidates
}

fn resolve_hostname_ipv6(hostname: &str) -> Vec<Ipv6Addr> {
    let mut addresses = Vec::new();
    if let Ok(socket_addrs) = (hostname, 0).to_socket_addrs() {
        push_unique_ipv6_values(
            &mut addresses,
            socket_addrs.filter_map(|addr| match addr.ip() {
                IpAddr::V6(ipv6) => Some(ipv6),
                IpAddr::V4(_) => None,
            }),
        );
    }

    #[cfg(target_os = "macos")]
    push_unique_ipv6_values(&mut addresses, resolve_macos_dscacheutil_ipv6(hostname));

    addresses
}

#[cfg(target_os = "macos")]
fn resolve_macos_dscacheutil_ipv6(hostname: &str) -> Vec<Ipv6Addr> {
    std::process::Command::new("dscacheutil")
        .args(["-q", "host", "-a", "name", hostname])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| {
            parse_macos_dscacheutil_ipv6_addresses(&String::from_utf8_lossy(&output.stdout))
        })
        .unwrap_or_default()
}

#[cfg(any(target_os = "macos", test))]
fn parse_macos_dscacheutil_ipv6_addresses(value: &str) -> Vec<Ipv6Addr> {
    value
        .lines()
        .filter_map(|line| line.trim().strip_prefix("ipv6_address:"))
        .filter_map(|value| value.trim().parse::<Ipv6Addr>().ok())
        .collect()
}

fn add_resolved_ipv6_candidates_to_device<I>(
    device: &mut LanDevice,
    addresses: I,
    hint: Option<&DeviceDiscoveryHint>,
) where
    I: IntoIterator<Item = Ipv6Addr>,
{
    for address in order_resolved_ipv6_candidates(addresses, hint) {
        add_resolved_ipv6_to_device(device, address);
    }
}

fn order_resolved_ipv6_candidates<I>(
    addresses: I,
    hint: Option<&DeviceDiscoveryHint>,
) -> Vec<Ipv6Addr>
where
    I: IntoIterator<Item = Ipv6Addr>,
{
    let selected = hint.and_then(selected_hint_ipv6);
    let mut candidates = addresses
        .into_iter()
        .filter(|address| is_global_ipv6(&address.to_string()))
        .enumerate()
        .collect::<Vec<_>>();

    candidates.sort_by_key(|(index, address)| {
        (
            resolved_ipv6_candidate_score(address, selected.as_ref()),
            *index,
        )
    });

    let mut ordered = Vec::new();
    for (_, address) in candidates {
        if !ordered.contains(&address) {
            ordered.push(address);
        }
    }
    ordered
}

fn resolved_ipv6_candidate_score(address: &Ipv6Addr, selected: Option<&Ipv6Addr>) -> u8 {
    if low_ipv6_interface_identifier(address) {
        return 0;
    }
    if selected.is_some_and(|selected| same_ipv6_interface_identifier(address, selected)) {
        return 1;
    }
    2
}

fn selected_hint_ipv6(hint: &DeviceDiscoveryHint) -> Option<Ipv6Addr> {
    let selected = if hint.selected_ip.trim().is_empty() {
        hint.selected_ipv6.trim()
    } else {
        hint.selected_ip.trim()
    };
    selected.parse::<Ipv6Addr>().ok()
}

fn same_ipv6_interface_identifier(left: &Ipv6Addr, right: &Ipv6Addr) -> bool {
    ipv6_interface_identifier(left) == ipv6_interface_identifier(right)
}

fn low_ipv6_interface_identifier(address: &Ipv6Addr) -> bool {
    let value = ipv6_interface_identifier(address);
    value > 0 && value <= 0xffff
}

fn ipv6_interface_identifier(address: &Ipv6Addr) -> u64 {
    let octets = address.octets();
    u64::from_be_bytes([
        octets[8], octets[9], octets[10], octets[11], octets[12], octets[13], octets[14],
        octets[15],
    ])
}

fn push_unique_ipv6_values<I>(values: &mut Vec<Ipv6Addr>, next_values: I)
where
    I: IntoIterator<Item = Ipv6Addr>,
{
    for value in next_values {
        if !values.contains(&value) {
            values.push(value);
        }
    }
}

fn push_unique_case_insensitive(values: &mut Vec<String>, value: String) {
    if !values
        .iter()
        .any(|existing| existing.eq_ignore_ascii_case(&value))
    {
        values.push(value);
    }
}

fn active_discovery_ipv4_targets(devices: &[LanDevice]) -> Vec<(usize, Ipv4Addr)> {
    let mut targets = Vec::new();

    for (device_index, device) in devices.iter().enumerate() {
        if !device.global_ipv6.is_empty() {
            continue;
        }

        for ip in device
            .ipv4
            .iter()
            .filter_map(|value| value.parse::<Ipv4Addr>().ok())
        {
            if targets.iter().any(|(_, existing)| *existing == ip) {
                continue;
            }
            targets.push((device_index, ip));
            if targets.len() >= ACTIVE_DISCOVERY_LIMIT {
                return targets;
            }
        }
    }

    targets
}

fn set_device_hostname(device: &mut LanDevice, hostname: &str) {
    let Some(hostname) = normalize_hostname(hostname) else {
        return;
    };

    if device.hostname.trim().is_empty() {
        device.hostname = hostname.clone();
    }

    let display_is_mac = normalize_mac(&device.display_name).is_some()
        || (!device.mac.is_empty() && device.display_name.eq_ignore_ascii_case(&device.mac));
    let display_is_ip = device.display_name.parse::<IpAddr>().is_ok();
    if device.display_name.trim().is_empty() || display_is_mac || display_is_ip {
        device.display_name = hostname;
    }
}

fn normalized_device_hostname(device: &LanDevice) -> Option<String> {
    normalize_hostname(&device.hostname).or_else(|| normalize_hostname(&device.display_name))
}

fn normalize_hostname(value: &str) -> Option<String> {
    let hostname = value.trim().trim_end_matches('.').to_string();
    if hostname.is_empty() {
        return None;
    }
    if hostname.parse::<IpAddr>().is_ok() || normalize_mac(&hostname).is_some() {
        return None;
    }
    if hostname
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        Some(hostname)
    } else {
        None
    }
}

fn add_resolved_ipv6_to_device(device: &mut LanDevice, address: Ipv6Addr) {
    if !is_usable_ip(IpAddr::V6(address)) {
        return;
    }

    let value = address.to_string();
    push_unique(&mut device.ipv6, value.clone());
    if is_global_ipv6(&value) {
        push_unique(&mut device.global_ipv6, value);
    }
}

fn query_netbios_hostnames(ips: &[Ipv4Addr]) -> HashMap<Ipv4Addr, String> {
    let mut unique_ips = Vec::new();
    for ip in ips.iter().copied() {
        if !unique_ips.contains(&ip) {
            unique_ips.push(ip);
            if unique_ips.len() >= ACTIVE_DISCOVERY_LIMIT {
                break;
            }
        }
    }

    let handles = unique_ips
        .into_iter()
        .map(|ip| thread::spawn(move || (ip, query_netbios_hostname(ip))))
        .collect::<Vec<_>>();

    let mut result = HashMap::new();
    for handle in handles {
        if let Ok((ip, Some(hostname))) = handle.join() {
            result.insert(ip, hostname);
        }
    }

    result
}

fn query_netbios_hostname(ip: Ipv4Addr) -> Option<String> {
    let socket = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    socket.set_read_timeout(Some(NETBIOS_QUERY_TIMEOUT)).ok()?;
    socket.set_write_timeout(Some(NETBIOS_QUERY_TIMEOUT)).ok()?;

    let transaction_id = netbios_transaction_id(ip);
    let packet = build_netbios_node_status_query(transaction_id);
    socket
        .send_to(&packet, SocketAddrV4::new(ip, 137))
        .ok()
        .filter(|bytes| *bytes == packet.len())?;

    let mut buffer = [0_u8; 1024];
    loop {
        match socket.recv_from(&mut buffer) {
            Ok((length, SocketAddr::V4(source))) if *source.ip() == ip => {
                if let Some(hostname) = parse_netbios_node_status_hostname(&buffer[..length]) {
                    return Some(hostname);
                }
            }
            Ok(_) => continue,
            Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                return None;
            }
            Err(_) => return None,
        }
    }
}

fn netbios_transaction_id(ip: Ipv4Addr) -> u16 {
    let octets = ip.octets();
    u16::from_be_bytes([octets[2], octets[3]]) ^ 0x4e42
}

fn build_netbios_node_status_query(transaction_id: u16) -> Vec<u8> {
    let mut packet = Vec::with_capacity(50);
    packet.extend_from_slice(&transaction_id.to_be_bytes());
    packet.extend_from_slice(&0_u16.to_be_bytes()); // query flags
    packet.extend_from_slice(&1_u16.to_be_bytes()); // QDCOUNT
    packet.extend_from_slice(&0_u16.to_be_bytes()); // ANCOUNT
    packet.extend_from_slice(&0_u16.to_be_bytes()); // NSCOUNT
    packet.extend_from_slice(&0_u16.to_be_bytes()); // ARCOUNT
    packet.push(32);
    packet.extend_from_slice(&encode_netbios_name("*"));
    packet.push(0);
    packet.extend_from_slice(&0x0021_u16.to_be_bytes()); // NBSTAT
    packet.extend_from_slice(&0x0001_u16.to_be_bytes()); // IN
    packet
}

fn encode_netbios_name(name: &str) -> [u8; 32] {
    let mut raw_name = [b' '; 16];
    for (index, byte) in name.as_bytes().iter().copied().take(15).enumerate() {
        raw_name[index] = byte.to_ascii_uppercase();
    }

    let mut encoded = [0_u8; 32];
    for (index, byte) in raw_name.iter().copied().enumerate() {
        encoded[index * 2] = b'A' + ((byte >> 4) & 0x0f);
        encoded[index * 2 + 1] = b'A' + (byte & 0x0f);
    }
    encoded
}

fn parse_netbios_node_status_hostname(packet: &[u8]) -> Option<String> {
    for offset in 0..packet.len() {
        let name_count = *packet.get(offset)? as usize;
        if name_count == 0 || name_count > 32 {
            continue;
        }

        let table_start = offset + 1;
        let table_len = name_count.checked_mul(18)?;
        if table_start + table_len > packet.len() {
            continue;
        }

        let mut fallback = None;
        for entry in packet[table_start..table_start + table_len].chunks_exact(18) {
            let suffix = entry[15];
            let flags = u16::from_be_bytes([entry[16], entry[17]]);
            let is_group_name = flags & 0x8000 != 0;
            let Some(name) = normalize_netbios_entry_name(&entry[..15]) else {
                continue;
            };

            if suffix == 0x00 && !is_group_name {
                return Some(name);
            }
            if fallback.is_none() && !is_group_name && suffix != 0x1e {
                fallback = Some(name);
            }
        }

        if fallback.is_some() {
            return fallback;
        }
    }

    None
}

fn normalize_netbios_entry_name(raw: &[u8]) -> Option<String> {
    let name = String::from_utf8(
        raw.iter()
            .copied()
            .take_while(|byte| *byte != 0)
            .collect::<Vec<_>>(),
    )
    .ok()?
    .trim()
    .to_string();

    normalize_hostname(&name).filter(|value| value != "*")
}

fn query_llmnr_aaaa(targets: &[(Ipv4Addr, String)]) -> HashMap<Ipv4Addr, Vec<Ipv6Addr>> {
    let socket = match UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0)) {
        Ok(socket) => socket,
        Err(_) => return HashMap::new(),
    };
    socket
        .set_read_timeout(Some(Duration::from_millis(120)))
        .ok();
    socket
        .set_write_timeout(Some(Duration::from_millis(120)))
        .ok();
    socket.set_multicast_ttl_v4(1).ok();

    let mut transaction_ids = Vec::new();
    let mut seen_hostnames = Vec::new();
    for hostname in targets
        .iter()
        .filter_map(|(_, hostname)| normalize_hostname(hostname))
    {
        let lookup_name = llmnr_lookup_name(&hostname);
        if seen_hostnames
            .iter()
            .any(|existing: &String| existing.eq_ignore_ascii_case(&lookup_name))
        {
            continue;
        }
        if seen_hostnames.len() >= ACTIVE_DISCOVERY_LIMIT {
            break;
        }

        let transaction_id = llmnr_transaction_id(&lookup_name, seen_hostnames.len());
        let packet = build_llmnr_query(transaction_id, &lookup_name);
        if packet.is_empty() {
            continue;
        }
        if socket.send_to(&packet, LLMNR_MULTICAST_ADDR).is_ok() {
            transaction_ids.push(transaction_id);
            seen_hostnames.push(lookup_name);
        }
    }

    if transaction_ids.is_empty() {
        return HashMap::new();
    }

    let target_ips = targets.iter().map(|(ip, _)| *ip).collect::<Vec<_>>();
    let deadline = Instant::now() + LLMNR_DISCOVERY_WINDOW;
    let mut buffer = [0_u8; 1500];
    let mut result: HashMap<Ipv4Addr, Vec<Ipv6Addr>> = HashMap::new();

    while Instant::now() < deadline {
        match socket.recv_from(&mut buffer) {
            Ok((length, SocketAddr::V4(source))) => {
                if !target_ips.contains(source.ip()) {
                    continue;
                }
                let Some((transaction_id, addresses)) =
                    parse_llmnr_aaaa_response(&buffer[..length])
                else {
                    continue;
                };
                if !transaction_ids.contains(&transaction_id) {
                    continue;
                }

                let entry = result.entry(*source.ip()).or_default();
                for address in addresses {
                    if !entry.contains(&address) {
                        entry.push(address);
                    }
                }
            }
            Ok(_) => continue,
            Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                continue;
            }
            Err(_) => break,
        }
    }

    result
}

fn llmnr_lookup_name(hostname: &str) -> String {
    hostname
        .split('.')
        .next()
        .unwrap_or(hostname)
        .trim()
        .to_string()
}

fn llmnr_transaction_id(hostname: &str, index: usize) -> u16 {
    hostname
        .as_bytes()
        .iter()
        .fold(0x4c4d_u16 ^ index as u16, |acc, byte| {
            acc.rotate_left(5) ^ u16::from(byte.to_ascii_lowercase())
        })
}

fn build_llmnr_query(transaction_id: u16, hostname: &str) -> Vec<u8> {
    let Some(labels) = dns_labels(hostname) else {
        return Vec::new();
    };

    let mut packet = Vec::with_capacity(64);
    packet.extend_from_slice(&transaction_id.to_be_bytes());
    packet.extend_from_slice(&0_u16.to_be_bytes()); // query flags
    packet.extend_from_slice(&1_u16.to_be_bytes()); // QDCOUNT
    packet.extend_from_slice(&0_u16.to_be_bytes()); // ANCOUNT
    packet.extend_from_slice(&0_u16.to_be_bytes()); // NSCOUNT
    packet.extend_from_slice(&0_u16.to_be_bytes()); // ARCOUNT
    write_dns_name(&mut packet, &labels);
    packet.extend_from_slice(&28_u16.to_be_bytes()); // AAAA
    packet.extend_from_slice(&1_u16.to_be_bytes()); // IN
    packet
}

fn dns_labels(hostname: &str) -> Option<Vec<String>> {
    let labels = hostname
        .trim()
        .trim_end_matches('.')
        .split('.')
        .filter(|label| !label.is_empty())
        .map(|label| label.to_string())
        .collect::<Vec<_>>();

    if labels.is_empty()
        || labels
            .iter()
            .any(|label| label.len() > 63 || !normalize_hostname(label).is_some())
    {
        None
    } else {
        Some(labels)
    }
}

fn write_dns_name(packet: &mut Vec<u8>, labels: &[String]) {
    for label in labels {
        packet.push(label.len() as u8);
        packet.extend_from_slice(label.as_bytes());
    }
    packet.push(0);
}

fn parse_llmnr_aaaa_response(packet: &[u8]) -> Option<(u16, Vec<Ipv6Addr>)> {
    if packet.len() < 12 {
        return None;
    }

    let transaction_id = u16::from_be_bytes([packet[0], packet[1]]);
    let flags = u16::from_be_bytes([packet[2], packet[3]]);
    if flags & 0x8000 == 0 {
        return None;
    }

    let question_count = u16::from_be_bytes([packet[4], packet[5]]) as usize;
    let answer_count = u16::from_be_bytes([packet[6], packet[7]]) as usize;
    let authority_count = u16::from_be_bytes([packet[8], packet[9]]) as usize;
    let additional_count = u16::from_be_bytes([packet[10], packet[11]]) as usize;

    let mut offset = 12;
    for _ in 0..question_count {
        offset = skip_dns_name(packet, offset)?;
        offset = offset.checked_add(4)?;
        if offset > packet.len() {
            return None;
        }
    }

    let mut addresses = Vec::new();
    for _ in 0..answer_count + authority_count + additional_count {
        offset = skip_dns_name(packet, offset)?;
        if offset + 10 > packet.len() {
            return None;
        }

        let record_type = u16::from_be_bytes([packet[offset], packet[offset + 1]]);
        let record_class = u16::from_be_bytes([packet[offset + 2], packet[offset + 3]]);
        let data_len = u16::from_be_bytes([packet[offset + 8], packet[offset + 9]]) as usize;
        offset += 10;

        if offset + data_len > packet.len() {
            return None;
        }

        if record_type == 28 && (record_class & 0x7fff) == 1 && data_len == 16 {
            let mut octets = [0_u8; 16];
            octets.copy_from_slice(&packet[offset..offset + 16]);
            let address = Ipv6Addr::from(octets);
            if !addresses.contains(&address) {
                addresses.push(address);
            }
        }

        offset += data_len;
    }

    Some((transaction_id, addresses))
}

fn skip_dns_name(packet: &[u8], offset: usize) -> Option<usize> {
    let mut cursor = offset;
    let mut jumps = 0;

    loop {
        let length = *packet.get(cursor)?;
        if length & 0xc0 == 0xc0 {
            packet.get(cursor + 1)?;
            cursor += 2;
            return Some(cursor);
        }

        if length == 0 {
            return Some(cursor + 1);
        }

        if length & 0xc0 != 0 {
            return None;
        }

        cursor = cursor.checked_add(1 + length as usize)?;
        if cursor > packet.len() {
            return None;
        }

        jumps += 1;
        if jumps > 128 {
            return None;
        }
    }
}

fn refresh_stale_global_ipv6_neighbors(rows: &[NeighborRow]) -> bool {
    let targets = rows
        .iter()
        .filter(|row| is_global_ipv6(&row.ip_address) && !is_current_neighbor_state(&row.state))
        .filter_map(|row| row.ip_address.parse::<Ipv6Addr>().ok())
        .take(8)
        .collect::<Vec<_>>();

    if targets.is_empty() {
        return false;
    }

    for target in targets {
        ping_ipv6_once(target);
    }

    true
}

#[cfg(windows)]
fn ping_ipv6_once(address: Ipv6Addr) {
    use std::os::windows::process::CommandExt;

    let value = address.to_string();
    let _ = std::process::Command::new("ping")
        .args(["-6", "-n", "1", "-w", "700", &value])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
}

#[cfg(target_os = "macos")]
fn ping_ipv6_once(address: Ipv6Addr) {
    let value = address.to_string();
    let _ = std::process::Command::new("ping6")
        .args(["-c", "1", "-W", "700", &value])
        .output();
}

#[cfg(all(not(windows), not(target_os = "macos")))]
fn ping_ipv6_once(_address: Ipv6Addr) {}

#[cfg(target_os = "macos")]
fn refresh_macos_ipv6_neighbors() {
    use std::process::Command;

    let interfaces = list_afinet_netifas()
        .ok()
        .unwrap_or_default()
        .into_iter()
        .filter(|(_, ip)| matches!(ip, IpAddr::V4(ipv4) if is_usable_ip(IpAddr::V4(*ipv4))))
        .map(|(name, _)| name)
        .collect::<Vec<_>>();

    let mut seen = Vec::new();
    for interface in interfaces {
        if seen.contains(&interface) {
            continue;
        }
        seen.push(interface.clone());
        if seen.len() > 4 {
            break;
        }

        let _ = Command::new("ping6")
            .args(["-c", "1", "-I", &interface, "ff02::1"])
            .output();
    }
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
    let remaining_tokens = tokens.collect::<Vec<_>>();
    let state = if remaining_tokens
        .iter()
        .any(|token| token.eq_ignore_ascii_case("expired"))
    {
        "Stale"
    } else {
        "Reachable"
    };

    Some(NeighborRow {
        ip_address,
        link_layer_address: link_layer_address.to_string(),
        state: state.to_string(),
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
    let local_macs = local_devices
        .iter()
        .map(|device| device.mac.trim().to_ascii_lowercase())
        .filter(|mac| !mac.is_empty())
        .collect::<Vec<_>>();

    local_devices
        .into_iter()
        .chain(neighbor_devices.into_iter().filter(|device| {
            let duplicate_ip = device
                .ipv4
                .iter()
                .chain(device.ipv6.iter())
                .any(|ip| local_ips.contains(ip));
            let device_mac = device.mac.trim().to_ascii_lowercase();
            let duplicate_mac = !device_mac.is_empty() && local_macs.contains(&device_mac);

            !duplicate_ip && !duplicate_mac
        }))
        .collect()
}

fn discover_local_interface_devices() -> Vec<LanDevice> {
    let now = Utc::now().to_rfc3339();

    let Ok(interfaces) = list_afinet_netifas() else {
        return Vec::new();
    };
    let local_macs = local_interface_mac_map();
    let stable_local_ipv6 = crate::ddns::stable_local_ipv6_candidate_set();

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
            add_local_interface_ip_to_device(&mut device, &name, ip, stable_local_ipv6.as_ref());
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

fn add_local_interface_ip_to_device(
    device: &mut LanDevice,
    interface_name: &str,
    ip: IpAddr,
    stable_local_ipv6: Option<&BTreeSet<(String, Ipv6Addr)>>,
) {
    match ip {
        IpAddr::V4(ipv4) => push_unique(&mut device.ipv4, ipv4.to_string()),
        IpAddr::V6(ipv6) => {
            let value = ipv6.to_string();
            push_unique(&mut device.ipv6, value.clone());

            let is_stable_ddns_ipv6 = match stable_local_ipv6 {
                Some(stable_local_ipv6) => {
                    stable_local_ipv6.contains(&(interface_name.trim().to_ascii_lowercase(), ipv6))
                }
                None => is_global_ipv6(&value),
            };

            if is_stable_ddns_ipv6 {
                push_unique(&mut device.global_ipv6, value);
            }
        }
    }
}

#[cfg(windows)]
fn local_interface_mac_map() -> HashMap<String, String> {
    let script = "Get-NetAdapter | Select-Object Name,MacAddress | ConvertTo-Json -Compress";
    let output = powershell_output(script);

    let stdout = output
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).to_string())
        .unwrap_or_default();

    parse_local_interface_mac_json(&stdout)
}

#[cfg(target_os = "macos")]
fn local_interface_mac_map() -> HashMap<String, String> {
    use std::process::Command;

    let stdout = Command::new("ifconfig")
        .arg("-a")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).to_string())
        .unwrap_or_default();

    parse_macos_ifconfig_mac_map(&stdout)
}

#[cfg(all(not(windows), not(target_os = "macos")))]
fn local_interface_mac_map() -> HashMap<String, String> {
    HashMap::new()
}

#[cfg(any(target_os = "macos", test))]
fn parse_macos_ifconfig_mac_map(value: &str) -> HashMap<String, String> {
    let mut result = HashMap::new();
    let mut current_interface = String::new();

    for line in value.lines() {
        if let Some(interface_name) = parse_macos_ifconfig_interface_name(line) {
            current_interface = interface_name;
            continue;
        }

        if current_interface.is_empty() {
            continue;
        }

        let mut tokens = line.split_whitespace();
        if tokens.next() != Some("ether") {
            continue;
        }

        let Some(mac) = tokens
            .next()
            .and_then(normalize_mac)
            .filter(|mac| is_usable_mac(mac))
        else {
            continue;
        };

        result.insert(current_interface.clone(), mac);
    }

    result
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

fn add_neighbor_ip_to_device(device: &mut LanDevice, ip: &str, state: &str) {
    match ip.parse::<IpAddr>() {
        Ok(IpAddr::V4(_)) => push_unique(&mut device.ipv4, ip.to_string()),
        Ok(IpAddr::V6(_)) => {
            push_unique(&mut device.ipv6, ip.to_string());
            if is_global_ipv6(ip) && is_current_neighbor_state(state) {
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

fn is_usable_neighbor_state(value: &str) -> bool {
    !matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "unreachable" | "incomplete" | "0" | "1"
    )
}

fn is_current_neighbor_state(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "reachable" | "delay" | "probe" | "permanent" | "2" | "3" | "5" | "6"
    )
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

    fn netbios_status_entry(name: &str, suffix: u8, flags: u16) -> [u8; 18] {
        let mut entry = [b' '; 18];
        for (index, byte) in name.as_bytes().iter().copied().take(15).enumerate() {
            entry[index] = byte;
        }
        entry[15] = suffix;
        entry[16..18].copy_from_slice(&flags.to_be_bytes());
        entry
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
    fn parses_netbios_node_status_workstation_hostname() {
        let mut packet = vec![0_u8; 12];
        packet.push(2);
        packet.extend_from_slice(&netbios_status_entry("WINBOX", 0x00, 0x0000));
        packet.extend_from_slice(&netbios_status_entry("WORKGROUP", 0x00, 0x8000));

        assert_eq!(
            parse_netbios_node_status_hostname(&packet),
            Some("WINBOX".to_string())
        );
    }

    #[test]
    fn parses_llmnr_aaaa_answers() {
        let query = build_llmnr_query(0x1234, "WINBOX");
        let mut response = Vec::new();
        response.extend_from_slice(&0x1234_u16.to_be_bytes());
        response.extend_from_slice(&0x8000_u16.to_be_bytes());
        response.extend_from_slice(&1_u16.to_be_bytes());
        response.extend_from_slice(&1_u16.to_be_bytes());
        response.extend_from_slice(&0_u16.to_be_bytes());
        response.extend_from_slice(&0_u16.to_be_bytes());
        response.extend_from_slice(&query[12..]);
        response.extend_from_slice(&[0xc0, 0x0c]);
        response.extend_from_slice(&28_u16.to_be_bytes());
        response.extend_from_slice(&1_u16.to_be_bytes());
        response.extend_from_slice(&30_u32.to_be_bytes());
        response.extend_from_slice(&16_u16.to_be_bytes());
        response.extend_from_slice(&"2408:8200::1234".parse::<Ipv6Addr>().unwrap().octets());

        let (transaction_id, addresses) = parse_llmnr_aaaa_response(&response).unwrap();

        assert_eq!(transaction_id, 0x1234);
        assert_eq!(
            addresses,
            vec!["2408:8200::1234".parse::<Ipv6Addr>().unwrap()]
        );
    }

    #[test]
    fn parses_macos_dscacheutil_ipv6_addresses() {
        let output = r#"
name: nowatspc.local
ipv6_address: 240e:358:106:9200::612
ipv6_address: 240e:358:106:9200:8ebc:1f1:c0b2:f655
ip_address: 192.168.100.143
"#;

        assert_eq!(
            parse_macos_dscacheutil_ipv6_addresses(output),
            vec![
                "240e:358:106:9200::612".parse::<Ipv6Addr>().unwrap(),
                "240e:358:106:9200:8ebc:1f1:c0b2:f655"
                    .parse::<Ipv6Addr>()
                    .unwrap()
            ]
        );
    }

    #[test]
    fn resolved_hint_ipv6_candidates_prefer_same_stable_host_suffix() {
        let hint = DeviceDiscoveryHint {
            device_mac: "88:c9:b3:b3:02:58".to_string(),
            device_name: "NoWatsPC".to_string(),
            selected_ip: "240e:358:13e:3a00::612".to_string(),
            ..DeviceDiscoveryHint::default()
        };
        let addresses = vec![
            "240e:358:106:9200:8ebc:1f1:c0b2:f655"
                .parse::<Ipv6Addr>()
                .unwrap(),
            "240e:358:106:9200::612".parse::<Ipv6Addr>().unwrap(),
        ];

        assert_eq!(
            order_resolved_ipv6_candidates(addresses, Some(&hint)),
            vec![
                "240e:358:106:9200::612".parse::<Ipv6Addr>().unwrap(),
                "240e:358:106:9200:8ebc:1f1:c0b2:f655"
                    .parse::<Ipv6Addr>()
                    .unwrap()
            ]
        );
    }

    #[test]
    fn resolved_hint_ipv6_candidates_prefer_low_stable_host_when_old_was_temporary() {
        let hint = DeviceDiscoveryHint {
            device_mac: "88:c9:b3:b3:02:58".to_string(),
            device_name: "NoWatsPC".to_string(),
            selected_ip: "240e:358:13e:3a00:96d9:1e6:a9fd:c969".to_string(),
            ..DeviceDiscoveryHint::default()
        };
        let addresses = vec![
            "240e:358:106:9200:8ebc:1f1:c0b2:f655"
                .parse::<Ipv6Addr>()
                .unwrap(),
            "240e:358:106:9200::612".parse::<Ipv6Addr>().unwrap(),
        ];

        assert_eq!(
            order_resolved_ipv6_candidates(addresses, Some(&hint))
                .first()
                .copied(),
            Some("240e:358:106:9200::612".parse::<Ipv6Addr>().unwrap())
        );
    }

    #[test]
    fn resolved_hint_ipv6_candidates_prefer_low_stable_host_over_same_old_random_address() {
        let hint = DeviceDiscoveryHint {
            device_mac: "88:c9:b3:b3:02:58".to_string(),
            device_name: "NoWatsPC".to_string(),
            selected_ip: "240e:358:130:7f00:fa35:5668:3636:1b1f".to_string(),
            ..DeviceDiscoveryHint::default()
        };
        let addresses = vec![
            "240e:358:130:7f00:fa35:5668:3636:1b1f"
                .parse::<Ipv6Addr>()
                .unwrap(),
            "240e:358:130:7f00::612".parse::<Ipv6Addr>().unwrap(),
        ];

        assert_eq!(
            order_resolved_ipv6_candidates(addresses, Some(&hint))
                .first()
                .copied(),
            Some("240e:358:130:7f00::612".parse::<Ipv6Addr>().unwrap())
        );
    }

    #[test]
    fn local_hostname_candidates_accepts_dash_local_alias() {
        assert_eq!(
            local_hostname_candidates("NoWatsPC-local"),
            vec![
                "NoWatsPC.local",
                "NoWatsPC",
                "NoWatsPC-local.local",
                "NoWatsPC-local"
            ]
        );
    }

    #[test]
    fn hinted_device_refresh_replaces_stale_global_ipv6_candidates() {
        let mut devices = vec![test_device(
            "mac-88-c9-b3-b3-02-58",
            "88:c9:b3:b3:02:58",
            "windows-neighbor",
            vec!["192.168.100.143"],
            vec![
                "240e:358:130:7f00:fa35:5668:3636:1b1f",
                "fe80::5d21:b00:d0aa:b8db",
            ],
        )];
        devices[0].mac = "88:c9:b3:b3:02:58".to_string();
        let hint = DeviceDiscoveryHint {
            device_mac: "88:c9:b3:b3:02:58".to_string(),
            device_name: "NoWatsPC".to_string(),
            selected_ip: "240e:358:130:7f00:fa35:5668:3636:1b1f".to_string(),
            ..DeviceDiscoveryHint::default()
        };

        refresh_hinted_device_ipv6_candidates_with_resolver(
            &mut devices,
            &[hint],
            |_| {
                vec![
                    "240e:358:1f9:b700:8e7:364e:ff6a:7ec"
                        .parse::<Ipv6Addr>()
                        .unwrap(),
                    "240e:358:130:7f00::612".parse::<Ipv6Addr>().unwrap(),
                ]
            },
        );

        assert_eq!(
            devices[0].ipv6,
            vec![
                "fe80::5d21:b00:d0aa:b8db",
                "240e:358:130:7f00::612",
                "240e:358:1f9:b700:8e7:364e:ff6a:7ec"
            ]
        );
        assert_eq!(
            devices[0].global_ipv6,
            vec![
                "240e:358:130:7f00::612",
                "240e:358:1f9:b700:8e7:364e:ff6a:7ec"
            ]
        );
    }

    #[test]
    fn hinted_device_refresh_drops_stale_global_ipv6_when_name_resolution_fails() {
        let mut devices = vec![test_device(
            "device-1",
            "NoWatsPC",
            "windows-neighbor",
            vec!["192.168.100.143"],
            vec![
                "240e:358:130:7f00:fa35:5668:3636:1b1f",
                "fe80::5d21:b00:d0aa:b8db",
            ],
        )];
        let hint = DeviceDiscoveryHint {
            device_id: "device-1".to_string(),
            device_name: "NoWatsPC".to_string(),
            ..DeviceDiscoveryHint::default()
        };

        refresh_hinted_device_ipv6_candidates_with_resolver(&mut devices, &[hint], |_| {
            Vec::new()
        });

        assert_eq!(devices[0].ipv6, vec!["fe80::5d21:b00:d0aa:b8db"]);
        assert!(devices[0].global_ipv6.is_empty());
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
240e:358:abcd:1234:9999:8888:7777:6666 88:c9:b3:b3:2:58 en0 expired S
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
            vec![
                "240e:358:abcd:1234:1111:2222:3333:4444",
                "240e:358:abcd:1234:9999:8888:7777:6666",
                "fe80::1"
            ]
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
    fn stale_neighbor_ipv6_is_not_a_ddns_candidate_but_device_stays_online() {
        let rows = vec![
            NeighborRow::new(
                "192.168.100.143",
                "88-C9-B3-B3-02-58",
                "Reachable",
                "Ethernet",
                Some("IPv4"),
            ),
            NeighborRow::new(
                "240e:358:13e:3a00::612",
                "88-C9-B3-B3-02-58",
                "Stale",
                "Ethernet",
                Some("IPv6"),
            ),
            NeighborRow::new(
                "240e:358:13e:3a00:96d9:1e6:a9fd:c969",
                "88-C9-B3-B3-02-58",
                "Reachable",
                "Ethernet",
                Some("IPv6"),
            ),
        ];

        let devices = merge_neighbor_rows(rows);

        assert_eq!(devices.len(), 1);
        assert!(devices[0].online);
        assert_eq!(
            devices[0].ipv6,
            vec![
                "240e:358:13e:3a00::612",
                "240e:358:13e:3a00:96d9:1e6:a9fd:c969"
            ]
        );
        assert_eq!(
            devices[0].global_ipv6,
            vec!["240e:358:13e:3a00:96d9:1e6:a9fd:c969"]
        );
    }

    #[test]
    fn local_interface_device_keeps_only_stable_ddns_ipv6_candidates() {
        let mut device = LanDevice {
            id: "local-machine".to_string(),
            display_name: "local".to_string(),
            hostname: "local".to_string(),
            mac: String::new(),
            ipv4: Vec::new(),
            ipv6: Vec::new(),
            global_ipv6: Vec::new(),
            online: true,
            source: "local-interface".to_string(),
            last_seen: "2026-05-19T00:00:00Z".to_string(),
        };
        let mut stable = BTreeSet::new();
        stable.insert((
            "en0".to_string(),
            "2408:8200::2".parse::<Ipv6Addr>().unwrap(),
        ));

        add_local_interface_ip_to_device(
            &mut device,
            "en0",
            "2408:8200::1".parse::<IpAddr>().unwrap(),
            Some(&stable),
        );
        add_local_interface_ip_to_device(
            &mut device,
            "en0",
            "2408:8200::2".parse::<IpAddr>().unwrap(),
            Some(&stable),
        );

        assert_eq!(device.ipv6, vec!["2408:8200::1", "2408:8200::2"]);
        assert_eq!(device.global_ipv6, vec!["2408:8200::2"]);
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
    fn parses_macos_ifconfig_mac_addresses_by_interface_name() {
        let output = r#"
en0: flags=8863<UP,BROADCAST,SMART,RUNNING,SIMPLEX,MULTICAST> mtu 1500
	options=6460<TSO4,TSO6,CHANNEL_IO,PARTIAL_CSUM,ZEROINVERT_CSUM>
	ether 88:c9:b3:b3:2:58
	inet6 fe80::1234%en0 prefixlen 64 secured scopeid 0xb
	inet 192.168.100.143 netmask 0xffffff00 broadcast 192.168.100.255
	status: active
awdl0: flags=8943<UP,BROADCAST,RUNNING,PROMISC,SIMPLEX,MULTICAST> mtu 1484
	ether 36:7f:4a:11:22:33
	status: active
lo0: flags=8049<UP,LOOPBACK,RUNNING,MULTICAST> mtu 16384
	inet 127.0.0.1 netmask 0xff000000
"#;

        let rows = parse_macos_ifconfig_mac_map(output);

        assert_eq!(rows.get("en0"), Some(&"88:c9:b3:b3:02:58".to_string()));
        assert_eq!(rows.get("awdl0"), Some(&"36:7f:4a:11:22:33".to_string()));
        assert!(!rows.contains_key("lo0"));
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

        let devices = merge_local_and_neighbor_devices(vec![local], vec![duplicate_neighbor]);

        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].id, "local-machine");
    }

    #[test]
    fn merged_lan_devices_drop_neighbor_duplicate_of_local_mac() {
        let mut local = test_device(
            "local-machine",
            "local",
            "local-interface",
            vec!["192.168.100.143"],
            vec!["240e:358:13e:3a00::7a7"],
        );
        local.mac = "88:c9:b3:b3:02:58".to_string();

        let mut stale_neighbor = test_device(
            "mac-88-c9-b3-b3-02-58",
            "88:c9:b3:b3:02:58",
            "windows-neighbor",
            Vec::new(),
            vec!["240e:358:13e:3a00::612"],
        );
        stale_neighbor.mac = "88:c9:b3:b3:02:58".to_string();

        let devices = merge_local_and_neighbor_devices(vec![local], vec![stale_neighbor]);

        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].id, "local-machine");
        assert_eq!(devices[0].global_ipv6, vec!["240e:358:13e:3a00::7a7"]);
    }

    #[test]
    fn powershell_args_request_hidden_window() {
        assert_eq!(
            powershell_args("$PSVersionTable.PSVersion"),
            [
                "-NoProfile",
                "-WindowStyle",
                "Hidden",
                "-Command",
                "$PSVersionTable.PSVersion"
            ]
        );
    }
}
