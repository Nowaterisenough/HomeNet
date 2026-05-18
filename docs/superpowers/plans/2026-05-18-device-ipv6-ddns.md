# Device IPv6 DDNS Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add one device-level IPv6 DDNS binding so a selected LAN device with global IPv6 can update an Aliyun AAAA record, while keeping the forwarding panel unchanged.

**Architecture:** Add a Rust discovery module that reads LAN neighbor data, a separate persisted `DeviceDdnsConfig`, Tauri commands for discovery/config/update, and a new Vue `DeviceDdnsPanel`. The main layout adds this panel beside the existing DDNS panel without editing `src/components/ForwardRulesPanel.vue`.

**Tech Stack:** Rust 2021, Tauri 2 commands, Serde/TOML config, existing Aliyun DDNS client, Vue 3 `<script setup>`, TypeScript, Vite, lucide-vue.

---

## File Structure

- Create `src-tauri/src/device_discovery.rs`: LAN device model, Windows neighbor-table discovery, pure merge and IPv6 classification helpers.
- Modify `src-tauri/src/config.rs`: add `DeviceDdnsConfig` and persist it under `AppConfig.device_ddns`.
- Modify `src-tauri/src/commands.rs`: add device discovery, device DDNS config, current record, and update commands.
- Modify `src-tauri/src/lib.rs`: register new module/commands and start a device DDNS background task.
- Modify `src/types.ts`: add `LanDevice` and `DeviceDdnsConfig`.
- Create `src/components/DeviceDdnsPanel.vue`: device list, selected device, and device DDNS form.
- Modify `src/App.vue`: import the new panel and adjust the left DDNS area layout.
- Modify `src/components/DdnsPanel.vue`: compact only height/spacing styles so it fits under the new device panel; keep command calls and fields.
- Do not modify `src/components/ForwardRulesPanel.vue`.

## Tasks

### Task 1: Config Model For Device DDNS

**Files:**
- Modify: `src-tauri/src/config.rs`

- [ ] **Step 1: Add the persisted model**

Add this struct below `DdnsConfig`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceDdnsConfig {
    #[serde(default)]
    pub enabled: bool,

    #[serde(default = "default_provider")]
    pub provider: String,

    #[serde(default)]
    pub access_key_id: String,

    #[serde(default)]
    pub access_key_secret: String,

    #[serde(default)]
    pub domain: String,

    #[serde(default)]
    pub sub_domain: String,

    #[serde(default = "default_ttl")]
    pub ttl: u32,

    #[serde(default = "default_interval_minutes")]
    pub interval_minutes: u32,

    #[serde(default)]
    pub device_id: String,

    #[serde(default)]
    pub device_mac: String,

    #[serde(default)]
    pub device_name: String,

    #[serde(default)]
    pub selected_ipv6: String,

    #[serde(default)]
    pub last_update_time: String,

    #[serde(default)]
    pub last_result: String,
}
```

- [ ] **Step 2: Add it to `AppConfig`**

Add this field after `ddns`:

```rust
#[serde(default)]
pub device_ddns: DeviceDdnsConfig,
```

Update `AppConfig::default()`:

```rust
device_ddns: DeviceDdnsConfig::default(),
```

- [ ] **Step 3: Add the default implementation**

Add this implementation below `impl Default for DdnsConfig`:

```rust
impl Default for DeviceDdnsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: default_provider(),
            access_key_id: String::new(),
            access_key_secret: String::new(),
            domain: String::new(),
            sub_domain: String::new(),
            ttl: default_ttl(),
            interval_minutes: default_interval_minutes(),
            device_id: String::new(),
            device_mac: String::new(),
            device_name: String::new(),
            selected_ipv6: String::new(),
            last_update_time: String::new(),
            last_result: String::new(),
        }
    }
}
```

- [ ] **Step 4: Run config tests**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml config
```

Expected: command exits 0. If no config tests exist yet, Cargo reports 0 matching tests and exits 0.

- [ ] **Step 5: Commit**

```powershell
git add src-tauri/src/config.rs
git commit -m "feat(ddns): 增加设备级 DDNS 配置模型"
```

### Task 2: LAN Device Discovery Module

**Files:**
- Create: `src-tauri/src/device_discovery.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Create the module with model and pure helpers**

Create `src-tauri/src/device_discovery.rs` with this base implementation:

```rust
use chrono::Local;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv6Addr};
use std::process::Command;

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

#[derive(Debug, Clone, Deserialize)]
struct NeighborRow {
    #[serde(rename = "IPAddress")]
    ip_address: String,
    #[serde(rename = "LinkLayerAddress")]
    link_layer_address: Option<String>,
    #[serde(rename = "State")]
    state: Option<String>,
    #[serde(rename = "InterfaceAlias")]
    interface_alias: Option<String>,
    #[serde(rename = "AddressFamily")]
    address_family: Option<String>,
}

#[derive(Debug, Default)]
struct DeviceAccumulator {
    mac: String,
    hostname: String,
    ipv4: Vec<String>,
    ipv6: Vec<String>,
    online: bool,
    source: String,
}

pub fn discover_lan_devices() -> Vec<LanDevice> {
    let rows = discover_neighbor_rows();
    if rows.is_empty() {
        return local_interface_devices();
    }
    merge_neighbor_rows(rows)
}

fn discover_neighbor_rows() -> Vec<NeighborRow> {
    #[cfg(target_os = "windows")]
    {
        return discover_windows_neighbors();
    }

    #[cfg(not(target_os = "windows"))]
    {
        Vec::new()
    }
}

#[cfg(target_os = "windows")]
fn discover_windows_neighbors() -> Vec<NeighborRow> {
    let script = "$ErrorActionPreference='SilentlyContinue'; \
Get-NetNeighbor -AddressFamily IPv4,IPv6 | \
Where-Object { $_.IPAddress -and $_.LinkLayerAddress -and $_.State -ne 'Unreachable' } | \
Select-Object IPAddress,LinkLayerAddress,State,InterfaceAlias,AddressFamily | \
ConvertTo-Json -Compress";

    let output = Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", script])
        .output();

    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }

    parse_neighbor_json(&String::from_utf8_lossy(&output.stdout))
}

fn parse_neighbor_json(raw: &str) -> Vec<NeighborRow> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    if trimmed.starts_with('[') {
        serde_json::from_str::<Vec<NeighborRow>>(trimmed).unwrap_or_default()
    } else {
        serde_json::from_str::<NeighborRow>(trimmed)
            .map(|row| vec![row])
            .unwrap_or_default()
    }
}

fn merge_neighbor_rows(rows: Vec<NeighborRow>) -> Vec<LanDevice> {
    let mut grouped: BTreeMap<String, DeviceAccumulator> = BTreeMap::new();

    for row in rows {
        let ip = row.ip_address.trim();
        if ip.is_empty() || ip.starts_with("ff") || ip == "::" || ip == "0.0.0.0" {
            continue;
        }

        let mac = normalize_mac(row.link_layer_address.as_deref().unwrap_or_default());
        let key = if mac.is_empty() {
            format!("ip:{}", ip)
        } else {
            format!("mac:{}", mac)
        };

        let entry = grouped.entry(key).or_insert_with(|| DeviceAccumulator {
            mac: mac.clone(),
            hostname: String::new(),
            ipv4: Vec::new(),
            ipv6: Vec::new(),
            online: false,
            source: row.interface_alias.clone().unwrap_or_else(|| "neighbor".to_string()),
        });

        if let Ok(addr) = ip.parse::<IpAddr>() {
            match addr {
                IpAddr::V4(_) => push_unique(&mut entry.ipv4, ip.to_string()),
                IpAddr::V6(_) => push_unique(&mut entry.ipv6, ip.to_string()),
            }
        }

        if is_online_state(row.state.as_deref().unwrap_or_default()) {
            entry.online = true;
        }
    }

    grouped
        .into_iter()
        .map(|(key, mut entry)| {
            entry.ipv4.sort();
            entry.ipv6.sort();
            let global_ipv6 = entry
                .ipv6
                .iter()
                .filter(|value| is_global_ipv6(value))
                .cloned()
                .collect::<Vec<_>>();
            let display_name = if entry.mac.is_empty() {
                entry
                    .ipv4
                    .first()
                    .or_else(|| entry.ipv6.first())
                    .cloned()
                    .unwrap_or_else(|| "未知设备".to_string())
            } else {
                entry.mac.clone()
            };

            LanDevice {
                id: key,
                display_name,
                hostname: entry.hostname,
                mac: entry.mac,
                ipv4: entry.ipv4,
                ipv6: entry.ipv6,
                global_ipv6,
                online: entry.online,
                source: entry.source,
                last_seen: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            }
        })
        .collect()
}

fn local_interface_devices() -> Vec<LanDevice> {
    match local_ip_address::list_afinet_netifas() {
        Ok(interfaces) => interfaces
            .into_iter()
            .map(|(name, addr)| {
                let value = addr.to_string();
                let (ipv4, ipv6) = match addr {
                    IpAddr::V4(_) => (vec![value.clone()], Vec::new()),
                    IpAddr::V6(_) => (Vec::new(), vec![value.clone()]),
                };
                let global_ipv6 = ipv6
                    .iter()
                    .filter(|item| is_global_ipv6(item))
                    .cloned()
                    .collect::<Vec<_>>();
                LanDevice {
                    id: format!("local:{}:{}", name, value),
                    display_name: name,
                    hostname: String::new(),
                    mac: String::new(),
                    ipv4,
                    ipv6,
                    global_ipv6,
                    online: true,
                    source: "local-interface".to_string(),
                    last_seen: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                }
            })
            .collect(),
        Err(_) => Vec::new(),
    }
}

pub fn is_global_ipv6(value: &str) -> bool {
    value
        .parse::<Ipv6Addr>()
        .map(|addr| (addr.segments()[0] & 0xe000) == 0x2000)
        .unwrap_or(false)
}

fn normalize_mac(value: &str) -> String {
    value
        .trim()
        .replace('-', ":")
        .to_ascii_lowercase()
        .split(':')
        .filter(|part| !part.is_empty())
        .map(|part| format!("{:0>2}", part))
        .collect::<Vec<_>>()
        .join(":")
}

fn is_online_state(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "reachable" | "stale" | "delay" | "probe" | "permanent"
    )
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.iter().any(|item| item == &value) {
        values.push(value);
    }
}
```

- [ ] **Step 2: Add tests in the same file**

Append:

```rust
#[cfg(test)]
mod tests {
    use super::{is_global_ipv6, merge_neighbor_rows, normalize_mac, parse_neighbor_json, NeighborRow};

    #[test]
    fn classifies_global_ipv6_only() {
        assert!(is_global_ipv6("240e:1234::1"));
        assert!(is_global_ipv6("2a01:4f8::1"));
        assert!(!is_global_ipv6("fe80::1"));
        assert!(!is_global_ipv6("fd00::1"));
        assert!(!is_global_ipv6("::1"));
    }

    #[test]
    fn normalizes_mac_addresses() {
        assert_eq!(normalize_mac("AA-BB-CC-01-02-03"), "aa:bb:cc:01:02:03");
        assert_eq!(normalize_mac("a:b:c:1:2:3"), "0a:0b:0c:01:02:03");
    }

    #[test]
    fn parses_single_powershell_neighbor_object() {
        let json = r#"{"IPAddress":"192.168.1.20","LinkLayerAddress":"AA-BB-CC-01-02-03","State":"Reachable","InterfaceAlias":"Ethernet","AddressFamily":"IPv4"}"#;
        let rows = parse_neighbor_json(json);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].ip_address, "192.168.1.20");
    }

    #[test]
    fn merges_rows_by_mac_and_collects_global_ipv6() {
        let rows = vec![
            NeighborRow {
                ip_address: "192.168.1.20".to_string(),
                link_layer_address: Some("AA-BB-CC-01-02-03".to_string()),
                state: Some("Reachable".to_string()),
                interface_alias: Some("Ethernet".to_string()),
                address_family: Some("IPv4".to_string()),
            },
            NeighborRow {
                ip_address: "240e:1234::20".to_string(),
                link_layer_address: Some("aa:bb:cc:01:02:03".to_string()),
                state: Some("Stale".to_string()),
                interface_alias: Some("Ethernet".to_string()),
                address_family: Some("IPv6".to_string()),
            },
            NeighborRow {
                ip_address: "fe80::20".to_string(),
                link_layer_address: Some("aa:bb:cc:01:02:03".to_string()),
                state: Some("Stale".to_string()),
                interface_alias: Some("Ethernet".to_string()),
                address_family: Some("IPv6".to_string()),
            },
        ];

        let devices = merge_neighbor_rows(rows);
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].mac, "aa:bb:cc:01:02:03");
        assert_eq!(devices[0].ipv4, vec!["192.168.1.20"]);
        assert_eq!(devices[0].global_ipv6, vec!["240e:1234::20"]);
        assert!(devices[0].online);
    }
}
```

- [ ] **Step 3: Register the module**

Add to the top of `src-tauri/src/lib.rs`:

```rust
mod device_discovery;
```

- [ ] **Step 4: Run tests**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml device_discovery
```

Expected: the four device discovery tests pass.

- [ ] **Step 5: Commit**

```powershell
git add src-tauri/src/device_discovery.rs src-tauri/src/lib.rs
git commit -m "feat(device): 增加局域网设备发现模型"
```

### Task 3: Device DDNS Commands And Background Update

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Extend imports in `commands.rs`**

Update the config import to include `DeviceDdnsConfig`:

```rust
use crate::config::{
    add_log, normalize_forward_rule, save_config, AppConfig, DdnsConfig, DeviceDdnsConfig,
    ForwardRule, LogEntry, RuntimeStatus,
};
```

Add:

```rust
use crate::device_discovery::LanDevice;
```

- [ ] **Step 2: Add helper functions in `commands.rs`**

Place these helpers near the existing DDNS helpers:

```rust
fn device_ddns_domain(config: &DeviceDdnsConfig) -> String {
    if config.domain.trim().is_empty() || config.sub_domain.trim().is_empty() {
        "未配置域名".to_string()
    } else {
        format!("{}.{}", config.sub_domain.trim(), config.domain.trim())
    }
}

fn validate_device_ddns_config(config: &DeviceDdnsConfig) -> Result<(), String> {
    if config.access_key_id.trim().is_empty() || config.access_key_secret.trim().is_empty() {
        return Err("AccessKey ID 或 Secret 未配置".to_string());
    }
    if config.domain.trim().is_empty() || config.sub_domain.trim().is_empty() {
        return Err("主域名或子域名未配置".to_string());
    }
    if config.device_id.trim().is_empty()
        && config.device_mac.trim().is_empty()
        && config.selected_ipv6.trim().is_empty()
    {
        return Err("未选择局域网设备".to_string());
    }
    Ok(())
}

fn to_device_ddns_aliyun_config(config: &DeviceDdnsConfig) -> DdnsConfig {
    DdnsConfig {
        enabled: config.enabled,
        provider: config.provider.clone(),
        access_key_id: config.access_key_id.clone(),
        access_key_secret: config.access_key_secret.clone(),
        domain: config.domain.clone(),
        sub_domain: config.sub_domain.clone(),
        record_type: "AAAA".to_string(),
        ttl: config.ttl,
        interval_minutes: config.interval_minutes,
    }
}

fn resolve_device_ipv6(config: &DeviceDdnsConfig, devices: &[LanDevice]) -> Result<String, String> {
    let selected = config.selected_ipv6.trim();
    if !selected.is_empty()
        && crate::device_discovery::is_global_ipv6(selected)
        && devices.iter().any(|device| device.global_ipv6.iter().any(|ip| ip == selected))
    {
        return Ok(selected.to_string());
    }

    let matched = devices.iter().find(|device| {
        (!config.device_id.trim().is_empty() && device.id == config.device_id)
            || (!config.device_mac.trim().is_empty() && device.mac == config.device_mac)
    });

    let Some(device) = matched else {
        return Err("未发现绑定的局域网设备".to_string());
    };

    device
        .global_ipv6
        .first()
        .cloned()
        .ok_or_else(|| "绑定设备没有公网 IPv6".to_string())
}
```

- [ ] **Step 3: Add command functions**

Add these command functions before `list_forward_rules`:

```rust
#[tauri::command]
pub async fn list_lan_devices() -> Result<Vec<LanDevice>, String> {
    Ok(crate::device_discovery::discover_lan_devices())
}

#[tauri::command]
pub async fn get_device_ddns_config(
    state: State<'_, AppState>,
) -> Result<DeviceDdnsConfig, String> {
    let cfg = read_config(&state)?;
    Ok(cfg.device_ddns)
}

#[tauri::command]
pub async fn save_device_ddns_config(
    state: State<'_, AppState>,
    config: DeviceDdnsConfig,
) -> Result<(), String> {
    let mut cfg = read_config(&state)?;
    let domain = device_ddns_domain(&config);
    cfg.device_ddns = config;
    write_config(&state, &cfg)?;
    add_log("info", "设备DDNS", &format!("设备 DDNS 配置已保存：{}", domain));
    Ok(())
}

#[tauri::command]
pub async fn get_device_ddns_current_record(
    state: State<'_, AppState>,
) -> Result<String, String> {
    let cfg = read_config(&state)?;
    validate_device_ddns_config(&cfg.device_ddns)?;
    let client = crate::ddns::aliyun::AliyunDdns::new(to_device_ddns_aliyun_config(&cfg.device_ddns));
    client.describe_record().await
}

#[tauri::command]
pub async fn trigger_device_ddns_update(
    state: State<'_, AppState>,
    config: Option<DeviceDdnsConfig>,
) -> Result<String, String> {
    let cfg = read_config(&state)?;
    let mut device_config = config.unwrap_or(cfg.device_ddns);
    if !device_config.enabled {
        return Err("设备 DDNS 未启用".to_string());
    }
    validate_device_ddns_config(&device_config)?;

    let devices = crate::device_discovery::discover_lan_devices();
    let ipv6 = resolve_device_ipv6(&device_config, &devices)?;
    device_config.selected_ipv6 = ipv6.clone();

    let domain = device_ddns_domain(&device_config);
    let client = crate::ddns::aliyun::AliyunDdns::new(to_device_ddns_aliyun_config(&device_config));
    let result = client.update_record("", &ipv6).await?;

    let mut next_cfg = read_config(&state)?;
    next_cfg.device_ddns.selected_ipv6 = ipv6;
    next_cfg.device_ddns.last_update_time = chrono::Local::now()
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    next_cfg.device_ddns.last_result = result.clone();
    write_config(&state, &next_cfg)?;

    add_log("info", "设备DDNS", &format!("设备 DDNS 更新完成：{}，{}", domain, result));
    Ok(result)
}
```

- [ ] **Step 4: Register commands in `lib.rs`**

Add these entries to `tauri::generate_handler!`:

```rust
commands::list_lan_devices,
commands::get_device_ddns_config,
commands::save_device_ddns_config,
commands::get_device_ddns_current_record,
commands::trigger_device_ddns_update,
```

- [ ] **Step 5: Add the background task in `lib.rs`**

Spawn it after the existing DDNS background task:

```rust
let device_ddns_handle = app.handle().clone();
tauri::async_runtime::spawn(async move {
    device_ddns_background_task(device_ddns_handle).await;
});
```

Add this function near `ddns_background_task`:

```rust
async fn device_ddns_background_task(app: tauri::AppHandle) {
    config::add_log("info", "设备DDNS", "设备 DDNS 后台任务已启动");

    loop {
        tokio::time::sleep(std::time::Duration::from_secs(45)).await;

        let (enabled, interval_secs, device_config) = {
            let state = app.state::<commands::AppState>();
            let cfg = state.config.lock().unwrap();
            (
                cfg.device_ddns.enabled,
                (cfg.device_ddns.interval_minutes.max(1) as u64) * 60,
                cfg.device_ddns.clone(),
            )
        };

        if !enabled {
            tokio::time::sleep(std::time::Duration::from_secs(interval_secs)).await;
            continue;
        }

        let devices = device_discovery::discover_lan_devices();
        let ipv6 = devices
            .iter()
            .find(|device| {
                (!device_config.device_id.trim().is_empty() && device.id == device_config.device_id)
                    || (!device_config.device_mac.trim().is_empty()
                        && device.mac == device_config.device_mac)
            })
            .and_then(|device| device.global_ipv6.first().cloned());

        let Some(ipv6) = ipv6 else {
            config::add_log("warn", "设备DDNS", "绑定设备未在线或没有公网 IPv6");
            tokio::time::sleep(std::time::Duration::from_secs(interval_secs)).await;
            continue;
        };

        let ddns_config = config::DdnsConfig {
            enabled: true,
            provider: device_config.provider.clone(),
            access_key_id: device_config.access_key_id.clone(),
            access_key_secret: device_config.access_key_secret.clone(),
            domain: device_config.domain.clone(),
            sub_domain: device_config.sub_domain.clone(),
            record_type: "AAAA".to_string(),
            ttl: device_config.ttl,
            interval_minutes: device_config.interval_minutes,
        };

        let result = ddns::aliyun::AliyunDdns::new(ddns_config)
            .update_record("", &ipv6)
            .await;

        match result {
            Ok(msg) => config::add_log("info", "设备DDNS", &format!("设备 DDNS 定时更新完成：{}", msg)),
            Err(e) => config::add_log("error", "设备DDNS", &format!("设备 DDNS 定时更新失败：{}", e)),
        }

        tokio::time::sleep(std::time::Duration::from_secs(interval_secs)).await;
    }
}
```

- [ ] **Step 6: Run backend checks**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: all Rust tests pass.

- [ ] **Step 7: Commit**

```powershell
git add src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "feat(ddns): 支持设备级 IPv6 DDNS 更新"
```

### Task 4: Frontend Types And Device Panel

**Files:**
- Modify: `src/types.ts`
- Create: `src/components/DeviceDdnsPanel.vue`

- [ ] **Step 1: Add frontend types**

Append to `src/types.ts`:

```ts
export interface DeviceDdnsConfig {
  enabled: boolean;
  provider: string;
  access_key_id: string;
  access_key_secret: string;
  domain: string;
  sub_domain: string;
  ttl: number;
  interval_minutes: number;
  device_id: string;
  device_mac: string;
  device_name: string;
  selected_ipv6: string;
  last_update_time: string;
  last_result: string;
}

export interface LanDevice {
  id: string;
  display_name: string;
  hostname: string;
  mac: string;
  ipv4: string[];
  ipv6: string[];
  global_ipv6: string[];
  online: boolean;
  source: string;
  last_seen: string;
}
```

- [ ] **Step 2: Create `DeviceDdnsPanel.vue` script**

Create the file with this script:

```vue
<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { Check, RefreshCw, Router, ShieldCheck } from "@lucide/vue";
import type { DeviceDdnsConfig, LanDevice } from "../types";

const defaultConfig: DeviceDdnsConfig = {
  enabled: false,
  provider: "aliyun",
  access_key_id: "",
  access_key_secret: "",
  domain: "",
  sub_domain: "",
  ttl: 600,
  interval_minutes: 10,
  device_id: "",
  device_mac: "",
  device_name: "",
  selected_ipv6: "",
  last_update_time: "",
  last_result: "",
};

const devices = ref<LanDevice[]>([]);
const config = ref<DeviceDdnsConfig>({ ...defaultConfig });
const selectedIpv6 = ref("");
const currentRecord = ref("");
const loadingDevices = ref(false);
const saving = ref(false);
const updating = ref(false);
const statusMessage = ref("");
const messageType = ref<"info" | "success" | "error">("info");

const selectedDevice = computed(() =>
  devices.value.find(
    (device) =>
      device.id === config.value.device_id ||
      (!!config.value.device_mac && device.mac === config.value.device_mac),
  ),
);

const bindableDevices = computed(() =>
  devices.value.filter((device) => device.global_ipv6.length > 0),
);

const deviceSummary = computed(() => {
  const total = devices.value.length;
  const bindable = bindableDevices.value.length;
  return `${total} 台设备，${bindable} 台可绑定公网 IPv6`;
});

function normalizeConfig(data: Partial<DeviceDdnsConfig> | null | undefined): DeviceDdnsConfig {
  return {
    ...defaultConfig,
    ...data,
    provider: data?.provider || "aliyun",
    ttl: Number(data?.ttl) || defaultConfig.ttl,
    interval_minutes: Number(data?.interval_minutes) || defaultConfig.interval_minutes,
  };
}

function primaryIpv6(device: LanDevice): string {
  return device.global_ipv6[0] || "";
}

function deviceTitle(device: LanDevice): string {
  return device.hostname || device.display_name || device.mac || device.ipv4[0] || "未知设备";
}

function selectDevice(device: LanDevice) {
  config.value.device_id = device.id;
  config.value.device_mac = device.mac;
  config.value.device_name = deviceTitle(device);
  config.value.selected_ipv6 = primaryIpv6(device);
  selectedIpv6.value = config.value.selected_ipv6;
  statusMessage.value = "";
}

function validateConfig(requireEnabled: boolean): string {
  if (requireEnabled && !config.value.enabled) return "设备 DDNS 未启用";
  if (!config.value.access_key_id.trim() || !config.value.access_key_secret.trim()) {
    return "请填写完整的 AccessKey ID 和 Secret";
  }
  if (!config.value.domain.trim() || !config.value.sub_domain.trim()) {
    return "请填写主域名和子域名";
  }
  if (!config.value.device_id.trim() && !config.value.selected_ipv6.trim()) {
    return "请选择一台有公网 IPv6 的设备";
  }
  if (!selectedIpv6.value.trim()) return "选中设备没有公网 IPv6";
  return "";
}

async function loadDevices() {
  loadingDevices.value = true;
  try {
    devices.value = await invoke<LanDevice[]>("list_lan_devices");
    if (config.value.device_id || config.value.device_mac) {
      const matched = selectedDevice.value;
      if (matched) {
        selectedIpv6.value = config.value.selected_ipv6 || primaryIpv6(matched);
      }
    }
  } catch (e: any) {
    devices.value = [];
    statusMessage.value = `设备发现失败：${String(e)}`;
    messageType.value = "error";
  } finally {
    loadingDevices.value = false;
  }
}

async function loadConfig() {
  try {
    const data = await invoke<DeviceDdnsConfig>("get_device_ddns_config");
    config.value = normalizeConfig(data);
    selectedIpv6.value = config.value.selected_ipv6;
  } catch {
    config.value = { ...defaultConfig };
  }
}

async function loadCurrentRecord() {
  if (!config.value.access_key_id || !config.value.domain || !config.value.sub_domain) {
    currentRecord.value = "";
    return;
  }
  try {
    currentRecord.value = await invoke<string>("get_device_ddns_current_record");
  } catch {
    currentRecord.value = "";
  }
}

async function saveConfig(showSuccess = true) {
  const error = config.value.enabled ? validateConfig(false) : "";
  if (error) {
    statusMessage.value = error;
    messageType.value = "error";
    return false;
  }

  config.value.selected_ipv6 = selectedIpv6.value;
  saving.value = true;
  try {
    await invoke("save_device_ddns_config", { config: config.value });
    window.dispatchEvent(new CustomEvent("homenet:logs-refresh"));
    if (showSuccess) {
      statusMessage.value = "设备 DDNS 配置已保存";
      messageType.value = "success";
    }
    return true;
  } catch (e: any) {
    statusMessage.value = `保存失败：${String(e)}`;
    messageType.value = "error";
    return false;
  } finally {
    saving.value = false;
  }
}

async function triggerUpdate() {
  const error = validateConfig(true);
  if (error) {
    statusMessage.value = error;
    messageType.value = "error";
    return;
  }

  updating.value = true;
  try {
    const saved = await saveConfig(false);
    if (!saved) return;
    const result = await invoke<string>("trigger_device_ddns_update", { config: config.value });
    statusMessage.value = result || "设备 DDNS 更新完成";
    messageType.value = "success";
    await Promise.all([loadConfig(), loadCurrentRecord()]);
    window.dispatchEvent(new CustomEvent("homenet:logs-refresh"));
  } catch (e: any) {
    statusMessage.value = `更新失败：${String(e)}`;
    messageType.value = "error";
  } finally {
    updating.value = false;
  }
}

async function toggleEnabled(event: Event) {
  const input = event.target as HTMLInputElement;
  const previous = config.value.enabled;
  config.value.enabled = input.checked;
  const saved = await saveConfig(false);
  if (!saved) config.value.enabled = previous;
}

onMounted(async () => {
  await Promise.all([loadConfig(), loadDevices()]);
  await loadCurrentRecord();
});
</script>
```

- [ ] **Step 3: Add template and scoped styles**

Append this template and style to `DeviceDdnsPanel.vue`:

```vue
<template>
  <section class="panel device-ddns-panel">
    <header class="panel-header">
      <div>
        <h2>局域网设备 DDNS</h2>
        <p>{{ deviceSummary }}</p>
      </div>
      <div class="header-actions">
        <label class="toggle-switch" aria-label="启用设备 DDNS">
          <input
            type="checkbox"
            :checked="config.enabled"
            :disabled="saving"
            @change="toggleEnabled"
          />
          <span class="toggle-slider"></span>
        </label>
        <button class="icon-button" type="button" :disabled="loadingDevices" @click="loadDevices">
          <RefreshCw :size="15" :stroke-width="2.2" />
        </button>
      </div>
    </header>

    <p v-if="statusMessage" class="status-message" :class="`msg-${messageType}`">
      {{ statusMessage }}
    </p>

    <div class="device-layout">
      <div class="device-list" role="listbox" aria-label="局域网设备">
        <button
          v-for="device in devices"
          :key="device.id"
          class="device-row"
          :class="{ selected: selectedDevice?.id === device.id, disabled: device.global_ipv6.length === 0 }"
          type="button"
          :disabled="device.global_ipv6.length === 0"
          @click="selectDevice(device)"
        >
          <Router class="device-icon" :size="16" :stroke-width="2.2" />
          <span class="device-copy">
            <strong>{{ deviceTitle(device) }}</strong>
            <span>{{ device.ipv4[0] || "--" }} · {{ device.global_ipv6[0] || "无公网 IPv6" }}</span>
          </span>
          <Check v-if="selectedDevice?.id === device.id" class="device-check" :size="15" />
        </button>
        <div v-if="devices.length === 0" class="empty-state">
          {{ loadingDevices ? "正在发现设备..." : "未发现局域网设备" }}
        </div>
      </div>

      <div class="device-form">
        <label>
          <span>AccessKey ID</span>
          <input v-model="config.access_key_id" type="text" />
        </label>
        <label>
          <span>AccessKey Secret</span>
          <input v-model="config.access_key_secret" type="password" />
        </label>
        <label>
          <span>主域名</span>
          <input v-model="config.domain" type="text" placeholder="example.com" />
        </label>
        <label>
          <span>子域名</span>
          <input v-model="config.sub_domain" type="text" placeholder="nas" />
        </label>
        <label>
          <span>设备 IPv6</span>
          <select v-model="selectedIpv6">
            <option value="">请选择公网 IPv6</option>
            <option
              v-for="ip in selectedDevice?.global_ipv6 ?? []"
              :key="ip"
              :value="ip"
            >
              {{ ip }}
            </option>
          </select>
        </label>
        <label>
          <span>TTL / 间隔</span>
          <div class="split-inputs">
            <input v-model.number="config.ttl" type="number" min="1" max="86400" />
            <input v-model.number="config.interval_minutes" type="number" min="1" max="1440" />
          </div>
        </label>
      </div>
    </div>

    <footer class="panel-footer">
      <span class="footer-status">
        <ShieldCheck :size="17" :stroke-width="2.2" />
        {{ currentRecord || config.last_result || "暂无设备 DDNS 更新记录" }}
      </span>
      <div class="footer-actions">
        <button class="btn btn-secondary" type="button" :disabled="saving" @click="saveConfig(true)">
          {{ saving ? "保存中..." : "保存" }}
        </button>
        <button class="btn btn-primary" type="button" :disabled="updating" @click="triggerUpdate">
          {{ updating ? "更新中..." : "立即更新" }}
        </button>
      </div>
    </footer>
  </section>
</template>

<style scoped>
.panel {
  min-width: 0;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  border: 1px solid rgba(217, 225, 237, 0.95);
  border-radius: var(--radius-md, 8px);
  background: rgba(255, 255, 255, 0.94);
  box-shadow: var(--shadow-card);
}

.panel-header {
  flex: 0 0 52px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 0 14px;
  border-bottom: 1px solid #e1e8f2;
}

.panel-header h2 {
  font-size: 16px;
  font-weight: 800;
}

.panel-header p {
  margin-top: 2px;
  color: #64748b;
  font-size: 12px;
}

.header-actions,
.footer-actions {
  display: flex;
  align-items: center;
  gap: 8px;
}

.icon-button {
  width: 30px;
  height: 30px;
  display: grid;
  place-items: center;
  border: 1px solid #d7e0eb;
  border-radius: 5px;
  background: #ffffff;
  color: #2563eb;
}

.status-message {
  margin: 8px 12px 0;
  padding: 7px 9px;
  border-radius: 6px;
  font-size: 12px;
}

.msg-info { color: #1d4ed8; background: #eaf2ff; }
.msg-success { color: #15803d; background: #e8f8ee; }
.msg-error { color: #b91c1c; background: #fee2e2; }

.device-layout {
  flex: 1 1 auto;
  min-height: 0;
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 10px;
  padding: 10px 12px;
}

.device-list {
  min-height: 0;
  overflow-y: auto;
  display: grid;
  align-content: start;
  gap: 6px;
}

.device-row {
  width: 100%;
  min-height: 46px;
  display: grid;
  grid-template-columns: 20px minmax(0, 1fr) 18px;
  align-items: center;
  gap: 8px;
  padding: 7px 8px;
  border: 1px solid #dbe4ee;
  border-radius: 6px;
  background: #ffffff;
  color: #111827;
  text-align: left;
}

.device-row.selected {
  border-color: #2563eb;
  background: #eef6ff;
}

.device-row.disabled {
  opacity: 0.55;
  cursor: not-allowed;
}

.device-copy {
  min-width: 0;
  display: grid;
  gap: 2px;
}

.device-copy strong,
.device-copy span {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.device-copy strong {
  font-size: 12px;
}

.device-copy span {
  color: #64748b;
  font-size: 11px;
}

.device-form {
  min-width: 0;
  display: grid;
  align-content: start;
  gap: 7px;
}

.device-form label {
  display: grid;
  grid-template-columns: 78px minmax(0, 1fr);
  align-items: center;
  gap: 8px;
}

.device-form span {
  color: #374151;
  font-size: 12px;
  font-weight: 700;
}

.device-form input,
.device-form select {
  width: 100%;
  height: 30px;
  border: 1px solid #d7e0eb;
  border-radius: 5px;
  background: #ffffff;
  color: #202532;
  padding: 0 8px;
  font-size: 12px;
}

.split-inputs {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 6px;
}

.empty-state {
  display: grid;
  place-items: center;
  min-height: 80px;
  color: #8a94a6;
  font-size: 12px;
}

.panel-footer {
  flex: 0 0 42px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  padding: 0 12px;
  border-top: 1px solid #e1e8f2;
  color: #64748b;
  font-size: 12px;
}

.footer-status {
  min-width: 0;
  display: inline-flex;
  align-items: center;
  gap: 7px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.btn {
  height: 30px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  padding: 0 10px;
  border-radius: 5px;
  border: 1px solid transparent;
  font-size: 12px;
  font-weight: 700;
}

.btn-primary {
  color: #ffffff;
  background: var(--color-primary, #2563eb);
}

.btn-secondary {
  color: #374151;
  background: #ffffff;
  border-color: #d7e0eb;
}

.toggle-switch {
  position: relative;
  display: inline-block;
  width: 34px;
  height: 19px;
}

.toggle-switch input {
  width: 0;
  height: 0;
  opacity: 0;
}

.toggle-slider {
  position: absolute;
  inset: 0;
  border-radius: 999px;
  background: #cbd5e1;
}

.toggle-slider::before {
  content: "";
  position: absolute;
  left: 3px;
  top: 3px;
  width: 13px;
  height: 13px;
  border-radius: 50%;
  background: #ffffff;
  transition: transform 0.15s ease;
}

.toggle-switch input:checked + .toggle-slider {
  background: var(--color-primary, #2563eb);
}

.toggle-switch input:checked + .toggle-slider::before {
  transform: translateX(15px);
}
</style>
```

- [ ] **Step 4: Run frontend type check**

Run:

```powershell
pnpm build
```

Expected: `vue-tsc --noEmit && vite build` exits 0.

- [ ] **Step 5: Commit**

```powershell
git add src/types.ts src/components/DeviceDdnsPanel.vue
git commit -m "feat(ui): 增加设备 DDNS 面板"
```

### Task 5: Main Layout Integration Without Touching Forward Rules

**Files:**
- Modify: `src/App.vue`
- Modify: `src/components/DdnsPanel.vue`

- [ ] **Step 1: Import and render the new panel**

In `src/App.vue`, add:

```ts
import DeviceDdnsPanel from "./components/DeviceDdnsPanel.vue";
```

Replace the `section-panels` content with:

```vue
<section class="section-panels" aria-label="配置面板">
  <div class="ddns-stack">
    <DeviceDdnsPanel />
    <DdnsPanel />
  </div>
  <ForwardRulesPanel />
</section>
```

- [ ] **Step 2: Adjust the main grid**

In `src/App.vue`, update these CSS rules:

```css
.main-content {
  flex: 1;
  min-width: 0;
  padding: 16px 24px 20px;
  overflow: hidden;
  display: grid;
  grid-template-rows: 112px minmax(0, 1fr) 168px;
  gap: 14px;
}

.section-cards {
  display: grid;
  grid-template-columns: repeat(5, minmax(0, 1fr));
  gap: 14px;
}

.section-panels {
  min-height: 0;
  display: grid;
  grid-template-columns: 560px minmax(0, 1fr);
  gap: 12px;
}

.ddns-stack {
  min-width: 0;
  min-height: 0;
  display: grid;
  grid-template-rows: 300px minmax(0, 1fr);
  gap: 12px;
}
```

- [ ] **Step 3: Compact the existing DDNS panel styles**

In `src/components/DdnsPanel.vue`, keep all script logic and template fields. Change only these CSS values:

```css
.panel-header {
  flex: 0 0 52px;
  height: 52px;
  padding: 0 14px;
}

.panel-header h2 {
  font-size: 17px;
}

.form-grid {
  gap: 5px;
  padding: 10px 14px 8px;
}

.field-row {
  grid-template-columns: 112px minmax(0, 1fr);
  min-height: 30px;
  gap: 10px;
}

input,
select {
  height: 30px;
  font-size: 12px;
}

.actions {
  gap: 7px;
  padding: 0 14px 8px;
}

.btn {
  height: 32px;
  font-size: 12px;
}

.panel-footer {
  min-height: 34px;
  padding: 0 14px;
}
```

- [ ] **Step 4: Confirm the forwarding panel file is untouched**

Run:

```powershell
git diff -- src/components/ForwardRulesPanel.vue
```

Expected: no output.

- [ ] **Step 5: Run build**

Run:

```powershell
pnpm build
```

Expected: `vue-tsc --noEmit && vite build` exits 0.

- [ ] **Step 6: Commit**

```powershell
git add src/App.vue src/components/DdnsPanel.vue
git commit -m "feat(ui): 调整设备 DDNS 布局"
```

### Task 6: End-To-End Verification

**Files:**
- No production edits expected.

- [ ] **Step 1: Run all Rust tests**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: all Rust tests pass.

- [ ] **Step 2: Run frontend build**

Run:

```powershell
pnpm build
```

Expected: `vue-tsc --noEmit && vite build` exits 0.

- [ ] **Step 3: Run Tauri dev app for manual check**

Run:

```powershell
pnpm tauri dev
```

Expected:

- The app opens.
- The top status cards remain visible.
- The left configuration area shows `局域网设备 DDNS` above the existing Aliyun DDNS panel.
- The right forwarding rules panel has the same table, toolbar, editor, columns, and actions as before.
- The logs panel remains visible.

- [ ] **Step 4: Manual device-DDNS behavior check**

Use a LAN device that has a global IPv6 address:

- Click refresh in `局域网设备 DDNS`.
- Select the device with a global IPv6.
- Fill AccessKey, domain, and subdomain.
- Enable device DDNS.
- Click `保存`.
- Click `立即更新`.

Expected:

- If credentials and domain are valid, Aliyun AAAA record is added or updated to the selected device IPv6.
- If the selected device has no global IPv6, update is blocked with `请选择一台有公网 IPv6 的设备` or `选中设备没有公网 IPv6`.
- A `设备DDNS` log row is written.

- [ ] **Step 5: Final working tree check**

Run:

```powershell
git status --short
```

Expected: no unexpected edits to `src/components/ForwardRulesPanel.vue`.

## Self-Review

- Spec coverage: Task 2 discovers LAN devices, Task 3 binds and updates a selected device AAAA record, Task 4 adds the device DDNS UI, Task 5 keeps the forwarding panel untouched and adjusts only outer layout plus DDNS spacing, Task 6 verifies device behavior and forwarding-panel preservation.
- Placeholder scan: the plan contains no open placeholders, no deferred implementation markers, and no vague error-handling instructions.
- Type consistency: `DeviceDdnsConfig` and `LanDevice` field names match between Rust and TypeScript; command names in Vue match command names registered in Tauri.
