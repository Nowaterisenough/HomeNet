use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

const PROBE_BUDGET: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Subnet {
    address: Ipv4Addr,
    prefix: u8,
}

impl Subnet {
    fn new(address: Ipv4Addr, prefix: u8) -> Option<Self> {
        if !(8..=30).contains(&prefix) {
            return None;
        }
        let subnet = Self { address, prefix };
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

pub(super) fn refresh_ipv4_neighbors() -> Result<(), String> {
    static CURSORS: OnceLock<Mutex<HashMap<Subnet, u32>>> = OnceLock::new();
    let Ok(mut cursors) = CURSORS.get_or_init(Mutex::default).try_lock() else {
        return Ok(());
    };
    let subnets = local_subnets();
    cursors.retain(|subnet, _| subnets.contains(subnet));
    let budget = PROBE_BUDGET / subnets.len().max(1);
    let mut sent = false;
    let mut last_error = None;
    for subnet in subnets {
        let probe = || -> std::io::Result<UdpSocket> {
            let socket = UdpSocket::bind(SocketAddrV4::new(subnet.address, 0))?;
            socket.set_nonblocking(true)?;
            socket.set_ttl(1)?;
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
        for address in probe_targets(subnet, cursor, budget) {
            if let Err(error) = socket.send_to(&[], SocketAddrV4::new(address, 9)) {
                let denied = error.kind() == std::io::ErrorKind::PermissionDenied;
                last_error = Some(error);
                if denied {
                    crate::config::add_log(
                        "warn",
                        "设备发现",
                        "局域网主动探测被拒绝，请检查本地网络权限",
                    );
                    break;
                }
            } else {
                sent = true;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    if sent {
        std::thread::sleep(Duration::from_millis(500));
    } else if let Some(error) = last_error {
        return Err(format!(
            "局域网主动探测未能发送，请检查本地网络权限、网卡和 VPN 设置：{error}"
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn local_subnets() -> Vec<Subnet> {
    let output = std::process::Command::new("/sbin/ifconfig")
        .arg("-a")
        .output();
    let text = super::discovery_command_stdout("/sbin/ifconfig", output);
    parse_macos_subnets(&text)
}

#[cfg(any(target_os = "macos", test))]
fn parse_macos_subnets(text: &str) -> Vec<Subnet> {
    let mut eligible = false;
    let mut result = Vec::new();
    for line in text.lines() {
        if let Some(name) = super::parse_macos_ifconfig_interface_name(line) {
            eligible = super::is_likely_physical_lan_interface(&name)
                && line.split(['<', '>', ',']).any(|flag| flag == "UP")
                && !line.contains("POINTOPOINT");
            continue;
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
            if let Some(subnet) = Subnet::new(address, prefix) {
                if !result.contains(&subnet) {
                    result.push(subnet);
                }
            }
        }
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
}
