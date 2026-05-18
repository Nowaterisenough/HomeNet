use chrono::Local;
use std::sync::Mutex;
use tauri::State;
use tokio::sync::Mutex as TokioMutex;
use uuid::Uuid;

use crate::config::{
    add_log, normalize_forward_rule, save_config, AppConfig, DdnsConfig, DeviceDdnsConfig,
    ForwardRule, LogEntry, RuntimeStatus,
};
use crate::ddns::NetworkInterfaceInfo;
use crate::device_discovery::LanDevice;
use crate::forward::manager::ForwardManager;

// ---------------------------------------------------------------------------
// App state shared across all Tauri commands
// ---------------------------------------------------------------------------

pub struct AppState {
    pub config: Mutex<AppConfig>,
    pub forward_manager: TokioMutex<ForwardManager>,
}

// ---------------------------------------------------------------------------
// Helper: extract current AppConfig from state
// ---------------------------------------------------------------------------

fn read_config(state: &AppState) -> Result<AppConfig, String> {
    state
        .config
        .lock()
        .map(|g| g.clone())
        .map_err(|e| format!("读取配置失败：{}", e))
}

fn write_config(state: &AppState, new_cfg: &AppConfig) -> Result<(), String> {
    save_config(new_cfg)?;
    let mut guard = state
        .config
        .lock()
        .map_err(|e| format!("写入配置失败：{}", e))?;
    *guard = new_cfg.clone();
    Ok(())
}

fn ddns_domain(config: &DdnsConfig) -> String {
    if config.domain.trim().is_empty() || config.sub_domain.trim().is_empty() {
        "未配置域名".to_string()
    } else {
        format!("{}.{}", config.sub_domain.trim(), config.domain.trim())
    }
}

fn validate_ddns_config(config: &DdnsConfig) -> Result<(), String> {
    if config.access_key_id.trim().is_empty() || config.access_key_secret.trim().is_empty() {
        return Err("AccessKey ID 或 Secret 未配置".to_string());
    }
    if config.domain.trim().is_empty() || config.sub_domain.trim().is_empty() {
        return Err("主域名或子域名未配置".to_string());
    }
    Ok(())
}

pub(crate) fn device_ddns_domain(config: &DeviceDdnsConfig) -> String {
    if config.domain.trim().is_empty() || config.sub_domain.trim().is_empty() {
        "未配置域名".to_string()
    } else {
        format!("{}.{}", config.sub_domain.trim(), config.domain.trim())
    }
}

pub(crate) fn validate_device_ddns_config(config: &DeviceDdnsConfig) -> Result<(), String> {
    if config.access_key_id.trim().is_empty() || config.access_key_secret.trim().is_empty() {
        return Err("AccessKey ID 或 Secret 未配置".to_string());
    }
    if config.domain.trim().is_empty() || config.sub_domain.trim().is_empty() {
        return Err("主域名或子域名未配置".to_string());
    }
    if config.device_id.trim().is_empty() && config.device_mac.trim().is_empty() {
        return Err("请选择要更新 DDNS 的设备".to_string());
    }
    Ok(())
}

pub(crate) fn to_device_ddns_aliyun_config(config: &DeviceDdnsConfig) -> DdnsConfig {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeviceDdnsOperationIdentity {
    enabled: bool,
    provider: String,
    access_key_id: String,
    access_key_secret: String,
    domain: String,
    sub_domain: String,
    ttl: u32,
    interval_minutes: u32,
    device_id: String,
    device_mac: String,
    selected_ipv6: String,
}

pub(crate) fn device_ddns_identity(
    config: &DeviceDdnsConfig,
) -> DeviceDdnsOperationIdentity {
    DeviceDdnsOperationIdentity {
        enabled: config.enabled,
        provider: config.provider.trim().to_string(),
        access_key_id: config.access_key_id.trim().to_string(),
        access_key_secret: config.access_key_secret.trim().to_string(),
        domain: config.domain.trim().to_string(),
        sub_domain: config.sub_domain.trim().to_string(),
        ttl: config.ttl,
        interval_minutes: config.interval_minutes,
        device_id: config.device_id.trim().to_string(),
        device_mac: config.device_mac.trim().to_ascii_lowercase(),
        selected_ipv6: config.selected_ipv6.trim().to_string(),
    }
}

pub(crate) fn device_ddns_identity_matches(
    config: &DeviceDdnsConfig,
    identity: &DeviceDdnsOperationIdentity,
) -> bool {
    device_ddns_identity(config) == *identity
}

pub(crate) fn apply_device_ddns_result_if_current(
    state: &AppState,
    identity: &DeviceDdnsOperationIdentity,
    selected_ipv6: Option<String>,
    last_result: String,
) -> Result<bool, String> {
    let mut guard = state
        .config
        .lock()
        .map_err(|e| format!("写入设备 DDNS 结果失败：{}", e))?;

    if !device_ddns_identity_matches(&guard.device_ddns, identity) {
        return Ok(false);
    }

    let mut cfg = guard.clone();
    if let Some(selected_ipv6) = selected_ipv6 {
        cfg.device_ddns.selected_ipv6 = selected_ipv6;
    }
    cfg.device_ddns.last_update_time = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    cfg.device_ddns.last_result = last_result;
    save_config(&cfg)?;
    *guard = cfg;
    Ok(true)
}

fn device_matches_config(device: &LanDevice, config: &DeviceDdnsConfig) -> bool {
    let device_id = config.device_id.trim();
    let device_mac = config.device_mac.trim();

    (!device_id.is_empty() && device.id == device_id)
        || (!device_mac.is_empty() && device.mac.eq_ignore_ascii_case(device_mac))
}

fn first_global_ipv6(device: &LanDevice) -> Option<&str> {
    device
        .global_ipv6
        .iter()
        .chain(device.ipv6.iter())
        .map(|value| value.trim())
        .find(|value| crate::device_discovery::is_global_ipv6(value))
}

pub(crate) fn resolve_device_ipv6(
    config: &DeviceDdnsConfig,
    devices: &[LanDevice],
) -> Result<String, String> {
    let device = devices
        .iter()
        .find(|device| device_matches_config(device, config))
        .ok_or_else(|| "未找到匹配的局域网设备".to_string())?;

    let selected_ipv6 = config.selected_ipv6.trim();
    if !selected_ipv6.is_empty() {
        let selected_is_available = device
            .global_ipv6
            .iter()
            .chain(device.ipv6.iter())
            .any(|value| value.trim() == selected_ipv6)
            && crate::device_discovery::is_global_ipv6(selected_ipv6);

        if selected_is_available {
            return Ok(selected_ipv6.to_string());
        }
    }

    first_global_ipv6(device)
        .map(|value| value.to_string())
        .ok_or_else(|| format!("设备 {} 没有可用的公网 IPv6", device.display_name))
}

pub(crate) async fn update_device_ddns_record(
    config: &DeviceDdnsConfig,
    devices: &[LanDevice],
) -> Result<(String, String), String> {
    validate_device_ddns_config(config)?;
    let ipv6 = resolve_device_ipv6(config, devices)?;
    let client = crate::ddns::aliyun::AliyunDdns::new(to_device_ddns_aliyun_config(config));
    let result = client.update_record("", &ipv6).await?;
    Ok((ipv6, result))
}

fn latest_ddns_update_time() -> String {
    crate::config::get_logs()
        .into_iter()
        .rev()
        .find(|entry| {
            entry.module == "DDNS"
                && (entry.message.contains("DNS 记录已更新")
                    || entry.message.contains("DNS 记录已新增")
                    || entry.message.contains("DNS 记录无需更新"))
        })
        .map(|entry| entry.time)
        .unwrap_or_else(|| "暂无".to_string())
}

fn forward_rule_label(rule: &ForwardRule) -> String {
    if rule.remark.trim().is_empty() {
        format!("{}:{}", rule.listen_addr.trim(), rule.listen_port)
    } else {
        rule.remark.trim().to_string()
    }
}

fn listen_endpoint(rule: &ForwardRule) -> String {
    let listen_addr = if rule.listen_addr.trim().is_empty() {
        "::"
    } else {
        rule.listen_addr.trim()
    };
    format!("[{}]:{}", listen_addr, rule.listen_port)
}

fn describe_forward_rule(rule: &ForwardRule) -> String {
    format!(
        "[{}] {} {} → {}:{}",
        forward_rule_label(rule),
        rule.protocol.to_uppercase(),
        listen_endpoint(rule),
        rule.target_ip.trim(),
        rule.target_port
    )
}

fn validate_forward_rule(rule: &ForwardRule) -> Result<(), String> {
    if rule.listen_port == 0 {
        return Err("监听端口范围：1-65535".to_string());
    }
    if rule.target_port == 0 {
        return Err("目标端口范围：1-65535".to_string());
    }
    if rule.target_ip.trim().is_empty() {
        return Err("目标设备 IP 未填写".to_string());
    }
    Ok(())
}

fn ensure_supported_forward_mode(mode: &str) -> Result<(), String> {
    let normalized = mode.trim().to_lowercase();
    if normalized.is_empty() || normalized == "relay" {
        return Ok(());
    }

    let mode_name = match normalized.as_str() {
        "nat" => "内核 NAT",
        "forward" | "transparent" | "tproxy" => "透明源地址透传",
        _ => "系统级转发",
    };
    Err(format!(
        "当前版本仅支持普通 TCP/UDP 转发，{} 模式尚未启用",
        mode_name
    ))
}

/// After rules change, re-apply the forward manager.
async fn reapply_forward_rules(state: &AppState) {
    let rules = {
        let config = state.config.lock().unwrap();
        config.forward_rules.clone()
    };
    let mut manager = state.forward_manager.lock().await;
    let results = manager.apply_rules(&rules).await;
    drop(manager);

    // Write status updates back to config
    if results.is_empty() {
        return;
    }
    let mut config = state.config.lock().unwrap();
    let mut changed = false;
    for result in &results {
        if let Some(rule) = config.forward_rules.iter_mut().find(|r| r.id == result.rule_id) {
            if rule.status != result.status {
                rule.status = result.status.clone();
                changed = true;
            }
        }
    }
    if changed {
        let _ = save_config(&config);
    }
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_runtime_status(state: State<'_, AppState>) -> Result<RuntimeStatus, String> {
    let cfg = read_config(&state)?;

    let enabled_count = cfg.forward_rules.iter().filter(|r| r.enabled).count();

    // Uptime: how long the process has been running (seconds).
    // Use sysinfo to get process start time; fallback to 0.
    let uptime = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().saturating_sub(start_time_secs()))
        .unwrap_or(0);

    let ipv4 = crate::ddns::get_public_ipv4().await;
    let ipv6 = crate::ddns::get_local_ipv6_for_interface(&cfg.ipv6_interface);

    Ok(RuntimeStatus {
        public_ipv4: ipv4.clone(),
        public_ipv6: ipv6.clone(),
        ddns_status: if cfg.ddns.enabled { "运行中" } else { "已停止" }.to_string(),
        last_update_time: latest_ddns_update_time(),
        rule_count: cfg.forward_rules.len() as u32,
        enabled_rule_count: enabled_count as u32,
        uptime,
    })
}

#[tauri::command]
pub async fn get_ddns_config(state: State<'_, AppState>) -> Result<DdnsConfig, String> {
    let cfg = read_config(&state)?;
    Ok(cfg.ddns)
}

#[tauri::command]
pub async fn save_ddns_config(
    state: State<'_, AppState>,
    config: DdnsConfig,
) -> Result<(), String> {
    let mut cfg = read_config(&state)?;
    let domain = ddns_domain(&config);
    cfg.ddns = config;
    write_config(&state, &cfg)?;
    add_log("info", "DDNS", &format!("DDNS 配置已保存：{}", domain));
    Ok(())
}

#[tauri::command]
pub async fn test_ddns_connection(
    state: State<'_, AppState>,
    config: Option<DdnsConfig>,
) -> Result<String, String> {
    let cfg = read_config(&state)?;
    let ddns_config = config.unwrap_or(cfg.ddns);
    validate_ddns_config(&ddns_config)?;
    let client = crate::ddns::aliyun::AliyunDdns::new(ddns_config);
    let result = client.test_connection().await?;
    Ok(result)
}

#[tauri::command]
pub async fn trigger_ddns_update(
    state: State<'_, AppState>,
    config: Option<DdnsConfig>,
) -> Result<String, String> {
    let cfg = read_config(&state)?;
    let ddns_config = config.unwrap_or(cfg.ddns);
    if !ddns_config.enabled {
        return Err("DDNS 未启用".to_string());
    }
    validate_ddns_config(&ddns_config)?;
    let ipv4 = crate::ddns::get_public_ipv4().await;
    let ipv6 = crate::ddns::get_local_ipv6_for_interface(&cfg.ipv6_interface);
    let domain = ddns_domain(&ddns_config);
    let client = crate::ddns::aliyun::AliyunDdns::new(ddns_config);
    let result = client.update_record(&ipv4, &ipv6).await?;
    add_log("info", "DDNS", &format!("DDNS 手动更新完成：{}，{}", domain, result));
    Ok(result)
}

#[tauri::command]
pub async fn get_ddns_current_record(state: State<'_, AppState>) -> Result<String, String> {
    let cfg = read_config(&state)?;
    validate_ddns_config(&cfg.ddns)?;
    let client = crate::ddns::aliyun::AliyunDdns::new(cfg.ddns);
    client.describe_record().await
}

#[tauri::command]
pub async fn list_network_interfaces() -> Result<Vec<NetworkInterfaceInfo>, String> {
    Ok(crate::ddns::list_network_interfaces())
}

#[tauri::command]
pub async fn get_ipv6_interface(state: State<'_, AppState>) -> Result<String, String> {
    let cfg = read_config(&state)?;
    Ok(cfg.ipv6_interface)
}

#[tauri::command]
pub async fn set_ipv6_interface(
    state: State<'_, AppState>,
    interface_name: String,
) -> Result<(), String> {
    let mut cfg = read_config(&state)?;
    let normalized_name = interface_name.trim().to_string();
    cfg.ipv6_interface = normalized_name.clone();
    write_config(&state, &cfg)?;
    add_log(
        "info",
        "网络",
        &format!(
            "IPv6 绑定网卡已设置为：{}",
            if normalized_name.is_empty() {
                "自动选择"
            } else {
                normalized_name.as_str()
            }
        ),
    );
    Ok(())
}

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
    add_log(
        "info",
        "设备DDNS",
        &format!("设备 DDNS 配置已保存：{}", domain),
    );
    Ok(())
}

#[tauri::command]
pub async fn get_device_ddns_current_record(state: State<'_, AppState>) -> Result<String, String> {
    let cfg = read_config(&state)?;
    validate_device_ddns_config(&cfg.device_ddns)?;
    let client =
        crate::ddns::aliyun::AliyunDdns::new(to_device_ddns_aliyun_config(&cfg.device_ddns));
    client.describe_record().await
}

#[tauri::command]
pub async fn trigger_device_ddns_update(
    state: State<'_, AppState>,
    config: Option<DeviceDdnsConfig>,
) -> Result<String, String> {
    let device_config = {
        let cfg = read_config(&state)?;
        config.unwrap_or(cfg.device_ddns)
    };
    let identity = device_ddns_identity(&device_config);

    if !device_config.enabled {
        return Err("设备 DDNS 未启用".to_string());
    }
    validate_device_ddns_config(&device_config)?;

    let devices = crate::device_discovery::discover_lan_devices();
    let domain = device_ddns_domain(&device_config);
    let update_result = update_device_ddns_record(&device_config, &devices).await;

    match update_result {
        Ok((ipv6, result)) => {
            match apply_device_ddns_result_if_current(
                &state,
                &identity,
                Some(ipv6.clone()),
                result.clone(),
            ) {
                Ok(true) => {}
                Ok(false) => add_log(
                    "warn",
                    "设备DDNS",
                    &format!(
                        "设备 DDNS 配置已变化，跳过本次手动更新结果写入：{}",
                        domain
                    ),
                ),
                Err(error) => add_log(
                    "error",
                    "设备DDNS",
                    &format!("设备 DDNS 手动更新结果写入失败：{}，{}", domain, error),
                ),
            }

            add_log(
                "info",
                "设备DDNS",
                &format!("设备 DDNS 手动更新完成：{} -> {}，{}", domain, ipv6, result),
            );
            Ok(result)
        }
        Err(error) => {
            match apply_device_ddns_result_if_current(&state, &identity, None, error.clone()) {
                Ok(true) => {}
                Ok(false) => add_log(
                    "warn",
                    "设备DDNS",
                    &format!(
                        "设备 DDNS 配置已变化，跳过本次手动失败结果写入：{}",
                        domain
                    ),
                ),
                Err(save_error) => add_log(
                    "error",
                    "设备DDNS",
                    &format!(
                        "设备 DDNS 手动失败结果写入失败：{}，{}",
                        domain, save_error
                    ),
                ),
            }

            add_log(
                "error",
                "设备DDNS",
                &format!("设备 DDNS 手动更新失败：{}，{}", domain, error),
            );
            Err(error)
        }
    }
}

#[tauri::command]
pub async fn list_forward_rules(state: State<'_, AppState>) -> Result<Vec<ForwardRule>, String> {
    let cfg = read_config(&state)?;
    Ok(cfg.forward_rules)
}

#[tauri::command]
pub async fn save_forward_rule(
    state: State<'_, AppState>,
    mut rule: ForwardRule,
) -> Result<ForwardRule, String> {
    ensure_supported_forward_mode(&rule.mode)?;
    normalize_forward_rule(&mut rule);
    validate_forward_rule(&rule)?;
    let mut cfg = read_config(&state)?;
    let is_new = rule.id.trim().is_empty();
    if rule.status.trim().is_empty() {
        rule.status = if rule.enabled { "正常".into() } else { "已禁用".into() };
    }

    if is_new {
        rule.id = Uuid::new_v4().to_string();
        rule.status = if rule.enabled { "正常".into() } else { "已禁用".into() };
        cfg.forward_rules.push(rule.clone());
    } else {
        if let Some(existing) = cfg.forward_rules.iter_mut().find(|r| r.id == rule.id) {
            *existing = rule.clone();
        } else {
            cfg.forward_rules.push(rule.clone());
        }
    }

    let saved_rule = rule;

    write_config(&state, &cfg)?;
    add_log(
        "info",
        "转发",
        &format!(
            "转发规则已{}：{}",
            if is_new { "新增" } else { "保存" },
            describe_forward_rule(&saved_rule)
        ),
    );
    reapply_forward_rules(&state).await;
    Ok(saved_rule)
}

#[tauri::command]
pub async fn delete_forward_rule(
    state: State<'_, AppState>,
    rule_id: String,
) -> Result<(), String> {
    let mut cfg = read_config(&state)?;
    let removed = cfg.forward_rules.iter().find(|r| r.id == rule_id).cloned();
    let Some(rule) = removed else {
        return Err(format!("未找到转发规则：{}", rule_id));
    };
    cfg.forward_rules.retain(|r| r.id != rule_id);
    write_config(&state, &cfg)?;
    add_log(
        "info",
        "转发",
        &format!("转发规则已删除：{}", describe_forward_rule(&rule)),
    );
    reapply_forward_rules(&state).await;
    Ok(())
}

#[tauri::command]
pub async fn enable_forward_rule(
    state: State<'_, AppState>,
    rule_id: String,
    enabled: bool,
) -> Result<(), String> {
    let mut cfg = read_config(&state)?;
    let updated = if let Some(rule) = cfg.forward_rules.iter_mut().find(|r| r.id == rule_id) {
        rule.enabled = enabled;
        rule.status = if enabled { "正常".into() } else { "已禁用".into() };
        Some(rule.clone())
    } else {
        None
    };

    let Some(rule) = updated else {
        return Err(format!("未找到转发规则：{}", rule_id));
    };

    write_config(&state, &cfg)?;
    add_log(
        "info",
        "转发",
        &format!(
            "转发规则已{}：{}",
            if enabled { "启用" } else { "禁用" },
            describe_forward_rule(&rule)
        ),
    );
    reapply_forward_rules(&state).await;
    Ok(())
}

#[tauri::command]
pub async fn get_recent_logs() -> Result<Vec<LogEntry>, String> {
    Ok(crate::config::get_logs())
}

#[tauri::command]
pub async fn clear_logs() -> Result<(), String> {
    crate::config::clear_logs();
    add_log("info", "日志", "日志已清空");
    Ok(())
}

#[tauri::command]
pub async fn get_auto_start() -> Result<bool, String> {
    Ok(crate::autostart::is_autostart_enabled())
}

#[tauri::command]
pub async fn set_auto_start(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    crate::autostart::set_autostart(enabled)?;

    let mut cfg = read_config(&state)?;
    cfg.auto_start = enabled;
    write_config(&state, &cfg)?;

    add_log(
        "info",
        "自启动",
        if enabled { "系统启动后自动运行已启用" } else { "系统启动后自动运行已关闭" },
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Record process start time (used for uptime calculation)
// ---------------------------------------------------------------------------

static START_TIME: std::sync::OnceLock<u64> = std::sync::OnceLock::new();

pub fn record_start_time() {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    START_TIME.get_or_init(|| secs);
}

fn start_time_secs() -> u64 {
    START_TIME.get().copied().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lan_device(id: &str, mac: &str, global_ipv6: Vec<&str>) -> LanDevice {
        let global_ipv6 = global_ipv6
            .into_iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>();

        LanDevice {
            id: id.to_string(),
            display_name: id.to_string(),
            hostname: String::new(),
            mac: mac.to_string(),
            ipv4: Vec::new(),
            ipv6: global_ipv6.clone(),
            global_ipv6,
            online: true,
            source: "test".to_string(),
            last_seen: "2026-05-18T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn resolve_device_ipv6_accepts_selected_global_ipv6_when_present() {
        let config = DeviceDdnsConfig {
            device_id: "device-1".to_string(),
            selected_ipv6: "2408:8200::10".to_string(),
            ..DeviceDdnsConfig::default()
        };
        let devices = vec![lan_device(
            "device-1",
            "aa:bb:cc:dd:ee:ff",
            vec!["2408:8200::10", "2408:8200::11"],
        )];

        assert_eq!(
            resolve_device_ipv6(&config, &devices),
            Ok("2408:8200::10".to_string())
        );
    }

    #[test]
    fn resolve_device_ipv6_falls_back_to_first_global_ipv6_for_device_id_match() {
        let config = DeviceDdnsConfig {
            device_id: "device-1".to_string(),
            ..DeviceDdnsConfig::default()
        };
        let devices = vec![lan_device(
            "device-1",
            "aa:bb:cc:dd:ee:ff",
            vec!["2408:8200::10", "2408:8200::11"],
        )];

        assert_eq!(
            resolve_device_ipv6(&config, &devices),
            Ok("2408:8200::10".to_string())
        );
    }

    #[test]
    fn resolve_device_ipv6_returns_error_when_device_is_missing() {
        let config = DeviceDdnsConfig {
            device_id: "missing-device".to_string(),
            ..DeviceDdnsConfig::default()
        };
        let devices = vec![lan_device(
            "device-1",
            "aa:bb:cc:dd:ee:ff",
            vec!["2408:8200::10"],
        )];

        assert!(resolve_device_ipv6(&config, &devices).is_err());
    }

    #[test]
    fn resolve_device_ipv6_falls_back_when_selected_ipv6_disappeared() {
        let config = DeviceDdnsConfig {
            device_id: "device-1".to_string(),
            selected_ipv6: "2408:8200::old".to_string(),
            ..DeviceDdnsConfig::default()
        };
        let devices = vec![lan_device(
            "device-1",
            "aa:bb:cc:dd:ee:ff",
            vec!["2408:8200::new"],
        )];

        assert_eq!(
            resolve_device_ipv6(&config, &devices),
            Ok("2408:8200::new".to_string())
        );
    }

    #[test]
    fn device_ddns_identity_ignores_runtime_result_fields() {
        let original = DeviceDdnsConfig {
            enabled: true,
            provider: "aliyun".to_string(),
            access_key_id: "ak".to_string(),
            access_key_secret: "secret".to_string(),
            domain: "example.com".to_string(),
            sub_domain: "host".to_string(),
            ttl: 600,
            interval_minutes: 10,
            device_id: "device-1".to_string(),
            device_mac: "AA:BB:CC:DD:EE:FF".to_string(),
            selected_ipv6: "2408:8200::1".to_string(),
            last_update_time: "old".to_string(),
            last_result: "old result".to_string(),
            ..DeviceDdnsConfig::default()
        };
        let current = DeviceDdnsConfig {
            selected_ipv6: "2408:8200::2".to_string(),
            last_update_time: "new".to_string(),
            last_result: "new result".to_string(),
            ..original.clone()
        };

        let identity = device_ddns_identity(&original);

        assert!(!device_ddns_identity_matches(&current, &identity));

        let current_same_identity = DeviceDdnsConfig {
            last_update_time: "new".to_string(),
            last_result: "new result".to_string(),
            ..original.clone()
        };

        assert!(device_ddns_identity_matches(&current_same_identity, &identity));
    }
}
