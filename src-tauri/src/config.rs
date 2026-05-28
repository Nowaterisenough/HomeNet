use chrono::Local;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Data models – persisted to config.toml
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_version")]
    pub version: String,

    #[serde(default)]
    pub auto_start: bool,

    #[serde(default)]
    pub start_minimized: bool,

    #[serde(default = "default_log_level")]
    pub log_level: String,

    #[serde(default)]
    pub ipv6_interface: String,

    #[serde(default)]
    pub ddns: DdnsConfig,

    #[serde(default)]
    pub device_ddns: DeviceDdnsConfig,

    #[serde(default)]
    pub device_ddns_configs: Vec<DeviceDdnsConfig>,

    #[serde(default)]
    pub forward_rules: Vec<ForwardRule>,

    #[serde(default)]
    pub reverse_proxy_rules: Vec<ReverseProxyRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DdnsConfig {
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

    #[serde(default = "default_record_type")]
    pub record_type: String,

    #[serde(default = "default_ttl")]
    pub ttl: u32,

    #[serde(default = "default_interval_minutes")]
    pub interval_minutes: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

    #[serde(default = "default_record_type")]
    pub record_type: String,

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
    pub selected_ip: String,

    #[serde(default)]
    pub last_update_time: String,

    #[serde(default)]
    pub last_result: String,

    #[serde(default)]
    pub last_online: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForwardRule {
    #[serde(default)]
    pub id: String,

    #[serde(default)]
    pub enabled: bool,

    #[serde(default = "default_protocol")]
    pub protocol: String,

    #[serde(default = "default_listen_addr")]
    pub listen_addr: String,

    #[serde(default)]
    pub listen_port: u16,

    #[serde(default)]
    pub target_ip: String,

    #[serde(default)]
    pub target_port: u16,

    #[serde(default = "default_mode")]
    pub mode: String,

    #[serde(default)]
    pub remark: String,

    #[serde(default = "default_status")]
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReverseProxyRule {
    #[serde(default)]
    pub id: String,

    #[serde(default)]
    pub enabled: bool,

    #[serde(default = "default_reverse_proxy_protocol")]
    pub protocol: String,

    #[serde(default)]
    pub domain: String,

    #[serde(default = "default_listen_addr")]
    pub listen_addr: String,

    #[serde(default = "default_reverse_proxy_listen_port")]
    pub listen_port: u16,

    #[serde(default)]
    pub backend_ip: String,

    #[serde(default = "default_reverse_proxy_backend_port")]
    pub backend_port: u16,

    #[serde(default = "default_reverse_proxy_tls")]
    pub tls: String,

    #[serde(default)]
    pub certificate: String,

    #[serde(default)]
    pub acme_email: String,

    #[serde(default = "default_acme_dns_provider")]
    pub acme_dns_provider: String,

    #[serde(default)]
    pub acme_access_key_id: String,

    #[serde(default)]
    pub acme_access_key_secret: String,

    #[serde(default)]
    pub acme_dns_domain: String,

    #[serde(default = "default_acme_directory_url")]
    pub acme_directory_url: String,

    #[serde(default)]
    pub certificate_path: String,

    #[serde(default)]
    pub private_key_path: String,

    #[serde(default)]
    pub certificate_expires_at: String,

    #[serde(default)]
    pub certificate_last_issued_at: String,

    #[serde(default)]
    pub certificate_last_error: String,

    #[serde(default)]
    pub remark: String,

    #[serde(default = "default_status")]
    pub status: String,
}

// ---------------------------------------------------------------------------
// Data models – runtime (not persisted, used for IPC)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeStatus {
    pub version: String,
    pub public_ipv4: String,
    pub public_ipv6: String,
    pub ddns_status: String,
    pub last_update_time: String,
    pub rule_count: u32,
    pub enabled_rule_count: u32,
    pub reverse_proxy_rule_count: u32,
    pub enabled_reverse_proxy_rule_count: u32,
    pub uptime: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub id: String,
    pub time: String,
    pub level: String,
    pub module: String,
    pub message: String,
}

// ---------------------------------------------------------------------------
// Default value helpers
// ---------------------------------------------------------------------------

fn default_version() -> String {
    env!("CARGO_PKG_VERSION").into()
}

fn default_log_level() -> String {
    "info".into()
}

fn default_provider() -> String {
    "aliyun".into()
}

fn default_record_type() -> String {
    "AAAA".into()
}

fn default_ttl() -> u32 {
    600
}

fn default_interval_minutes() -> u32 {
    10
}

fn default_protocol() -> String {
    "TCP".into()
}

fn default_listen_addr() -> String {
    "::".into()
}

fn default_mode() -> String {
    "relay".into()
}

fn default_status() -> String {
    "正常".into()
}

fn default_reverse_proxy_protocol() -> String {
    "HTTP".into()
}

fn default_reverse_proxy_listen_port() -> u16 {
    80
}

fn default_reverse_proxy_backend_port() -> u16 {
    80
}

fn default_reverse_proxy_tls() -> String {
    "off".into()
}

fn default_acme_dns_provider() -> String {
    "aliyun".into()
}

fn default_acme_directory_url() -> String {
    "https://acme-v02.api.letsencrypt.org/directory".into()
}

fn normalize_reverse_proxy_tls(tls: &str, protocol: &str) -> String {
    let normalized = tls.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "auto" | "acme" => "auto".to_string(),
        "manual" => "manual".to_string(),
        "passthrough" if protocol == "HTTPS" => "passthrough".to_string(),
        "off" => "off".to_string(),
        _ if protocol == "HTTPS" => "passthrough".to_string(),
        _ => default_reverse_proxy_tls(),
    }
}

pub fn normalize_forward_protocol(protocol: &str) -> String {
    match protocol.trim().to_uppercase().replace('＋', "+").as_str() {
        "UDP" => "UDP".to_string(),
        "TCP+UDP" | "UDP+TCP" => "TCP+UDP".to_string(),
        _ => "TCP".to_string(),
    }
}

pub fn normalize_forward_rule(rule: &mut ForwardRule) -> bool {
    let original_protocol = rule.protocol.clone();
    let original_mode = rule.mode.clone();
    let original_listen_addr = rule.listen_addr.clone();
    let original_status = rule.status.clone();

    rule.protocol = normalize_forward_protocol(&rule.protocol);
    rule.mode = "relay".into();
    if rule.listen_addr.trim().is_empty() {
        rule.listen_addr = default_listen_addr();
    }
    if rule.status.trim().is_empty() {
        rule.status = if rule.enabled {
            default_status()
        } else {
            "已禁用".into()
        };
    }

    rule.protocol != original_protocol
        || rule.mode != original_mode
        || rule.listen_addr != original_listen_addr
        || rule.status != original_status
}

pub fn normalize_reverse_proxy_protocol(protocol: &str) -> String {
    match protocol.trim().to_uppercase().as_str() {
        "HTTPS" => "HTTPS".to_string(),
        _ => "HTTP".to_string(),
    }
}

pub fn normalize_reverse_proxy_rule(rule: &mut ReverseProxyRule) -> bool {
    let original_protocol = rule.protocol.clone();
    let original_domain = rule.domain.clone();
    let original_listen_addr = rule.listen_addr.clone();
    let original_listen_port = rule.listen_port;
    let original_backend_port = rule.backend_port;
    let original_tls = rule.tls.clone();
    let original_acme_dns_provider = rule.acme_dns_provider.clone();
    let original_acme_dns_domain = rule.acme_dns_domain.clone();
    let original_acme_directory_url = rule.acme_directory_url.clone();
    let original_status = rule.status.clone();

    rule.protocol = normalize_reverse_proxy_protocol(&rule.protocol);
    rule.domain = rule
        .domain
        .trim()
        .trim_end_matches('.')
        .to_ascii_lowercase();
    if rule.listen_addr.trim().is_empty() {
        rule.listen_addr = default_listen_addr();
    }
    if rule.listen_port == 0 {
        rule.listen_port = if rule.protocol == "HTTPS" { 443 } else { 80 };
    }
    if rule.backend_port == 0 {
        rule.backend_port = if rule.protocol == "HTTPS" { 443 } else { 80 };
    }
    rule.tls = normalize_reverse_proxy_tls(&rule.tls, &rule.protocol);
    rule.acme_dns_provider = rule.acme_dns_provider.trim().to_ascii_lowercase();
    if rule.acme_dns_provider.trim().is_empty() {
        rule.acme_dns_provider = default_acme_dns_provider();
    }
    rule.acme_dns_domain = rule
        .acme_dns_domain
        .trim()
        .trim_end_matches('.')
        .to_ascii_lowercase();
    if rule.acme_directory_url.trim().is_empty() {
        rule.acme_directory_url = default_acme_directory_url();
    } else {
        rule.acme_directory_url = rule.acme_directory_url.trim().to_string();
    }
    if rule.status.trim().is_empty() {
        rule.status = if rule.enabled {
            default_status()
        } else {
            "已禁用".into()
        };
    }

    rule.protocol != original_protocol
        || rule.domain != original_domain
        || rule.listen_addr != original_listen_addr
        || rule.listen_port != original_listen_port
        || rule.backend_port != original_backend_port
        || rule.tls != original_tls
        || rule.acme_dns_provider != original_acme_dns_provider
        || rule.acme_dns_domain != original_acme_dns_domain
        || rule.acme_directory_url != original_acme_directory_url
        || rule.status != original_status
}

fn clear_placeholder_device_ddns_config(config: &mut DeviceDdnsConfig) -> bool {
    let uses_placeholder_credentials = config.access_key_id.trim() == "LTAI5****************"
        || config.access_key_secret.trim() == "************************";
    let uses_placeholder_domain =
        config.domain.trim().eq_ignore_ascii_case("example.com")
            || config.sub_domain.trim() == "nas,web,home";

    if !uses_placeholder_credentials && !uses_placeholder_domain {
        return false;
    }

    config.enabled = false;
    config.access_key_id.clear();
    config.access_key_secret.clear();
    config.domain.clear();
    config.sub_domain.clear();
    config.last_update_time.clear();
    config.last_result.clear();
    config.last_online = false;
    true
}

fn normalize_device_ddns_config(config: &mut DeviceDdnsConfig) -> bool {
    let mut changed = false;
    let record_type = config.record_type.trim().to_uppercase();
    if record_type != "A" && record_type != "AAAA" {
        config.record_type = default_record_type();
        changed = true;
    } else if config.record_type != record_type {
        config.record_type = record_type;
        changed = true;
    }

    if config.selected_ip.trim().is_empty() && !config.selected_ipv6.trim().is_empty() {
        config.selected_ip = config.selected_ipv6.trim().to_string();
        changed = true;
    }

    changed
}

fn device_ddns_has_device_selector(config: &DeviceDdnsConfig) -> bool {
    !config.device_id.trim().is_empty() || !config.device_mac.trim().is_empty()
}

fn device_ddns_targets_same_device(left: &DeviceDdnsConfig, right: &DeviceDdnsConfig) -> bool {
    let left_id = left.device_id.trim();
    let right_id = right.device_id.trim();
    if !left_id.is_empty() && !right_id.is_empty() && left_id == right_id {
        return true;
    }

    let left_mac = left.device_mac.trim();
    let right_mac = right.device_mac.trim();
    !left_mac.is_empty() && !right_mac.is_empty() && left_mac.eq_ignore_ascii_case(right_mac)
}

fn migrate_legacy_device_ddns_config(config: &mut AppConfig) -> bool {
    if !device_ddns_has_device_selector(&config.device_ddns) {
        return false;
    }
    if config
        .device_ddns_configs
        .iter()
        .any(|item| device_ddns_targets_same_device(item, &config.device_ddns))
    {
        return false;
    }

    config.device_ddns_configs.push(config.device_ddns.clone());
    true
}

fn dedupe_device_ddns_configs(configs: &mut Vec<DeviceDdnsConfig>) -> bool {
    let mut changed = false;
    let mut deduped: Vec<DeviceDdnsConfig> = Vec::new();

    for config in std::mem::take(configs) {
        if !device_ddns_has_device_selector(&config) {
            changed = true;
            continue;
        }

        if let Some(existing) = deduped
            .iter_mut()
            .find(|item| device_ddns_targets_same_device(item, &config))
        {
            *existing = config;
            changed = true;
        } else {
            deduped.push(config);
        }
    }

    *configs = deduped;
    changed
}

fn normalize_app_config(config: &mut AppConfig) -> bool {
    let mut changed = false;
    changed |= clear_placeholder_device_ddns_config(&mut config.device_ddns);
    changed |= normalize_device_ddns_config(&mut config.device_ddns);
    for device_config in &mut config.device_ddns_configs {
        changed |= clear_placeholder_device_ddns_config(device_config);
        changed |= normalize_device_ddns_config(device_config);
    }
    changed |= migrate_legacy_device_ddns_config(config);
    changed |= dedupe_device_ddns_configs(&mut config.device_ddns_configs);
    for rule in &mut config.forward_rules {
        changed |= normalize_forward_rule(rule);
    }
    for rule in &mut config.reverse_proxy_rules {
        changed |= normalize_reverse_proxy_rule(rule);
    }
    changed
}

// ---------------------------------------------------------------------------
// Default AppConfig (used when no config file exists)
// ---------------------------------------------------------------------------

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: default_version(),
            auto_start: false,
            start_minimized: false,
            log_level: default_log_level(),
            ipv6_interface: String::new(),
            ddns: DdnsConfig::default(),
            device_ddns: DeviceDdnsConfig::default(),
            device_ddns_configs: Vec::new(),
            forward_rules: Vec::new(),
            reverse_proxy_rules: Vec::new(),
        }
    }
}

impl Default for DdnsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: default_provider(),
            access_key_id: String::new(),
            access_key_secret: String::new(),
            domain: String::new(),
            sub_domain: String::new(),
            record_type: default_record_type(),
            ttl: default_ttl(),
            interval_minutes: default_interval_minutes(),
        }
    }
}

impl Default for DeviceDdnsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: default_provider(),
            access_key_id: String::new(),
            access_key_secret: String::new(),
            domain: String::new(),
            sub_domain: String::new(),
            record_type: default_record_type(),
            ttl: default_ttl(),
            interval_minutes: default_interval_minutes(),
            device_id: String::new(),
            device_mac: String::new(),
            device_name: String::new(),
            selected_ipv6: String::new(),
            selected_ip: String::new(),
            last_update_time: String::new(),
            last_result: String::new(),
            last_online: false,
        }
    }
}

impl Default for ReverseProxyRule {
    fn default() -> Self {
        Self {
            id: String::new(),
            enabled: false,
            protocol: default_reverse_proxy_protocol(),
            domain: String::new(),
            listen_addr: default_listen_addr(),
            listen_port: default_reverse_proxy_listen_port(),
            backend_ip: String::new(),
            backend_port: default_reverse_proxy_backend_port(),
            tls: default_reverse_proxy_tls(),
            certificate: String::new(),
            acme_email: String::new(),
            acme_dns_provider: default_acme_dns_provider(),
            acme_access_key_id: String::new(),
            acme_access_key_secret: String::new(),
            acme_dns_domain: String::new(),
            acme_directory_url: default_acme_directory_url(),
            certificate_path: String::new(),
            private_key_path: String::new(),
            certificate_expires_at: String::new(),
            certificate_last_issued_at: String::new(),
            certificate_last_error: String::new(),
            remark: String::new(),
            status: "已禁用".into(),
        }
    }
}

// ---------------------------------------------------------------------------
// Config path resolution
// ---------------------------------------------------------------------------

fn config_dir() -> PathBuf {
    let base = dirs_next::config_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("home-net")
}

fn config_file_path() -> PathBuf {
    config_dir().join("config.toml")
}

pub fn log_dir() -> PathBuf {
    // Re-use the same app directory; logs go alongside config.
    config_dir()
}

/// Ensure the configuration directory exists, creating it if necessary.
fn ensure_config_dir() -> std::io::Result<()> {
    let dir = config_dir();
    fs::create_dir_all(&dir)
}

// ---------------------------------------------------------------------------
// Config load / save
// ---------------------------------------------------------------------------

/// Load configuration from the TOML file.
/// If the file does not exist a default config is created, saved, and returned.
pub fn load_config() -> AppConfig {
    let path = config_file_path();

    match fs::read_to_string(&path) {
        Ok(content) => match toml::from_str::<AppConfig>(&content) {
            Ok(mut cfg) => {
                add_log("info", "配置", "已从磁盘加载配置");
                if normalize_app_config(&mut cfg) {
                    if let Err(e) = save_config(&cfg) {
                        add_log(
                            "warn",
                            "配置",
                            &format!("旧版转发规则配置迁移写入失败：{}", e),
                        );
                    } else {
                        add_log("info", "配置", "已迁移旧版转发规则配置");
                    }
                }
                cfg
            }
            Err(e) => {
                add_log(
                    "warn",
                    "配置",
                    &format!("配置文件解析失败：{}，已使用默认配置", e),
                );
                AppConfig::default()
            }
        },
        Err(_) => {
            // Config file does not exist – create a default one.
            let default_cfg = AppConfig::default();
            if let Err(e) = save_config(&default_cfg) {
                add_log("warn", "配置", &format!("默认配置写入失败：{}", e));
            } else {
                add_log("info", "配置", "已创建默认配置文件");
            }
            default_cfg
        }
    }
}

/// Atomically save configuration to the TOML file.
/// Writes to a temporary file first, then renames.
pub fn save_config(config: &AppConfig) -> Result<(), String> {
    ensure_config_dir().map_err(|e| format!("创建配置目录失败：{}", e))?;

    let toml_str = toml::to_string_pretty(config).map_err(|e| format!("序列化配置失败：{}", e))?;

    let path = config_file_path();
    let tmp_path = path.with_extension("toml.tmp");

    let mut f = fs::File::create(&tmp_path).map_err(|e| format!("创建临时配置文件失败：{}", e))?;
    f.write_all(toml_str.as_bytes())
        .map_err(|e| format!("写入临时配置文件失败：{}", e))?;
    f.flush()
        .map_err(|e| format!("刷新临时配置文件失败：{}", e))?;

    fs::rename(&tmp_path, &path).map_err(|e| format!("替换配置文件失败：{}", e))?;

    add_log("info", "配置", "配置已保存到磁盘");
    Ok(())
}

// ---------------------------------------------------------------------------
// In-memory log buffer
// ---------------------------------------------------------------------------

static LOG_BUFFER: OnceLock<Mutex<Vec<LogEntry>>> = OnceLock::new();
const LOG_BUFFER_CAPACITY: usize = 1000;

fn get_log_buffer() -> &'static Mutex<Vec<LogEntry>> {
    LOG_BUFFER.get_or_init(|| Mutex::new(Vec::new()))
}

/// Add a log entry to the in-memory ring buffer.
pub fn add_log(level: &str, module: &str, message: &str) {
    let entry = LogEntry {
        id: Uuid::new_v4().to_string(),
        time: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        level: level.to_string(),
        module: module.to_string(),
        message: message.to_string(),
    };

    // Also emit via tracing for file / console output.
    match level {
        "error" => tracing::error!("[{}] {}", module, message),
        "warn" => tracing::warn!("[{}] {}", module, message),
        "info" => tracing::info!("[{}] {}", module, message),
        _ => tracing::debug!("[{}] {}", module, message),
    }

    let buffer = get_log_buffer();
    if let Ok(mut guard) = buffer.lock() {
        while guard.len() >= LOG_BUFFER_CAPACITY {
            guard.remove(0);
        }
        guard.push(entry);
    }
}

/// Return a snapshot of all in-memory log entries (newest last).
pub fn get_logs() -> Vec<LogEntry> {
    let buffer = get_log_buffer();
    if let Ok(guard) = buffer.lock() {
        guard.clone()
    } else {
        Vec::new()
    }
}

/// Clear all in-memory log entries.
pub fn clear_logs() {
    let buffer = get_log_buffer();
    if let Ok(mut guard) = buffer.lock() {
        guard.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_ddns_default_uses_ddns_defaults_and_empty_device_fields() {
        let config = DeviceDdnsConfig::default();

        assert!(!config.enabled);
        assert_eq!(config.provider, "aliyun");
        assert_eq!(config.ttl, 600);
        assert_eq!(config.interval_minutes, 10);
        assert_eq!(config.record_type, "AAAA");
        assert_eq!(config.device_id, "");
        assert_eq!(config.device_mac, "");
        assert_eq!(config.device_name, "");
        assert_eq!(config.selected_ipv6, "");
        assert_eq!(config.selected_ip, "");
        assert_eq!(config.last_update_time, "");
        assert_eq!(config.last_result, "");
        assert!(!config.last_online);
    }

    #[test]
    fn app_config_deserializes_when_device_ddns_is_absent() {
        let toml = r#"
version = "0.1.0"
auto_start = false
start_minimized = false
log_level = "info"
ipv6_interface = ""
forward_rules = []

[ddns]
enabled = false
provider = "aliyun"
access_key_id = ""
access_key_secret = ""
domain = ""
sub_domain = ""
record_type = "AAAA"
ttl = 600
interval_minutes = 10
"#;

        let config: AppConfig = toml::from_str(toml).expect("missing device_ddns uses default");

        assert!(!config.device_ddns.enabled);
        assert_eq!(config.device_ddns.provider, "aliyun");
        assert_eq!(config.device_ddns.ttl, 600);
        assert_eq!(config.device_ddns.interval_minutes, 10);
    }

    #[test]
    fn app_config_normalization_migrates_legacy_device_ddns_into_device_list() {
        let mut config = AppConfig::default();
        config.device_ddns.enabled = true;
        config.device_ddns.device_id = "local-machine".to_string();
        config.device_ddns.device_name = "本机设备".to_string();
        config.device_ddns.domain = "bytech.uno".to_string();
        config.device_ddns.sub_domain = "game".to_string();

        assert!(normalize_app_config(&mut config));
        assert_eq!(config.device_ddns_configs.len(), 1);
        assert_eq!(config.device_ddns_configs[0].device_id, "local-machine");
        assert_eq!(config.device_ddns_configs[0].device_name, "本机设备");
    }

    #[test]
    fn reverse_proxy_defaults_to_no_rules() {
        let config = AppConfig::default();

        assert!(config.reverse_proxy_rules.is_empty());
    }

    #[test]
    fn app_config_deserializes_when_reverse_proxy_rules_are_absent() {
        let toml = r#"
version = "0.1.0"
auto_start = false
start_minimized = false
log_level = "info"
ipv6_interface = ""
forward_rules = []

[ddns]
enabled = false
provider = "aliyun"
access_key_id = ""
access_key_secret = ""
domain = ""
sub_domain = ""
record_type = "AAAA"
ttl = 600
interval_minutes = 10
"#;

        let config: AppConfig =
            toml::from_str(toml).expect("missing reverse proxy rules uses default");

        assert!(config.reverse_proxy_rules.is_empty());
    }

    #[test]
    fn reverse_proxy_rule_default_is_disabled_http_rule() {
        let rule = ReverseProxyRule::default();

        assert!(!rule.enabled);
        assert_eq!(rule.protocol, "HTTP");
        assert_eq!(rule.listen_addr, "::");
        assert_eq!(rule.listen_port, 80);
        assert_eq!(rule.backend_port, 80);
    }

    #[test]
    fn reverse_proxy_rule_default_has_empty_certificate_automation_fields() {
        let rule = ReverseProxyRule::default();

        assert_eq!(rule.tls, "off");
        assert_eq!(rule.acme_email, "");
        assert_eq!(rule.acme_dns_provider, "aliyun");
        assert_eq!(rule.acme_access_key_id, "");
        assert_eq!(rule.acme_access_key_secret, "");
        assert_eq!(rule.acme_dns_domain, "");
        assert_eq!(rule.acme_directory_url, "https://acme-v02.api.letsencrypt.org/directory");
        assert_eq!(rule.certificate_path, "");
        assert_eq!(rule.private_key_path, "");
        assert_eq!(rule.certificate_expires_at, "");
        assert_eq!(rule.certificate_last_issued_at, "");
        assert_eq!(rule.certificate_last_error, "");
    }

    #[test]
    fn normalize_reverse_proxy_rule_accepts_certificate_tls_modes() {
        let mut rule = ReverseProxyRule {
            protocol: "https".to_string(),
            domain: "Proxy.Example.COM.".to_string(),
            tls: "AUTO".to_string(),
            ..ReverseProxyRule::default()
        };

        assert!(normalize_reverse_proxy_rule(&mut rule));

        assert_eq!(rule.protocol, "HTTPS");
        assert_eq!(rule.domain, "proxy.example.com");
        assert_eq!(rule.tls, "auto");
        assert_eq!(rule.listen_port, 443);
    }

    #[test]
    fn app_config_normalization_clears_placeholder_device_ddns_values() {
        let mut config = AppConfig::default();
        config.device_ddns.enabled = true;
        config.device_ddns.access_key_id = "LTAI5****************".to_string();
        config.device_ddns.access_key_secret = "************************".to_string();
        config.device_ddns.domain = "example.com".to_string();
        config.device_ddns.sub_domain = "nas,web,home".to_string();
        config.device_ddns.device_id = "device-1".to_string();

        assert!(normalize_app_config(&mut config));
        assert!(!config.device_ddns.enabled);
        assert_eq!(config.device_ddns.access_key_id, "");
        assert_eq!(config.device_ddns.access_key_secret, "");
        assert_eq!(config.device_ddns.domain, "");
        assert_eq!(config.device_ddns.sub_domain, "");
        assert_eq!(config.device_ddns.device_id, "device-1");
    }
}
