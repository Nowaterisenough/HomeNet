pub mod aliyun;

use crate::config::add_log;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv6Addr};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInterfaceInfo {
    pub name: String,
    pub ipv4: Vec<String>,
    pub ipv6: Vec<String>,
    pub has_global_ipv6: bool,
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
            let has_global_ipv6 = ipv6.iter().any(|addr| is_global_unicast_ipv6(addr));
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
fn is_global_unicast_ipv6(addr: &str) -> bool {
    addr.parse::<Ipv6Addr>()
        .map(|ipv6| is_global_unicast_ipv6_addr(&ipv6))
        .unwrap_or(false)
}

fn is_global_unicast_ipv6_addr(addr: &Ipv6Addr) -> bool {
    (addr.segments()[0] & 0xe000) == 0x2000
}
