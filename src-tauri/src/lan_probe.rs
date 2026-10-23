use std::collections::HashMap;
use std::io::{self, ErrorKind};
use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};
use std::num::NonZeroU32;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

const PROBE_BUDGET: usize = 1024;

#[cfg(any(target_os = "macos", test))]
#[path = "lan_echo.rs"]
mod lan_echo;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Subnet {
    address: Ipv4Addr,
    prefix: u8,
    interface_index: Option<NonZeroU32>,
}

impl Subnet {
    fn new(address: Ipv4Addr, prefix: u8) -> Option<Self> {
        if !(8..=30).contains(&prefix) {
            return None;
        }
        let subnet = Self {
            address,
            prefix,
            interface_index: None,
        };
        let (network, broadcast) = subnet.bounds();
        let local = |ip: Ipv4Addr| ip.is_private() || ip.is_link_local();
        (local(Ipv4Addr::from(network)) && local(Ipv4Addr::from(broadcast))).then_some(subnet)
    }

    fn bounds(self) -> (u32, u32) {
        let mask = u32::MAX << (32 - self.prefix);
        let network = u32::from(self.address) & mask;
        (network, network | !mask)
    }

    fn initial_cursor(self) -> u32 {
        u32::from(self.address).saturating_sub(self.bounds().0 + 1)
    }

    #[cfg(any(target_os = "macos", test))]
    fn broadcast(self) -> Ipv4Addr {
        Ipv4Addr::from(self.bounds().1)
    }
}

fn bound_probe_socket(subnet: Subnet) -> io::Result<UdpSocket> {
    #[cfg(target_os = "macos")]
    {
        bound_macos_socket(subnet, socket2::Protocol::UDP)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let socket = UdpSocket::bind(SocketAddrV4::new(subnet.address, 0))?;
        socket.set_ttl(1)?;
        Ok(socket)
    }
}

#[cfg(target_os = "macos")]
fn bound_macos_socket(subnet: Subnet, protocol: socket2::Protocol) -> io::Result<UdpSocket> {
    let index = subnet
        .interface_index
        .ok_or_else(|| io::Error::new(ErrorKind::AddrNotAvailable, "未取得局域网网卡索引"))?;
    let socket = socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::DGRAM, Some(protocol))?;
    // IP_BOUND_IF prevents the VPN default route from capturing local probes.
    socket.bind_device_by_index_v4(Some(index))?;
    socket.bind(&SocketAddrV4::new(subnet.address, 0).into())?;
    let socket: UdpSocket = socket.into();
    socket.set_ttl(1)?;
    Ok(socket)
}

fn probe_error_message(operation: &str, error: &io::Error) -> String {
    let advice = match error.kind() {
        ErrorKind::PermissionDenied => "被系统拒绝，请检查本地网络权限或防火墙",
        ErrorKind::HostUnreachable | ErrorKind::NetworkUnreachable | ErrorKind::NetworkDown => {
            "路由不可达，请检查网卡连接和 VPN 的局域网访问设置"
        }
        ErrorKind::AddrNotAvailable => "无法绑定本地网卡地址，请检查网卡连接后刷新",
        _ => "失败",
    };
    format!("{operation}{advice}：{error}")
}

fn probe_targets(subnet: Subnet, cursor: &mut u32, budget: usize) -> Vec<Ipv4Addr> {
    let (network, broadcast) = subnet.bounds();
    let hosts = broadcast - network - 1;
    let mut targets = Vec::new();
    for _ in 0..hosts.min(budget as u32) {
        let address = Ipv4Addr::from(network + 1 + *cursor % hosts);
        *cursor = (*cursor + 1) % hosts;
        if address != subnet.address {
            targets.push(address);
        }
    }
    targets
}

pub(super) fn refresh_ipv4_neighbors() -> Result<Vec<Ipv4Addr>, String> {
    static CURSORS: OnceLock<Mutex<HashMap<Subnet, u32>>> = OnceLock::new();
    let Ok(mut cursors) = CURSORS.get_or_init(Mutex::default).try_lock() else {
        return Ok(Vec::new());
    };
    let subnets = local_subnets();
    cursors.retain(|subnet, _| subnets.contains(subnet));
    let budget = PROBE_BUDGET / subnets.len().max(1);
    let mut sent = false;
    let mut last_error = None;
    let mut reachable = std::collections::BTreeSet::new();
    #[cfg(target_os = "macos")]
    let (mut echo_completed, mut echo_error) = (false, None);
    for subnet in subnets {
        let probe = || -> io::Result<UdpSocket> {
            let socket = bound_probe_socket(subnet)?;
            socket.set_nonblocking(true)?;
            Ok(socket)
        };
        let socket = match probe() {
            Ok(socket) => socket,
            Err(error) => {
                last_error = Some(error);
                continue;
            }
        };
        let cursor = cursors
            .entry(subnet)
            .or_insert_with(|| subnet.initial_cursor());
        // A unicast datagram resolves ARP even when the device ignores ICMP/NetBIOS.
        // Respect real interface masks, never probe routed/public networks, and
        // rotate larger subnets over subsequent scheduled or manual scans.
        let targets = probe_targets(subnet, cursor, budget);
        for address in &targets {
            if let Err(error) = socket.send_to(&[], SocketAddrV4::new(*address, 9)) {
                let denied = error.kind() == std::io::ErrorKind::PermissionDenied;
                last_error = Some(error);
                if denied {
                    break;
                }
            } else {
                sent = true;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        #[cfg(target_os = "macos")]
        match bound_macos_socket(subnet, socket2::Protocol::ICMPV4)
            .and_then(|socket| lan_echo::discover(socket, &targets))
        {
            Ok(result) => {
                echo_completed = true;
                crate::config::add_log(
                    "info",
                    "设备发现",
                    &format!(
                        "网卡索引 {}：{}",
                        subnet
                            .interface_index
                            .map(NonZeroU32::get)
                            .unwrap_or_default(),
                        result.summary
                    ),
                );
                reachable.extend(result.addresses);
            }
            Err(error) => {
                crate::config::add_log(
                    "info",
                    "设备发现",
                    &format!(
                        "网卡索引 {}：{}",
                        subnet
                            .interface_index
                            .map(NonZeroU32::get)
                            .unwrap_or_default(),
                        probe_error_message("回包探测", &error)
                    ),
                );
                echo_error = Some(error);
            }
        }
    }
    #[cfg(target_os = "macos")]
    if !echo_completed {
        if let Some(error) = echo_error {
            return Err(probe_error_message("局域网回包探测", &error));
        }
    }
    if sent || !reachable.is_empty() {
        std::thread::sleep(Duration::from_millis(500));
    } else if let Some(error) = last_error {
        return Err(probe_error_message("局域网主动探测", &error));
    }
    Ok(reachable.into_iter().collect())
}

#[cfg(any(target_os = "macos", test))]
fn record_probe_failure(
    failures: &mut HashMap<Subnet, ErrorKind>,
    subnet: Subnet,
    error: Option<ErrorKind>,
) -> bool {
    match error {
        Some(kind) => failures.insert(subnet, kind) != Some(kind),
        None => {
            failures.remove(&subnet);
            false
        }
    }
}

#[cfg(target_os = "macos")]
pub(super) fn refresh_macos_broadcast_neighbors() {
    static FAILURES: OnceLock<Mutex<HashMap<Subnet, ErrorKind>>> = OnceLock::new();
    let Ok(mut failures) = FAILURES.get_or_init(Mutex::default).try_lock() else {
        return;
    };
    let subnets = local_subnets();
    failures.retain(|subnet, _| subnets.contains(subnet));
    for subnet in subnets {
        let result = probe_macos_broadcast(subnet);
        if record_probe_failure(
            &mut failures,
            subnet,
            result.as_ref().err().map(io::Error::kind),
        ) {
            let error = result.unwrap_err();
            crate::config::add_log(
                "warn",
                "设备发现",
                &format!(
                    "{}；已继续其他设备发现路径",
                    probe_error_message("局域网辅助广播", &error)
                ),
            );
        }
    }
}

#[cfg(target_os = "macos")]
fn probe_macos_broadcast(subnet: Subnet) -> io::Result<()> {
    use std::time::Instant;
    let socket = bound_probe_socket(subnet)?;
    socket.set_broadcast(true)?;
    socket.set_write_timeout(Some(super::NETBIOS_QUERY_TIMEOUT))?;
    let target = subnet.broadcast();
    let mut packet = super::build_netbios_node_status_query(super::netbios_transaction_id(target));
    packet[3] |= 0x10;
    socket.send_to(&packet, SocketAddrV4::new(target, 137))?;
    let deadline = Instant::now() + super::NETBIOS_QUERY_TIMEOUT;
    let mut buffer = [0_u8; 1024];
    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
        socket.set_read_timeout(Some(remaining.max(Duration::from_millis(1))))?;
        match socket.recv_from(&mut buffer) {
            Ok(_) => {}
            Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                break
            }
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn local_subnets() -> Vec<Subnet> {
    let output = std::process::Command::new("/sbin/ifconfig")
        .args(["-v", "-a"])
        .output();
    let text = super::discovery_command_stdout("/sbin/ifconfig", output);
    parse_macos_subnets(&text)
}

#[cfg(any(target_os = "macos", test))]
fn parse_macos_subnets(text: &str) -> Vec<Subnet> {
    let mut eligible = false;
    let mut interface_index = None;
    let mut pending = Vec::new();
    let mut result = Vec::new();
    for line in text.lines() {
        if let Some(name) = super::parse_macos_ifconfig_interface_name(line) {
            if eligible {
                result.append(&mut pending);
            } else {
                pending.clear();
            }
            let flags: Vec<_> = line
                .split(['<', '>'])
                .nth(1)
                .unwrap_or_default()
                .split(',')
                .collect();
            eligible = super::is_likely_physical_lan_interface(&name)
                && ["UP", "RUNNING", "BROADCAST"]
                    .iter()
                    .all(|flag| flags.contains(flag))
                && !flags.contains(&"POINTOPOINT");
            let tokens: Vec<_> = line.split_whitespace().collect();
            interface_index = tokens
                .windows(2)
                .find(|pair| pair[0] == "index")
                .and_then(|pair| pair[1].parse::<u32>().ok())
                .and_then(NonZeroU32::new);
            continue;
        }
        if line.trim().starts_with("status:") && line.trim() != "status: active" {
            eligible = false;
        }
        if !eligible {
            continue;
        }
        let tokens: Vec<_> = line.split_whitespace().collect();
        if tokens.first() != Some(&"inet") {
            continue;
        }
        let address = tokens
            .get(1)
            .and_then(|value| value.parse::<Ipv4Addr>().ok());
        let mask = tokens
            .windows(2)
            .find(|pair| pair[0] == "netmask")
            .and_then(|pair| u32::from_str_radix(pair[1].trim_start_matches("0x"), 16).ok());
        if let (Some(address), Some(mask)) = (address, mask) {
            let prefix = mask.leading_ones() as u8;
            if prefix == 0 || mask != u32::MAX << (32 - prefix) {
                continue;
            }
            if let Some(mut subnet) = Subnet::new(address, prefix) {
                subnet.interface_index = interface_index;
                if !pending.contains(&subnet) {
                    pending.push(subnet);
                }
            }
        }
    }
    if eligible {
        result.append(&mut pending);
    }
    result
}

#[cfg(windows)]
fn local_subnets() -> Vec<Subnet> {
    let script = "Get-NetAdapter -Physical | Where-Object Status -eq 'Up' | ForEach-Object { Get-NetIPAddress -InterfaceIndex $_.ifIndex -AddressFamily IPv4 -ErrorAction SilentlyContinue } | Where-Object AddressState -eq 'Preferred' | Select-Object IPAddress,PrefixLength | ConvertTo-Json -Compress";
    super::powershell_output(script)
        .ok()
        .filter(|output| output.status.success())
        .map(|output| parse_windows_subnets(&String::from_utf8_lossy(&output.stdout)))
        .unwrap_or_default()
}

#[cfg(any(windows, test))]
fn parse_windows_subnets(text: &str) -> Vec<Subnet> {
    let rows = match serde_json::from_str::<serde_json::Value>(text) {
        Ok(serde_json::Value::Array(rows)) => rows,
        Ok(row @ serde_json::Value::Object(_)) => vec![row],
        _ => return Vec::new(),
    };
    let mut result = Vec::new();
    for row in rows {
        let address = row["IPAddress"]
            .as_str()
            .and_then(|value| value.parse().ok());
        let prefix = row["PrefixLength"]
            .as_u64()
            .and_then(|value| u8::try_from(value).ok());
        if let (Some(address), Some(prefix)) = (address, prefix) {
            if let Some(subnet) = Subnet::new(address, prefix) {
                if !result.contains(&subnet) {
                    result.push(subnet);
                }
            }
        }
    }
    result
}

#[cfg(all(not(windows), not(target_os = "macos")))]
fn local_subnets() -> Vec<Subnet> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subnet_probe_uses_real_mask_not_a_fixed_slash_24() {
        let subnet = Subnet::new("192.168.2.10".parse().unwrap(), 23).unwrap();
        let mut cursor = subnet.initial_cursor();
        let targets = probe_targets(subnet, &mut cursor, PROBE_BUDGET);
        assert_eq!(targets.len(), 509);
        assert!(targets.contains(&"192.168.3.254".parse().unwrap()));
        assert!(!targets.contains(&subnet.address));
        assert!(!targets.contains(&"192.168.2.0".parse().unwrap()));
        assert!(!targets.contains(&"192.168.3.255".parse().unwrap()));
    }

    #[test]
    fn large_subnets_rotate_without_exceeding_the_probe_budget() {
        let subnet = Subnet::new("10.0.8.20".parse().unwrap(), 16).unwrap();
        let mut cursor = subnet.initial_cursor();
        let first = probe_targets(subnet, &mut cursor, PROBE_BUDGET);
        let second = probe_targets(subnet, &mut cursor, PROBE_BUDGET);
        assert!(first.len() <= PROBE_BUDGET && second.len() <= PROBE_BUDGET);
        assert!(!second.iter().any(|ip| first.contains(ip)));
    }

    #[test]
    fn public_loopback_and_masks_extending_beyond_lan_are_not_probed() {
        for (address, prefix) in [
            ("8.8.8.8", 24),
            ("127.0.0.1", 8),
            ("192.168.1.1", 8),
            ("10.0.0.1", 0),
            ("10.0.0.1", 33),
        ] {
            assert!(Subnet::new(address.parse().unwrap(), prefix).is_none());
        }
    }

    #[test]
    fn macos_subnets_exclude_down_tunnel_and_invalid_mask_interfaces() {
        let text = "en0: flags=8863<UP,BROADCAST,RUNNING>\n inet 192.168.8.5 netmask 0xfffffe00 broadcast 192.168.9.255\nen1: flags=8802<BROADCAST>\n inet 10.0.0.1 netmask 0xffffff00\nutun0: flags=8051<UP,POINTOPOINT,RUNNING>\n inet 10.1.0.1 netmask 0xffffff00\nen2: flags=8863<UP,BROADCAST,RUNNING>\n inet 192.168.0.1 netmask 0xff00ff00";
        assert_eq!(
            parse_macos_subnets(text),
            vec![Subnet::new("192.168.8.5".parse().unwrap(), 23).unwrap()]
        );
    }

    #[test]
    fn windows_subnets_accept_single_or_multiple_addresses() {
        let one = r#"{"IPAddress":"192.168.1.9","PrefixLength":24}"#;
        assert_eq!(parse_windows_subnets(one).len(), 1);
        assert_eq!(
            parse_windows_subnets(&format!(
                "[{one},{one},{{\"IPAddress\":\"8.8.8.8\",\"PrefixLength\":24}}]"
            ))
            .len(),
            1
        );
    }

    #[test]
    fn broadcasts_follow_each_subnet_mask_instead_of_the_global_route() {
        for (address, prefix, expected) in [
            ("192.168.8.5", 23, "192.168.9.255"),
            ("10.2.3.7", 24, "10.2.3.255"),
        ] {
            let subnet = Subnet::new(address.parse().unwrap(), prefix).unwrap();
            assert_eq!(subnet.broadcast(), expected.parse::<Ipv4Addr>().unwrap());
            assert_ne!(subnet.broadcast(), Ipv4Addr::BROADCAST);
        }
    }

    #[test]
    fn macos_verbose_interfaces_keep_the_index_and_discard_inactive_links() {
        let text = "en0: flags=8863<UP,BROADCAST,RUNNING> mtu 1500 index 17\n inet 192.168.8.5 netmask 0xffffff00\n status: active\nen1: flags=8863<UP,BROADCAST,RUNNING> mtu 1500 index 18\n inet 10.0.0.2 netmask 0xffffff00\n status: inactive\nen2: flags=8863<UP,BROADCAST> mtu 1500 index 19\n inet 10.1.0.2 netmask 0xffffff00\nen3: flags=8863<UP,BROADCAST,RUNNING> mtu 1500 index 20\n status: inactive\n inet 10.2.0.2 netmask 0xffffff00\nutun4: flags=8051<UP,POINTOPOINT,RUNNING> mtu 1500 index 21\n inet 10.3.0.2 netmask 0xffffff00";
        let subnets = parse_macos_subnets(text);
        assert_eq!(subnets.len(), 1);
        assert_eq!(subnets[0].interface_index, NonZeroU32::new(17));
        assert_eq!(
            subnets[0].address,
            "192.168.8.5".parse::<Ipv4Addr>().unwrap()
        );
    }

    #[test]
    fn route_failures_are_not_mislabeled_as_permission_denials() {
        for kind in [
            ErrorKind::HostUnreachable,
            ErrorKind::NetworkUnreachable,
            ErrorKind::NetworkDown,
        ] {
            let message = probe_error_message("探测", &io::Error::new(kind, "fixture"));
            assert!(message.contains("路由不可达"));
            assert!(!message.contains("权限"));
        }
        assert!(probe_error_message(
            "探测",
            &io::Error::new(ErrorKind::PermissionDenied, "fixture")
        )
        .contains("本地网络权限"));
        assert!(probe_error_message(
            "探测",
            &io::Error::new(ErrorKind::AddrNotAvailable, "fixture")
        )
        .contains("本地网卡地址"));
    }

    #[test]
    fn unchanged_auxiliary_failures_are_reported_once_until_recovery() {
        let mut failures = HashMap::new();
        let first = Subnet::new("192.168.1.10".parse().unwrap(), 24).unwrap();
        let second = Subnet::new("192.168.2.10".parse().unwrap(), 24).unwrap();
        assert!(record_probe_failure(
            &mut failures,
            first,
            Some(ErrorKind::HostUnreachable)
        ));
        assert!(!record_probe_failure(
            &mut failures,
            first,
            Some(ErrorKind::HostUnreachable)
        ));
        assert!(record_probe_failure(
            &mut failures,
            second,
            Some(ErrorKind::HostUnreachable)
        ));
        assert!(record_probe_failure(
            &mut failures,
            first,
            Some(ErrorKind::PermissionDenied)
        ));
        assert!(!record_probe_failure(&mut failures, first, None));
        assert!(record_probe_failure(
            &mut failures,
            first,
            Some(ErrorKind::PermissionDenied)
        ));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn darwin_error_65_has_route_specific_guidance() {
        let message = probe_error_message("局域网辅助广播", &io::Error::from_raw_os_error(65));
        assert!(message.contains("路由不可达"));
        assert!(!message.contains("权限"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn missing_interface_index_never_falls_back_to_the_default_route() {
        let subnet = Subnet {
            address: Ipv4Addr::LOCALHOST,
            prefix: 8,
            interface_index: None,
        };
        let error = bound_probe_socket(subnet).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::AddrNotAvailable);
        assert!(error.to_string().contains("网卡索引"));
    }
}
