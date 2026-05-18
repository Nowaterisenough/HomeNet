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
    pub forward_rules: Vec<ForwardRule>,
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

// ---------------------------------------------------------------------------
// Data models – runtime (not persisted, used for IPC)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeStatus {
    pub public_ipv4: String,
    pub public_ipv6: String,
    pub ddns_status: String,
    pub last_update_time: String,
    pub rule_count: u32,
    pub enabled_rule_count: u32,
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

fn normalize_app_config(config: &mut AppConfig) -> bool {
    let mut changed = false;
    for rule in &mut config.forward_rules {
        changed |= normalize_forward_rule(rule);
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
            forward_rules: Vec::new(),
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
        assert_eq!(config.device_id, "");
        assert_eq!(config.device_mac, "");
        assert_eq!(config.device_name, "");
        assert_eq!(config.selected_ipv6, "");
        assert_eq!(config.last_update_time, "");
        assert_eq!(config.last_result, "");
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
}
