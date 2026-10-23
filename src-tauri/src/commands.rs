use chrono::Local;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::Mutex,
    time::Duration,
};
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_updater::{Update, Updater, UpdaterExt};
use tokio::sync::Mutex as TokioMutex;
use url::Url;
use uuid::Uuid;

use crate::config::{
    add_log, normalize_forward_rule, normalize_reverse_proxy_rule, save_config, AppConfig,
    DdnsConfig, DeviceDdnsConfig, ForwardRule, LogEntry, ReverseProxyRule, RuntimeStatus,
    WebDavAccount, WebDavConfig,
};
use crate::ddns::NetworkInterfaceInfo;
use crate::device_discovery::LanDevice;
use crate::forward::manager::ForwardManager;
use crate::reverse_proxy::ReverseProxyManager;
use crate::webdav::{
    hash_webdav_password, probe_webdav_listener, validate_webdav_config, WebDavManager,
    WebDavRuntimeStatus,
};

// ---------------------------------------------------------------------------
// App state shared across all Tauri commands
// ---------------------------------------------------------------------------

pub struct AppState {
    pub config: Mutex<AppConfig>,
    pub forward_manager: TokioMutex<ForwardManager>,
    pub reverse_proxy_manager: TokioMutex<ReverseProxyManager>,
    pub webdav_manager: TokioMutex<WebDavManager>,
}

const UPDATE_MANIFEST_URL: &str =
    "https://github.com/Nowaterisenough/HomeNet/releases/latest/download/latest.json";
const GITHUB_LATEST_RELEASE_API: &str =
    "https://api.github.com/repos/Nowaterisenough/HomeNet/releases/latest";
const GITHUB_REPOSITORY_URL: &str = "https://github.com/Nowaterisenough/HomeNet/";
const UPDATE_USER_AGENT: &str = concat!("HomeNet/", env!("CARGO_PKG_VERSION"));
#[cfg(any(target_os = "macos", test))]
const MACOS_APPLICATION_PATH: &str = "/Applications/HomeNet.app";
#[cfg(any(target_os = "macos", test))]
const MACOS_QUARANTINE_ATTRIBUTE: &str = "com.apple.quarantine";

#[derive(Debug, Clone, serde::Serialize)]
pub struct AppUpdateResult {
    pub status: String,
    pub current_version: String,
    pub latest_version: String,
    pub message: String,
    pub notes: String,
    pub verification: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SaveWebDavConfigRequest {
    pub enabled: bool,
    pub listen_addr: String,
    pub listen_port: u16,
    pub root_path: String,
    pub global_read_only: bool,
    #[serde(default)]
    pub accounts: Vec<SaveWebDavAccountRequest>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SaveWebDavAccountRequest {
    #[serde(default)]
    pub id: String,
    pub username: String,
    #[serde(default)]
    pub password: String,
    pub read_only: bool,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct WebDavConfigView {
    pub enabled: bool,
    pub listen_addr: String,
    pub listen_port: u16,
    pub root_path: String,
    pub global_read_only: bool,
    pub accounts: Vec<WebDavAccountView>,
    pub status: WebDavRuntimeStatus,
}

#[derive(Debug, Clone, Serialize)]
pub struct WebDavAccountView {
    pub id: String,
    pub username: String,
    pub read_only: bool,
    pub enabled: bool,
    pub password_configured: bool,
    pub last_used: String,
}

#[derive(Debug, Clone, Deserialize)]
struct GithubRelease {
    tag_name: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    assets: Vec<GithubReleaseAsset>,
    #[serde(skip)]
    asset_urls_inferred: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct GithubReleaseAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UpdateAssetKind {
    WindowsX64Setup,
    MacosArm64AppZip,
}

impl UpdateAssetKind {
    fn asset_suffix(self) -> &'static str {
        match self {
            Self::WindowsX64Setup => "_windows-x64-setup.exe",
            Self::MacosArm64AppZip => "_macos-arm64-app.zip",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::WindowsX64Setup => "Windows x64 安装包",
            Self::MacosArm64AppZip => "macOS ARM64 应用包",
        }
    }
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

fn parse_release_version(value: &str) -> Option<Version> {
    let value = value.trim();
    Version::parse(value.strip_prefix('v').unwrap_or(value)).ok()
}

pub(crate) fn is_release_version_newer(current_version: &str, latest_version: &str) -> bool {
    let Some(current) = parse_release_version(current_version) else {
        return false;
    };
    let Some(latest) = parse_release_version(latest_version) else {
        return false;
    };
    latest > current
}

fn update_error_message(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn ensure_update_platform_supported() -> Result<(), String> {
    if (cfg!(target_os = "windows") && cfg!(target_arch = "x86_64"))
        || (cfg!(target_os = "macos") && cfg!(target_arch = "aarch64"))
    {
        Ok(())
    } else {
        Err("当前平台暂不支持应用内更新".to_string())
    }
}

fn embedded_updater_public_key() -> Option<&'static str> {
    let public_key = option_env!("HOMENET_UPDATER_PUBLIC_KEY")
        .unwrap_or_default()
        .trim();
    if public_key.is_empty() {
        None
    } else {
        Some(public_key)
    }
}

fn configured_app_updater(app: &AppHandle) -> Result<Updater, String> {
    ensure_update_platform_supported()?;
    let endpoint = Url::parse(UPDATE_MANIFEST_URL).map_err(update_error_message)?;
    app.updater_builder()
        .endpoints(vec![endpoint])
        .map_err(update_error_message)?
        .pubkey(
            embedded_updater_public_key()
                .ok_or_else(|| "当前构建未嵌入更新签名公钥".to_string())?,
        )
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(update_error_message)
}

async fn available_app_update(app: &AppHandle) -> Result<Option<Update>, String> {
    configured_app_updater(app)?
        .check()
        .await
        .map_err(update_error_message)
}

fn current_update_asset_kind() -> Result<UpdateAssetKind, String> {
    if cfg!(target_os = "windows") && cfg!(target_arch = "x86_64") {
        return Ok(UpdateAssetKind::WindowsX64Setup);
    }
    if cfg!(target_os = "macos") && cfg!(target_arch = "aarch64") {
        return Ok(UpdateAssetKind::MacosArm64AppZip);
    }
    Err("当前平台暂不支持应用内更新".to_string())
}

fn select_github_update_asset(
    release: &GithubRelease,
    kind: UpdateAssetKind,
) -> Option<&GithubReleaseAsset> {
    release
        .assets
        .iter()
        .find(|asset| asset.name.ends_with(kind.asset_suffix()))
}

async fn fetch_latest_github_release(client: &reqwest::Client) -> Result<GithubRelease, String> {
    let repository = Url::parse(GITHUB_REPOSITORY_URL).map_err(update_error_message)?;
    fetch_github_release_from(client, GITHUB_LATEST_RELEASE_API, &repository).await
}

async fn fetch_github_release_from(
    client: &reqwest::Client,
    api_url: &str,
    repository: &Url,
) -> Result<GithubRelease, String> {
    match fetch_github_release_api(client, api_url).await {
        Ok(release) => return Ok(release),
        Err(api_error) => {
            add_log("warn", "更新", &format!("{api_error}，尝试官方发布页"));
            let result = async {
                let response = client
                    .head(
                        repository
                            .join("releases/latest")
                            .map_err(update_error_message)?,
                    )
                    .header(reqwest::header::USER_AGENT, UPDATE_USER_AGENT)
                    .timeout(Duration::from_secs(15))
                    .send()
                    .await
                    .map_err(update_error_message)?;
                if !response.status().is_success() {
                    return Err(format!("官方发布页返回 HTTP {}", response.status()));
                }
                release_from_official_page(response.url(), repository)
            }
            .await;
            result.map_err(|error| {
                format!("{api_error}；官方发布页回退失败：{error}。请检查网络或代理后重试")
            })
        }
    }
}

async fn fetch_github_release_api(
    client: &reqwest::Client,
    api_url: &str,
) -> Result<GithubRelease, String> {
    let response = client
        .get(api_url)
        .header(reqwest::header::USER_AGENT, UPDATE_USER_AGENT)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(update_error_message)?;
    let status = response.status();
    if !status.is_success() {
        let limited = status.as_u16() == 429
            || response
                .headers()
                .get("x-ratelimit-remaining")
                .is_some_and(|value| value == "0");
        return Err(if limited {
            format!("GitHub 版本接口请求额度已用尽（HTTP {status}）")
        } else {
            format!("GitHub 版本接口返回 HTTP {status}")
        });
    }
    let release: GithubRelease = response.json().await.map_err(update_error_message)?;
    if parse_release_version(&release.tag_name).is_none() {
        return Err("GitHub 版本接口返回了无效版本号".into());
    }
    Ok(release)
}

fn release_from_official_page(url: &Url, repository: &Url) -> Result<GithubRelease, String> {
    let prefix = format!("{}releases/tag/", repository.path());
    let tag = url.path().strip_prefix(&prefix).unwrap_or_default();
    if url.origin() != repository.origin()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || parse_release_version(tag).is_none()
        || tag.contains('/')
    {
        return Err("官方发布页未返回有效的本仓库版本地址".into());
    }
    // These names are defined by our release workflow. Verify the selected asset
    // before offering installation; a tag alone does not prove an upload exists.
    let assets = [
        UpdateAssetKind::WindowsX64Setup,
        UpdateAssetKind::MacosArm64AppZip,
    ]
    .into_iter()
    .map(|kind| {
        let name = format!("HomeNet_{tag}{}", kind.asset_suffix());
        let url = repository
            .join(&format!("releases/download/{tag}/{name}"))
            .map_err(update_error_message)?;
        Ok(GithubReleaseAsset {
            name,
            browser_download_url: url.into(),
        })
    })
    .collect::<Result<Vec<_>, String>>()?;
    Ok(GithubRelease {
        tag_name: tag.into(),
        body: String::new(),
        assets,
        asset_urls_inferred: true,
    })
}

async fn verify_inferred_update_asset(
    client: &reqwest::Client,
    release: &GithubRelease,
    asset: &GithubReleaseAsset,
) -> Result<(), String> {
    if !release.asset_urls_inferred {
        return Ok(());
    }
    let response = client
        .head(&asset.browser_download_url)
        .header(reqwest::header::USER_AGENT, UPDATE_USER_AGENT)
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(update_error_message)?;
    if !response.status().is_success() {
        return Err(format!(
            "版本 {} 的安装包 {} 暂不可用（HTTP {}），请稍后重试",
            release.tag_name,
            asset.name,
            response.status()
        ));
    }
    Ok(())
}

fn validate_github_asset_url(value: &str) -> Result<(), String> {
    let url = Url::parse(value).map_err(update_error_message)?;
    if url.scheme() != "https"
        || url.host_str() != Some("github.com")
        || url.port_or_known_default() != Some(443)
        || url.query().is_some()
        || url.fragment().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || !url
            .path()
            .starts_with("/Nowaterisenough/HomeNet/releases/download/")
    {
        return Err("更新包地址不属于 HomeNet 官方 HTTPS 发布源".into());
    }
    Ok(())
}

async fn download_github_update_asset(
    client: &reqwest::Client,
    asset: &GithubReleaseAsset,
) -> Result<PathBuf, String> {
    validate_github_asset_url(&asset.browser_download_url)?;
    let response = client
        .get(&asset.browser_download_url)
        .header(reqwest::header::USER_AGENT, UPDATE_USER_AGENT)
        .timeout(Duration::from_secs(300))
        .send()
        .await
        .map_err(update_error_message)?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("下载更新失败：GitHub 返回 HTTP {}", status));
    }

    let bytes = response.bytes().await.map_err(update_error_message)?;
    let update_dir = std::env::temp_dir().join(format!("homenet-update-{}", Uuid::new_v4()));
    fs::create_dir_all(&update_dir).map_err(|error| format!("创建更新临时目录失败：{}", error))?;
    let target = update_dir.join(sanitize_update_asset_filename(&asset.name));
    tokio::fs::write(&target, bytes)
        .await
        .map_err(|error| format!("写入更新包失败：{}", error))?;
    Ok(target)
}

fn sanitize_update_asset_filename(name: &str) -> String {
    let sanitized = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    if sanitized.trim_matches('_').is_empty() {
        "HomeNet-update.bin".to_string()
    } else {
        sanitized
    }
}

fn start_unsigned_update_installer(
    kind: UpdateAssetKind,
    package_path: &Path,
) -> Result<(), String> {
    match kind {
        UpdateAssetKind::WindowsX64Setup => start_unsigned_windows_installer(package_path),
        UpdateAssetKind::MacosArm64AppZip => start_unsigned_macos_installer(package_path),
    }
}

#[cfg(target_os = "windows")]
fn start_unsigned_windows_installer(package_path: &Path) -> Result<(), String> {
    Command::new(package_path)
        .arg("/S")
        .spawn()
        .map_err(|error| format!("启动 Windows 静默安装失败：{}", error))?;
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn start_unsigned_windows_installer(_package_path: &Path) -> Result<(), String> {
    Err("当前平台不能运行 Windows 安装包".to_string())
}

#[cfg(target_os = "macos")]
fn start_unsigned_macos_installer(package_path: &Path) -> Result<(), String> {
    let script = build_unsigned_macos_update_script(package_path);
    let script_path = package_path.with_file_name("install-homenet-update.sh");
    fs::write(&script_path, script)
        .map_err(|error| format!("写入 macOS 更新脚本失败：{}", error))?;
    Command::new("sh")
        .arg(&script_path)
        .spawn()
        .map_err(|error| format!("启动 macOS 更新脚本失败：{}", error))?;
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn start_unsigned_macos_installer(_package_path: &Path) -> Result<(), String> {
    Err("当前平台不能运行 macOS 更新脚本".to_string())
}

#[cfg(any(target_os = "macos", test))]
fn build_unsigned_macos_update_script(package_path: &Path) -> String {
    let package_path = shell_quote(&package_path.to_string_lossy());
    format!(
        r#"#!/bin/bash
set -e

ZIP_PATH={package_path}
APP_PATH="{MACOS_APPLICATION_PATH}"
WORK_DIR="$(mktemp -d /tmp/homenet-update.XXXXXX)"

cleanup() {{
  rm -rf "$WORK_DIR"
  rm -f "$ZIP_PATH"
}}
trap cleanup EXIT

while pgrep -x "HomeNet" >/dev/null 2>&1; do
  sleep 1
done

ditto -x -k "$ZIP_PATH" "$WORK_DIR"
FOUND_APP="$(find "$WORK_DIR" -maxdepth 3 -name 'HomeNet.app' -type d | head -n 1)"
if [ -z "$FOUND_APP" ]; then
  exit 1
fi

if ! {{ rm -rf "$APP_PATH" && ditto "$FOUND_APP" "$APP_PATH"; }}; then
  export FOUND_APP
  osascript <<'APPLESCRIPT'
set sourceApp to system attribute "FOUND_APP"
do shell script "rm -rf /Applications/HomeNet.app && ditto " & quoted form of sourceApp & " /Applications/HomeNet.app" with administrator privileges
APPLESCRIPT
fi

if ! xattr -dr {MACOS_QUARANTINE_ATTRIBUTE} {MACOS_APPLICATION_PATH} 2>/dev/null; then
  if sudo -n true 2>/dev/null; then
    sudo xattr -dr {MACOS_QUARANTINE_ATTRIBUTE} {MACOS_APPLICATION_PATH} || true
  else
    osascript -e '{apple_script}' || true
  fi
fi

ATTEMPT=1
while [ "$ATTEMPT" -le 5 ]; do
  open -n "$APP_PATH" >/dev/null 2>&1 || true
  sleep 1
  if pgrep -x "HomeNet" >/dev/null 2>&1; then
    exit 0
  fi
  ATTEMPT=$((ATTEMPT + 1))
done
exit 1
"#,
        apple_script = macos_quarantine_apple_script().replace('\'', "'\\''")
    )
}

#[cfg(any(target_os = "macos", test))]
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn update_versions_match(expected_version: &str, actual_version: &str) -> bool {
    match (
        parse_release_version(expected_version),
        parse_release_version(actual_version),
    ) {
        (Some(expected), Some(actual)) => expected == actual,
        _ => false,
    }
}

#[cfg(any(target_os = "macos", test))]
fn macos_quarantine_apple_script() -> String {
    format!(
        "do shell script \"xattr -dr {MACOS_QUARANTINE_ATTRIBUTE} {MACOS_APPLICATION_PATH}\" with administrator privileges"
    )
}

#[cfg(target_os = "macos")]
fn clear_macos_application_quarantine() -> Result<(), String> {
    let sudo_available = Command::new("sudo")
        .args(["-n", "true"])
        .status()
        .is_ok_and(|status| status.success());

    if sudo_available {
        let status = Command::new("sudo")
            .args([
                "xattr",
                "-dr",
                MACOS_QUARANTINE_ATTRIBUTE,
                MACOS_APPLICATION_PATH,
            ])
            .status()
            .map_err(|error| format!("执行 sudo xattr 失败：{}", error))?;
        if status.success() {
            return Ok(());
        }
    }

    let status = Command::new("osascript")
        .args(["-e", &macos_quarantine_apple_script()])
        .status()
        .map_err(|error| format!("请求管理员授权解除应用隔离失败：{}", error))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "解除应用隔离失败，请手动执行：sudo xattr -dr {} {}",
            MACOS_QUARANTINE_ATTRIBUTE, MACOS_APPLICATION_PATH
        ))
    }
}

fn ddns_domain(config: &DdnsConfig) -> String {
    let domain = config.domain.trim();
    let sub_domain = config.sub_domain.trim();
    if domain.is_empty() {
        "未配置域名".to_string()
    } else if sub_domain.is_empty() || sub_domain == "@" {
        domain.to_string()
    } else {
        format!("{}.{}", sub_domain, domain)
    }
}

fn validate_ddns_config(config: &DdnsConfig) -> Result<(), String> {
    if config.access_key_id.trim().is_empty() || config.access_key_secret.trim().is_empty() {
        return Err("AccessKey ID 或 Secret 未配置".to_string());
    }
    if config.domain.trim().is_empty() {
        return Err("主域名未配置".to_string());
    }
    Ok(())
}

pub(crate) fn device_ddns_domain(config: &DeviceDdnsConfig) -> String {
    let domain = config.domain.trim();
    let sub_domain = config.sub_domain.trim();
    if domain.is_empty() {
        "未配置域名".to_string()
    } else if sub_domain.is_empty() || sub_domain == "@" {
        domain.to_string()
    } else {
        format!("{}.{}", sub_domain, domain)
    }
}

pub(crate) fn validate_device_ddns_config(config: &DeviceDdnsConfig) -> Result<(), String> {
    if config.access_key_id.trim().is_empty() || config.access_key_secret.trim().is_empty() {
        return Err("AccessKey ID 或 Secret 未配置".to_string());
    }
    if config.domain.trim().is_empty() {
        return Err("主域名未配置".to_string());
    }
    let record_type = config.record_type.trim().to_uppercase();
    if record_type != "A" && record_type != "AAAA" {
        return Err("记录类型仅支持 A 或 AAAA".to_string());
    }
    if !valid_binding_mac(&config.device_mac) {
        return Err("请填写真实设备 MAC 地址，不能使用系统占位值、全零、广播或组播地址".to_string());
    }
    if config.domain.contains([',', '/', '\\', ':', '?', '#'])
        || config.domain.trim().chars().any(char::is_whitespace)
        || config.sub_domain.contains([',', '/', '\\', ':', '?', '#'])
        || config.sub_domain.trim().chars().any(char::is_whitespace)
    {
        return Err("请填写有效的主域名和单个子域名，不要包含协议或路径".into());
    }
    if !config.provider.trim().eq_ignore_ascii_case("aliyun") {
        return Err("目前仅支持阿里云 DNS".into());
    }
    if !(60..=86400).contains(&config.ttl)
        || !(1..=1440).contains(&config.interval_minutes)
        || config.ip_candidate_index == 0
    {
        return Err("请检查 TTL、同步间隔和候选顺位的取值范围".into());
    }
    Ok(())
}

fn valid_binding_mac(value: &str) -> bool {
    crate::device_discovery::normalize_mac(value).is_some_and(|mac| {
        mac != "00:00:00:00:00:00"
            && u8::from_str_radix(&mac[..2], 16).is_ok_and(|first| first & 1 == 0)
    })
}

pub(crate) fn to_device_ddns_aliyun_config(config: &DeviceDdnsConfig) -> DdnsConfig {
    DdnsConfig {
        enabled: config.enabled,
        provider: config.provider.clone(),
        access_key_id: config.access_key_id.clone(),
        access_key_secret: config.access_key_secret.clone(),
        domain: config.domain.clone(),
        sub_domain: config.sub_domain.clone(),
        record_type: config.record_type.trim().to_uppercase(),
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
    ip_candidate_index: u32,
    device_id: String,
    device_mac: String,
    record_type: String,
    selected_ip: String,
    selected_ipv6: String,
}

pub(crate) fn device_ddns_identity(config: &DeviceDdnsConfig) -> DeviceDdnsOperationIdentity {
    DeviceDdnsOperationIdentity {
        enabled: config.enabled,
        provider: config.provider.trim().to_string(),
        access_key_id: config.access_key_id.trim().to_string(),
        access_key_secret: config.access_key_secret.trim().to_string(),
        domain: config.domain.trim().to_string(),
        sub_domain: config.sub_domain.trim().to_string(),
        ttl: config.ttl,
        interval_minutes: config.interval_minutes,
        ip_candidate_index: config.ip_candidate_index.max(1),
        device_id: config.device_id.trim().to_string(),
        device_mac: config.device_mac.trim().to_ascii_lowercase(),
        record_type: config.record_type.trim().to_uppercase(),
        selected_ip: config.selected_ip.trim().to_string(),
        selected_ipv6: config.selected_ipv6.trim().to_string(),
    }
}

pub(crate) fn device_ddns_identity_matches(
    config: &DeviceDdnsConfig,
    identity: &DeviceDdnsOperationIdentity,
) -> bool {
    device_ddns_identity(config) == *identity
}

fn device_ddns_has_device_selector(config: &DeviceDdnsConfig) -> bool {
    !config.device_id.trim().is_empty() || !config.device_mac.trim().is_empty()
}

fn device_ddns_selector_matches(
    config: &DeviceDdnsConfig,
    device_id: &str,
    device_mac: &str,
) -> bool {
    if generated_local_machine_config(config)
        && config.device_id.trim() == "local-machine"
        && device_id.trim() == "local-machine"
    {
        return true;
    }

    device_selectors_match(&config.device_id, &config.device_mac, device_id, device_mac)
}

fn generated_local_machine_config(config: &DeviceDdnsConfig) -> bool {
    config.device_name.trim().starts_with("本机设备")
}

fn device_selectors_match(left_id: &str, left_mac: &str, right_id: &str, right_mac: &str) -> bool {
    if let (Some(left), Some(right)) = (
        crate::device_discovery::normalize_mac(left_mac),
        crate::device_discovery::normalize_mac(right_mac),
    ) {
        return left == right;
    }

    let left_id = left_id.trim();
    let right_id = right_id.trim();
    !left_id.is_empty() && !right_id.is_empty() && left_id == right_id
}

pub(crate) fn active_device_ddns_configs(config: &AppConfig) -> Vec<DeviceDdnsConfig> {
    if !config.device_ddns_configs.is_empty() {
        let mut configs = config.device_ddns_configs.clone();
        crate::config::dedupe_device_ddns_configs(&mut configs);
        return configs;
    }

    if device_ddns_has_device_selector(&config.device_ddns) {
        vec![config.device_ddns.clone()]
    } else {
        Vec::new()
    }
}

pub(crate) fn device_discovery_hints(
    configs: &[DeviceDdnsConfig],
) -> Vec<crate::device_discovery::DeviceDiscoveryHint> {
    configs
        .iter()
        .filter(|config| device_ddns_has_device_selector(config))
        .map(|config| {
            let use_candidate_index = !config.record_type.trim().eq_ignore_ascii_case("A");
            crate::device_discovery::DeviceDiscoveryHint {
                device_id: config.device_id.trim().to_string(),
                device_mac: config.device_mac.trim().to_string(),
                device_name: config.device_name.trim().to_string(),
                selected_ip: if use_candidate_index {
                    String::new()
                } else {
                    config.selected_ip.trim().to_string()
                },
                selected_ipv6: if use_candidate_index {
                    String::new()
                } else {
                    config.selected_ipv6.trim().to_string()
                },
            }
        })
        .collect()
}

fn normalize_device_ddns_payload(mut config: DeviceDdnsConfig) -> DeviceDdnsConfig {
    config.provider = config.provider.trim().to_ascii_lowercase();
    config.access_key_id = config.access_key_id.trim().to_string();
    config.access_key_secret = config.access_key_secret.trim().to_string();
    if let Some(mac) = crate::device_discovery::normalize_mac(&config.device_mac) {
        config.device_mac = mac;
    }
    if config.device_id.trim().is_empty() && !config.device_mac.is_empty() {
        config.device_id = format!("mac-{}", config.device_mac.replace(':', "-"));
    }
    config.domain = config
        .domain
        .trim()
        .trim_end_matches('.')
        .to_ascii_lowercase();
    config.sub_domain = config
        .sub_domain
        .trim()
        .trim_end_matches('.')
        .to_ascii_lowercase();
    if config.sub_domain == "@" {
        config.sub_domain.clear();
    }
    config.record_type = config.record_type.trim().to_uppercase();
    if config.record_type != "A" && config.record_type != "AAAA" {
        config.record_type = "AAAA".to_string();
    }
    if config.ip_candidate_index == 0 {
        config.ip_candidate_index = 1;
    }
    if config.selected_ip.trim().is_empty() && !config.selected_ipv6.trim().is_empty() {
        config.selected_ip = config.selected_ipv6.trim().to_string();
    }
    if config.record_type == "AAAA" {
        config.selected_ipv6 = config.selected_ip.clone();
    }
    config
}

fn upsert_device_ddns_config(config: &mut AppConfig, device_config: DeviceDdnsConfig) {
    let device_config = normalize_device_ddns_payload(device_config);
    config.device_ddns = device_config.clone();

    if !device_ddns_has_device_selector(&device_config) {
        return;
    }

    config
        .device_ddns_configs
        .retain(|item| !crate::config::device_ddns_bindings_conflict(item, &device_config));
    config.device_ddns_configs.push(device_config);
}

fn delete_device_ddns_config_from_config(
    config: &mut AppConfig,
    device_id: &str,
    device_mac: &str,
    record_key: Option<&(String, String, String)>,
) -> bool {
    let selected = |item: &DeviceDdnsConfig| {
        device_ddns_selector_matches(item, device_id, device_mac)
            && record_key
                .is_none_or(|key| crate::config::device_ddns_record_key(item).as_ref() == Some(key))
    };
    let removed: Vec<_> = active_device_ddns_configs(config)
        .into_iter()
        .filter(&selected)
        .collect();
    let matches_removed = |item: &DeviceDdnsConfig| {
        selected(item)
            || removed
                .iter()
                .any(|removed| crate::config::device_ddns_bindings_conflict(item, removed))
    };
    let original_len = config.device_ddns_configs.len();
    config
        .device_ddns_configs
        .retain(|item| !matches_removed(item));

    let mut changed = config.device_ddns_configs.len() != original_len;
    if matches_removed(&config.device_ddns) {
        config.device_ddns = DeviceDdnsConfig::default();
        changed = true;
    }
    changed
}

fn create_device_binding(
    config: &mut AppConfig,
    binding: DeviceDdnsConfig,
) -> Result<DeviceDdnsConfig, String> {
    validate_device_ddns_config(&binding)?;
    let mut binding = normalize_device_ddns_payload(binding);
    let key = crate::config::device_ddns_record_key(&binding);
    let existing = active_device_ddns_configs(config);
    if existing
        .iter()
        .any(|item| crate::config::device_ddns_record_key(item) == key)
    {
        return Err("此域名和记录类型已存在，请编辑原绑定；新增不会覆盖已有配置".into());
    }
    binding.last_update_time.clear();
    binding.last_result.clear();
    binding.last_online = false;
    config.device_ddns_configs = existing;
    upsert_device_ddns_config(config, binding.clone());
    Ok(binding)
}

fn save_device_binding(
    config: &mut AppConfig,
    binding: DeviceDdnsConfig,
    original_record_key: Option<&(String, String, String)>,
) -> Result<(), String> {
    validate_device_ddns_config(&binding)?;
    let binding = normalize_device_ddns_payload(binding);
    let mut existing = active_device_ddns_configs(config);
    if let Some(original) = original_record_key {
        let index = existing
            .iter()
            .position(|item| crate::config::device_ddns_record_key(item).as_ref() == Some(original))
            .ok_or_else(|| "原绑定已变更或被删除，请刷新后重试".to_string())?;
        let next_key = crate::config::device_ddns_record_key(&binding);
        if !device_ddns_selector_matches(&existing[index], &binding.device_id, &binding.device_mac)
        {
            return Err("绑定的目标设备已变化，请刷新后重试".into());
        }
        if existing
            .iter()
            .enumerate()
            .any(|(i, item)| i != index && crate::config::device_ddns_record_key(item) == next_key)
        {
            return Err("此域名和记录类型已被另一条绑定使用".into());
        }
        existing.remove(index);
    }
    config.device_ddns_configs = existing;
    upsert_device_ddns_config(config, binding);
    Ok(())
}

pub(crate) fn apply_device_ddns_status_if_current(
    state: &AppState,
    identity: &DeviceDdnsOperationIdentity,
    selected_ip: Option<String>,
    last_result: String,
    last_online: Option<bool>,
    touch_update_time: bool,
) -> Result<bool, String> {
    let mut guard = state
        .config
        .lock()
        .map_err(|e| format!("写入设备 DDNS 结果失败：{}", e))?;

    let mut cfg = guard.clone();
    let mut updated = false;

    for device_config in &mut cfg.device_ddns_configs {
        if device_ddns_identity_matches(device_config, identity) {
            apply_device_ddns_status(
                device_config,
                selected_ip.clone(),
                &last_result,
                last_online,
                touch_update_time,
            );
            updated = true;
            break;
        }
    }

    if device_ddns_identity_matches(&cfg.device_ddns, identity) {
        apply_device_ddns_status(
            &mut cfg.device_ddns,
            selected_ip,
            &last_result,
            last_online,
            touch_update_time,
        );
        updated = true;
    }

    if !updated {
        return Ok(false);
    }

    save_config(&cfg)?;
    *guard = cfg;
    Ok(true)
}

fn apply_device_ddns_status(
    config: &mut DeviceDdnsConfig,
    selected_ip: Option<String>,
    last_result: &str,
    last_online: Option<bool>,
    touch_update_time: bool,
) {
    if let Some(selected_ip) = selected_ip {
        config.selected_ip = selected_ip.clone();
        if config.record_type.trim().eq_ignore_ascii_case("AAAA") {
            config.selected_ipv6 = selected_ip;
        }
    }
    if let Some(last_online) = last_online {
        config.last_online = last_online;
    }
    if touch_update_time {
        config.last_update_time = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    }
    config.last_result = last_result.to_string();
}

fn device_matches_config(device: &LanDevice, config: &DeviceDdnsConfig) -> bool {
    device_selectors_match(
        &device.id,
        &device.mac,
        &config.device_id,
        &config.device_mac,
    )
}

fn matching_device_for_config<'a>(
    config: &DeviceDdnsConfig,
    devices: &'a [LanDevice],
) -> Option<&'a LanDevice> {
    let config_mac = config.device_mac.trim();
    if !config_mac.is_empty()
        && crate::device_discovery::normalize_mac(config_mac).is_none()
        && !generated_local_machine_config(config)
    {
        return None;
    }
    if !config_mac.is_empty() {
        if let Some(device) = devices.iter().find(|device| {
            !device.mac.trim().is_empty() && device.mac.trim().eq_ignore_ascii_case(config_mac)
        }) {
            return Some(device);
        }
    }

    if generated_local_machine_config(config) && config.device_id.trim() == "local-machine" {
        return devices
            .iter()
            .find(|device| device.id.trim() == "local-machine");
    }

    devices
        .iter()
        .find(|device| device_matches_config(device, config))
}

pub(crate) fn device_ddns_device_is_online(
    config: &DeviceDdnsConfig,
    devices: &[LanDevice],
) -> bool {
    matching_device_for_config(config, devices).is_some_and(|device| device.online)
}

fn global_ipv6_by_candidate_index<'a>(
    config: &DeviceDdnsConfig,
    device: &'a LanDevice,
) -> Option<&'a str> {
    let candidate_index = config.ip_candidate_index.max(1) as usize;
    device
        .global_ipv6
        .iter()
        .map(|value| value.trim())
        .filter(|value| crate::device_discovery::is_global_ipv6(value))
        .nth(candidate_index - 1)
}

fn first_ipv4(device: &LanDevice) -> Option<&str> {
    device
        .ipv4
        .iter()
        .map(|value| value.trim())
        .find(|value| !value.is_empty())
}

fn configured_device_record_value(config: &DeviceDdnsConfig) -> &str {
    if config.record_type.trim().eq_ignore_ascii_case("A") {
        config.selected_ip.trim()
    } else {
        let selected_ip = config.selected_ip.trim();
        if selected_ip.is_empty() {
            config.selected_ipv6.trim()
        } else {
            selected_ip
        }
    }
}

pub(crate) fn resolve_device_ipv6(
    config: &DeviceDdnsConfig,
    devices: &[LanDevice],
) -> Result<String, String> {
    let device = matching_device_for_config(config, devices)
        .ok_or_else(|| "未找到匹配的局域网设备".to_string())?;

    global_ipv6_by_candidate_index(config, device)
        .map(|value| value.to_string())
        .ok_or_else(|| {
            format!(
                "设备 {} 没有第 {} 个可用的公网 IPv6 候选",
                device.display_name,
                config.ip_candidate_index.max(1)
            )
        })
}

pub(crate) fn resolve_device_ipv4(
    config: &DeviceDdnsConfig,
    devices: &[LanDevice],
) -> Result<String, String> {
    let device = matching_device_for_config(config, devices)
        .ok_or_else(|| "未找到匹配的局域网设备".to_string())?;

    let selected_ip = config.selected_ip.trim();
    if !selected_ip.is_empty() {
        let selected_is_available = device.ipv4.iter().any(|value| value.trim() == selected_ip);
        if selected_is_available {
            return Ok(selected_ip.to_string());
        }
    }

    first_ipv4(device)
        .map(|value| value.to_string())
        .ok_or_else(|| format!("设备 {} 没有可用的 IPv4", device.display_name))
}

pub(crate) fn resolve_device_record_value(
    config: &DeviceDdnsConfig,
    devices: &[LanDevice],
) -> Result<String, String> {
    if config.record_type.trim().eq_ignore_ascii_case("A") {
        resolve_device_ipv4(config, devices)
    } else {
        resolve_device_ipv6(config, devices)
    }
}

pub(crate) fn device_ddns_record_value_has_changed(
    config: &DeviceDdnsConfig,
    devices: &[LanDevice],
) -> bool {
    let Ok(current_value) = resolve_device_record_value(config, devices) else {
        return false;
    };

    let configured_value = configured_device_record_value(config);
    configured_value.is_empty() || current_value.trim() != configured_value
}

pub(crate) async fn update_device_ddns_record(
    config: &DeviceDdnsConfig,
    devices: &[LanDevice],
) -> Result<(String, String), String> {
    validate_device_ddns_config(config)?;
    let ip = resolve_device_record_value(config, devices)?;
    let client = crate::ddns::aliyun::AliyunDdns::new(to_device_ddns_aliyun_config(config));
    let result = if config.record_type.trim().eq_ignore_ascii_case("A") {
        client.update_record(&ip, "").await?
    } else {
        client.update_record("", &ip).await?
    };
    Ok((ip, result))
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

fn runtime_ddns_status(config: &AppConfig) -> &'static str {
    if config.ddns.enabled
        || active_device_ddns_configs(config)
            .iter()
            .any(|item| item.enabled)
    {
        "运行中"
    } else {
        "已停止"
    }
}

fn latest_enabled_device_ddns_update_time(config: &AppConfig) -> Option<String> {
    active_device_ddns_configs(config)
        .into_iter()
        .filter(|item| item.enabled)
        .filter_map(|item| {
            let last_update_time = item.last_update_time.trim();
            if last_update_time.is_empty() {
                None
            } else {
                Some(last_update_time.to_string())
            }
        })
        .max()
}

fn latest_runtime_ddns_update_time(config: &AppConfig) -> String {
    let global_update_time = if config.ddns.enabled {
        let update_time = latest_ddns_update_time();
        if update_time == "暂无" {
            None
        } else {
            Some(update_time)
        }
    } else {
        None
    };

    global_update_time
        .into_iter()
        .chain(latest_enabled_device_ddns_update_time(config))
        .max()
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

fn reverse_proxy_label(rule: &ReverseProxyRule) -> String {
    if rule.remark.trim().is_empty() {
        rule.domain.trim().to_string()
    } else {
        rule.remark.trim().to_string()
    }
}

fn describe_reverse_proxy_rule(rule: &ReverseProxyRule) -> String {
    format!(
        "[{}] {} {}:{} -> {}:{}",
        reverse_proxy_label(rule),
        rule.protocol.to_uppercase(),
        if rule.listen_addr.trim().is_empty() {
            "::"
        } else {
            rule.listen_addr.trim()
        },
        rule.listen_port,
        rule.backend_ip.trim(),
        rule.backend_port
    )
}

fn validate_reverse_proxy_rule(rule: &ReverseProxyRule) -> Result<(), String> {
    if rule.domain.trim().is_empty() {
        return Err("外部域名未填写".to_string());
    }
    if rule.listen_port == 0 {
        return Err("监听端口范围：1-65535".to_string());
    }
    if rule.backend_ip.trim().is_empty() {
        return Err("后端地址未填写".to_string());
    }
    if rule.backend_port == 0 {
        return Err("后端端口范围：1-65535".to_string());
    }
    let tls = rule.tls.trim().to_ascii_lowercase();
    if (tls == "auto" || tls == "manual") && !rule.protocol.eq_ignore_ascii_case("HTTPS") {
        return Err("自动证书和手动证书仅支持 HTTPS 反向代理".to_string());
    }
    if tls == "auto" {
        if rule.acme_email.trim().is_empty()
            || rule.acme_access_key_id.trim().is_empty()
            || rule.acme_access_key_secret.trim().is_empty()
            || rule.acme_dns_domain.trim().is_empty()
        {
            return Err("ACME 自动证书需要邮箱、阿里云 AccessKey 和 DNS 主域名".to_string());
        }
        if !rule.acme_dns_provider.trim().eq_ignore_ascii_case("aliyun") {
            return Err("ACME 自动证书当前仅支持阿里云 DNS-01".to_string());
        }
    }
    if tls == "manual"
        && (rule.certificate_path.trim().is_empty() || rule.private_key_path.trim().is_empty())
    {
        return Err("手动证书需要填写证书文件和私钥文件路径".to_string());
    }
    Ok(())
}

fn apply_issued_certificate_to_rule(
    rule: &mut ReverseProxyRule,
    issued: &crate::certificates::IssuedCertificate,
) {
    rule.certificate_path = issued.cert_path.to_string_lossy().to_string();
    rule.private_key_path = issued.key_path.to_string_lossy().to_string();
    rule.certificate_last_issued_at = issued.issued_at.clone();
    rule.certificate_expires_at = issued.expires_at.clone();
    rule.certificate_last_error.clear();
    rule.certificate = format!("自动证书，有效期至 {}", issued.expires_at);
}

async fn issue_reverse_proxy_certificate_by_id(
    state: &AppState,
    rule_id: &str,
) -> Result<ReverseProxyRule, String> {
    let rule = read_config(state)?
        .reverse_proxy_rules
        .into_iter()
        .find(|rule| rule.id == rule_id)
        .ok_or_else(|| format!("未找到反向代理规则：{}", rule_id))?;
    validate_reverse_proxy_rule(&rule)?;

    let issued = match crate::certificates::issue_certificate(&rule).await {
        Ok(issued) => issued,
        Err(error) => {
            let mut cfg = read_config(state)?;
            if let Some(existing) = cfg
                .reverse_proxy_rules
                .iter_mut()
                .find(|existing| existing.id == rule_id)
            {
                existing.certificate_last_error = error.clone();
                existing.status = "证书错误".to_string();
                let _ = write_config(state, &cfg);
            }
            add_log(
                "error",
                "证书",
                &format!("反代自动证书申请失败：{}，{}", rule.domain, error),
            );
            return Err(error);
        }
    };

    let mut cfg = read_config(state)?;
    let saved_rule = if let Some(existing) = cfg
        .reverse_proxy_rules
        .iter_mut()
        .find(|existing| existing.id == rule_id)
    {
        apply_issued_certificate_to_rule(existing, &issued);
        existing.status = if existing.enabled {
            "正常".to_string()
        } else {
            "已禁用".to_string()
        };
        existing.clone()
    } else {
        return Err(format!("未找到反向代理规则：{}", rule_id));
    };

    write_config(state, &cfg)?;
    add_log(
        "info",
        "证书",
        &format!(
            "反代自动证书已签发：{} -> {}",
            saved_rule.domain, saved_rule.certificate_path
        ),
    );
    reapply_reverse_proxy_rules(state).await;
    Ok(saved_rule)
}

pub(crate) async fn ensure_reverse_proxy_certificates(state: &AppState) {
    let candidates = match read_config(state) {
        Ok(config) => config
            .reverse_proxy_rules
            .into_iter()
            .filter(|rule| rule.enabled && crate::certificates::certificate_renewal_due(rule))
            .collect::<Vec<_>>(),
        Err(error) => {
            add_log(
                "error",
                "证书",
                &format!("读取反代证书配置失败：{}", error),
            );
            return;
        }
    };

    for rule in candidates {
        if rule.id.trim().is_empty() {
            continue;
        }
        let _ = issue_reverse_proxy_certificate_by_id(state, &rule.id).await;
    }
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
        if let Some(rule) = config
            .forward_rules
            .iter_mut()
            .find(|r| r.id == result.rule_id)
        {
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

async fn reapply_reverse_proxy_rules(state: &AppState) {
    let rules = {
        let config = state.config.lock().unwrap();
        config.reverse_proxy_rules.clone()
    };
    let mut manager = state.reverse_proxy_manager.lock().await;
    let results = manager.apply_rules(&rules).await;
    drop(manager);

    if results.is_empty() {
        return;
    }

    let mut config = state.config.lock().unwrap();
    let mut changed = false;
    for result in &results {
        if let Some(rule) = config
            .reverse_proxy_rules
            .iter_mut()
            .find(|rule| rule.id == result.rule_id)
        {
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

async fn reapply_webdav(state: &AppState) -> WebDavRuntimeStatus {
    let config = {
        let config = state.config.lock().unwrap();
        config.webdav.clone()
    };
    let mut manager = state.webdav_manager.lock().await;
    manager.apply(&config).await
}

async fn webdav_config_from_request(
    existing: &WebDavConfig,
    request: SaveWebDavConfigRequest,
) -> Result<WebDavConfig, String> {
    let mut accounts = Vec::with_capacity(request.accounts.len());
    for input in request.accounts {
        let existing_account = existing
            .accounts
            .iter()
            .find(|account| !input.id.trim().is_empty() && account.id == input.id);
        let id = existing_account
            .map(|account| account.id.clone())
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        if accounts
            .iter()
            .any(|account: &WebDavAccount| account.id == id)
        {
            return Err("WebDAV 账号标识重复，请重新加载配置".to_string());
        }
        let password_hash = if input.password.is_empty() {
            existing_account
                .map(|account| account.password_hash.clone())
                .unwrap_or_default()
        } else {
            let password = input.password;
            tokio::task::spawn_blocking(move || hash_webdav_password(&password))
                .await
                .map_err(|error| format!("生成 WebDAV 密码哈希失败：{}", error))??
        };
        accounts.push(WebDavAccount {
            id,
            username: input.username.trim().to_string(),
            password_hash,
            read_only: input.read_only,
            enabled: input.enabled,
        });
    }

    let config = WebDavConfig {
        enabled: request.enabled,
        listen_addr: request.listen_addr.trim().to_string(),
        listen_port: request.listen_port,
        root_path: request.root_path.trim().to_string(),
        global_read_only: request.global_read_only,
        accounts,
    };
    validate_webdav_config(&config)?;
    Ok(config)
}

fn webdav_config_view(
    config: &WebDavConfig,
    status: WebDavRuntimeStatus,
    last_used: &std::collections::HashMap<String, String>,
) -> WebDavConfigView {
    WebDavConfigView {
        enabled: config.enabled,
        listen_addr: config.listen_addr.clone(),
        listen_port: config.listen_port,
        root_path: config.root_path.clone(),
        global_read_only: config.global_read_only,
        accounts: config
            .accounts
            .iter()
            .map(|account| WebDavAccountView {
                id: account.id.clone(),
                username: account.username.clone(),
                read_only: account.read_only,
                enabled: account.enabled,
                password_configured: !account.password_hash.trim().is_empty(),
                last_used: last_used.get(&account.id).cloned().unwrap_or_default(),
            })
            .collect(),
        status,
    }
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_runtime_status(state: State<'_, AppState>) -> Result<RuntimeStatus, String> {
    let cfg = read_config(&state)?;

    let enabled_count = cfg.forward_rules.iter().filter(|r| r.enabled).count();
    let enabled_reverse_proxy_count = cfg.reverse_proxy_rules.iter().filter(|r| r.enabled).count();

    // Uptime: how long the process has been running (seconds).
    // Use sysinfo to get process start time; fallback to 0.
    let uptime = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().saturating_sub(start_time_secs()))
        .unwrap_or(0);

    let ipv4 = crate::ddns::get_public_ipv4().await;
    let ipv6 = crate::ddns::get_local_ipv6_for_interface(&cfg.ipv6_interface);

    Ok(RuntimeStatus {
        version: env!("CARGO_PKG_VERSION").to_string(),
        public_ipv4: ipv4.clone(),
        public_ipv6: ipv6.clone(),
        ddns_status: runtime_ddns_status(&cfg).to_string(),
        last_update_time: latest_runtime_ddns_update_time(&cfg),
        rule_count: cfg.forward_rules.len() as u32,
        enabled_rule_count: enabled_count as u32,
        reverse_proxy_rule_count: cfg.reverse_proxy_rules.len() as u32,
        enabled_reverse_proxy_rule_count: enabled_reverse_proxy_count as u32,
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
    add_log(
        "info",
        "DDNS",
        &format!("DDNS 手动更新完成：{}，{}", domain, result),
    );
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
pub async fn list_lan_devices(state: State<'_, AppState>) -> Result<Vec<LanDevice>, String> {
    let cfg = read_config(&state)?;
    let configs = active_device_ddns_configs(&cfg);
    let hints = device_discovery_hints(&configs);
    tauri::async_runtime::spawn_blocking(move || {
        crate::device_discovery::scan_lan_devices_with_hints(&hints)
    })
    .await
    .map_err(|error| format!("扫描局域网设备失败：{error}"))?
}

#[tauri::command]
pub async fn get_device_ddns_config(
    state: State<'_, AppState>,
) -> Result<DeviceDdnsConfig, String> {
    let cfg = read_config(&state)?;
    Ok(active_device_ddns_configs(&cfg)
        .into_iter()
        .next()
        .unwrap_or(cfg.device_ddns))
}

#[tauri::command]
pub async fn list_device_ddns_configs(
    state: State<'_, AppState>,
) -> Result<Vec<DeviceDdnsConfig>, String> {
    let cfg = read_config(&state)?;
    Ok(active_device_ddns_configs(&cfg))
}

#[tauri::command]
pub async fn create_device_ddns_config(
    state: State<'_, AppState>,
    config: DeviceDdnsConfig,
) -> Result<DeviceDdnsConfig, String> {
    // Check uniqueness and persist under one lock so simultaneous creates cannot overwrite each other.
    let mut guard = state.config.lock().map_err(|error| error.to_string())?;
    let mut next = guard.clone();
    let binding = create_device_binding(&mut next, config)?;
    save_config(&next)?;
    *guard = next;
    add_log(
        "info",
        "设备DDNS",
        &format!("已新增设备 DDNS 绑定：{}", device_ddns_domain(&binding)),
    );
    Ok(binding)
}

#[tauri::command]
pub async fn save_device_ddns_config(
    state: State<'_, AppState>,
    config: DeviceDdnsConfig,
    original_record_key: Option<(String, String, String)>,
) -> Result<(), String> {
    let mut guard = state.config.lock().map_err(|error| error.to_string())?;
    let mut cfg = guard.clone();
    let domain = device_ddns_domain(&config);
    let device_name = config.device_name.trim().to_string();
    save_device_binding(&mut cfg, config, original_record_key.as_ref())?;
    save_config(&cfg)?;
    *guard = cfg;
    add_log(
        "info",
        "设备DDNS",
        &format!(
            "设备 DDNS 配置已保存：{}{}",
            if device_name.is_empty() {
                "未命名设备".to_string()
            } else {
                device_name
            },
            if domain == "未配置域名" {
                String::new()
            } else {
                format!(" -> {}", domain)
            }
        ),
    );
    Ok(())
}

#[tauri::command]
pub async fn delete_device_ddns_config(
    state: State<'_, AppState>,
    device_id: String,
    device_mac: String,
    record_key: Option<(String, String, String)>,
) -> Result<(), String> {
    let mut guard = state.config.lock().map_err(|error| error.to_string())?;
    let mut cfg = guard.clone();
    let changed = delete_device_ddns_config_from_config(
        &mut cfg,
        &device_id,
        &device_mac,
        record_key.as_ref(),
    );
    if changed {
        save_config(&cfg)?;
        *guard = cfg;
        add_log("info", "设备DDNS", "设备 DDNS 绑定已解除");
    } else if record_key.is_some() {
        return Err("原绑定已变更或被删除，请刷新后重试".into());
    }
    Ok(())
}

#[tauri::command]
pub async fn get_device_ddns_current_record(state: State<'_, AppState>) -> Result<String, String> {
    let cfg = read_config(&state)?;
    let device_config = active_device_ddns_configs(&cfg)
        .into_iter()
        .next()
        .unwrap_or(cfg.device_ddns);
    validate_device_ddns_config(&device_config)?;
    let client = crate::ddns::aliyun::AliyunDdns::new(to_device_ddns_aliyun_config(&device_config));
    client.describe_record().await
}

#[tauri::command]
pub async fn trigger_device_ddns_update(
    state: State<'_, AppState>,
    config: Option<DeviceDdnsConfig>,
) -> Result<String, String> {
    let (device_config, hint_configs) = {
        let cfg = read_config(&state)?;
        let device_config = match config {
            Some(config) => config,
            None => active_device_ddns_configs(&cfg)
                .into_iter()
                .find(|config| config.enabled)
                .unwrap_or_else(|| cfg.device_ddns.clone()),
        };
        let mut hint_configs = active_device_ddns_configs(&cfg);
        if let Some(existing) = hint_configs
            .iter()
            .position(|config| crate::config::device_ddns_bindings_conflict(config, &device_config))
        {
            hint_configs[existing] = device_config.clone();
        } else if device_ddns_has_device_selector(&device_config) {
            hint_configs.push(device_config.clone());
        }
        (device_config, hint_configs)
    };
    let identity = device_ddns_identity(&device_config);

    if !device_config.enabled {
        return Err("设备 DDNS 未启用".to_string());
    }
    validate_device_ddns_config(&device_config)?;

    let hints = device_discovery_hints(&hint_configs);
    let devices = crate::device_discovery::discover_lan_devices_with_hints(&hints);
    let currently_online = device_ddns_device_is_online(&device_config, &devices);
    let domain = device_ddns_domain(&device_config);
    let update_result = update_device_ddns_record(&device_config, &devices).await;

    match update_result {
        Ok((ip, result)) => {
            match apply_device_ddns_status_if_current(
                &state,
                &identity,
                Some(ip.clone()),
                result.clone(),
                Some(true),
                true,
            ) {
                Ok(true) => {}
                Ok(false) => add_log(
                    "warn",
                    "设备DDNS",
                    &format!("设备 DDNS 配置已变化，跳过本次手动更新结果写入：{}", domain),
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
                &format!("设备 DDNS 手动更新完成：{} -> {}，{}", domain, ip, result),
            );
            Ok(result)
        }
        Err(error) => {
            match apply_device_ddns_status_if_current(
                &state,
                &identity,
                None,
                error.clone(),
                Some(currently_online),
                true,
            ) {
                Ok(true) => {}
                Ok(false) => add_log(
                    "warn",
                    "设备DDNS",
                    &format!("设备 DDNS 配置已变化，跳过本次手动失败结果写入：{}", domain),
                ),
                Err(save_error) => add_log(
                    "error",
                    "设备DDNS",
                    &format!("设备 DDNS 手动失败结果写入失败：{}，{}", domain, save_error),
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
        rule.status = if rule.enabled {
            "正常".into()
        } else {
            "已禁用".into()
        };
    }

    if is_new {
        rule.id = Uuid::new_v4().to_string();
        rule.status = if rule.enabled {
            "正常".into()
        } else {
            "已禁用".into()
        };
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
        rule.status = if enabled {
            "正常".into()
        } else {
            "已禁用".into()
        };
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
pub async fn list_reverse_proxy_rules(
    state: State<'_, AppState>,
) -> Result<Vec<ReverseProxyRule>, String> {
    let cfg = read_config(&state)?;
    Ok(cfg.reverse_proxy_rules)
}

#[tauri::command]
pub async fn save_reverse_proxy_rule(
    state: State<'_, AppState>,
    mut rule: ReverseProxyRule,
) -> Result<ReverseProxyRule, String> {
    normalize_reverse_proxy_rule(&mut rule);
    validate_reverse_proxy_rule(&rule)?;

    let mut cfg = read_config(&state)?;
    let is_new = rule.id.trim().is_empty();
    if is_new {
        rule.id = Uuid::new_v4().to_string();
        rule.status = if rule.enabled {
            "正常".into()
        } else {
            "已禁用".into()
        };
        cfg.reverse_proxy_rules.push(rule.clone());
    } else if let Some(existing) = cfg
        .reverse_proxy_rules
        .iter_mut()
        .find(|existing| existing.id == rule.id)
    {
        *existing = rule.clone();
    } else {
        cfg.reverse_proxy_rules.push(rule.clone());
    }

    let saved_rule = rule;
    write_config(&state, &cfg)?;
    add_log(
        "info",
        "反代",
        &format!(
            "反向代理规则已{}：{}",
            if is_new { "新增" } else { "保存" },
            describe_reverse_proxy_rule(&saved_rule)
        ),
    );
    reapply_reverse_proxy_rules(&state).await;
    Ok(saved_rule)
}

#[tauri::command]
pub async fn delete_reverse_proxy_rule(
    state: State<'_, AppState>,
    rule_id: String,
) -> Result<(), String> {
    let mut cfg = read_config(&state)?;
    let removed = cfg
        .reverse_proxy_rules
        .iter()
        .find(|rule| rule.id == rule_id)
        .cloned();
    let Some(rule) = removed else {
        return Err(format!("未找到反向代理规则：{}", rule_id));
    };
    cfg.reverse_proxy_rules.retain(|rule| rule.id != rule_id);
    write_config(&state, &cfg)?;
    add_log(
        "info",
        "反代",
        &format!("反向代理规则已删除：{}", describe_reverse_proxy_rule(&rule)),
    );
    reapply_reverse_proxy_rules(&state).await;
    Ok(())
}

#[tauri::command]
pub async fn enable_reverse_proxy_rule(
    state: State<'_, AppState>,
    rule_id: String,
    enabled: bool,
) -> Result<(), String> {
    let mut cfg = read_config(&state)?;
    let updated = if let Some(rule) = cfg
        .reverse_proxy_rules
        .iter_mut()
        .find(|rule| rule.id == rule_id)
    {
        rule.enabled = enabled;
        rule.status = if enabled {
            "正常".into()
        } else {
            "已禁用".into()
        };
        Some(rule.clone())
    } else {
        None
    };

    let Some(rule) = updated else {
        return Err(format!("未找到反向代理规则：{}", rule_id));
    };

    write_config(&state, &cfg)?;
    add_log(
        "info",
        "反代",
        &format!(
            "反向代理规则已{}：{}",
            if enabled { "启用" } else { "禁用" },
            describe_reverse_proxy_rule(&rule)
        ),
    );
    reapply_reverse_proxy_rules(&state).await;
    Ok(())
}

#[tauri::command]
pub async fn issue_reverse_proxy_certificate(
    state: State<'_, AppState>,
    rule_id: String,
) -> Result<ReverseProxyRule, String> {
    issue_reverse_proxy_certificate_by_id(&state, &rule_id).await
}

#[tauri::command]
pub async fn get_webdav_config(state: State<'_, AppState>) -> Result<WebDavConfigView, String> {
    let config = read_config(&state)?.webdav;
    let manager = state.webdav_manager.lock().await;
    Ok(webdav_config_view(
        &config,
        manager.status(),
        &manager.account_last_used(),
    ))
}

#[tauri::command]
pub async fn save_webdav_config(
    state: State<'_, AppState>,
    request: SaveWebDavConfigRequest,
) -> Result<WebDavConfigView, String> {
    let mut app_config = read_config(&state)?;
    let webdav_config = webdav_config_from_request(&app_config.webdav, request).await?;
    app_config.webdav = webdav_config.clone();
    write_config(&state, &app_config)?;
    add_log(
        "info",
        "WebDAV",
        &format!(
            "WebDAV 配置已保存：{}:{}，{} 个账号",
            webdav_config.listen_addr,
            webdav_config.listen_port,
            webdav_config.accounts.len()
        ),
    );
    let status = reapply_webdav(&state).await;
    let manager = state.webdav_manager.lock().await;
    Ok(webdav_config_view(
        &webdav_config,
        status,
        &manager.account_last_used(),
    ))
}

#[tauri::command]
pub async fn set_webdav_enabled(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<WebDavConfigView, String> {
    let mut app_config = read_config(&state)?;
    app_config.webdav.enabled = enabled;
    validate_webdav_config(&app_config.webdav)?;
    let webdav_config = app_config.webdav.clone();
    write_config(&state, &app_config)?;
    add_log(
        "info",
        "WebDAV",
        if enabled {
            "WebDAV 服务已启用"
        } else {
            "WebDAV 服务已停用"
        },
    );
    let status = reapply_webdav(&state).await;
    let manager = state.webdav_manager.lock().await;
    Ok(webdav_config_view(
        &webdav_config,
        status,
        &manager.account_last_used(),
    ))
}

#[tauri::command]
pub async fn test_webdav_service(state: State<'_, AppState>) -> Result<String, String> {
    let config = read_config(&state)?.webdav;
    let status = state.webdav_manager.lock().await.status();
    if status.state != "running" {
        return Err(status.message);
    }
    let result = probe_webdav_listener(&config).await?;
    add_log("info", "WebDAV", &result);
    Ok(result)
}

#[tauri::command]
pub async fn select_webdav_directory(app: AppHandle) -> Result<Option<String>, String> {
    let selected = tokio::task::spawn_blocking(move || app.dialog().file().blocking_pick_folder())
        .await
        .map_err(|error| format!("打开目录选择器失败：{}", error))?;
    selected
        .map(|path| {
            path.into_path()
                .map(|path| path.to_string_lossy().to_string())
                .map_err(|error| format!("读取所选目录失败：{}", error))
        })
        .transpose()
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
pub async fn set_auto_start(state: State<'_, AppState>, enabled: bool) -> Result<(), String> {
    crate::autostart::set_autostart(enabled)?;

    let mut cfg = read_config(&state)?;
    cfg.auto_start = enabled;
    write_config(&state, &cfg)?;

    add_log(
        "info",
        "自启动",
        if enabled {
            "系统启动后自动运行已启用"
        } else {
            "系统启动后自动运行已关闭"
        },
    );
    Ok(())
}

#[tauri::command]
pub async fn get_auto_update_check(state: State<'_, AppState>) -> Result<bool, String> {
    Ok(read_config(&state)?.auto_check_updates)
}

#[tauri::command]
pub async fn set_auto_update_check(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    let mut cfg = read_config(&state)?;
    cfg.auto_check_updates = enabled;
    write_config(&state, &cfg)?;
    add_log(
        "info",
        "更新",
        if enabled {
            "自动检查更新已启用"
        } else {
            "自动检查更新已关闭"
        },
    );
    Ok(())
}

#[tauri::command]
pub async fn check_app_update(app: AppHandle) -> Result<AppUpdateResult, String> {
    let current_version = env!("CARGO_PKG_VERSION").to_string();
    if embedded_updater_public_key().is_some() {
        return match available_app_update(&app).await? {
            Some(update) => {
                let latest_version = update.version.clone();
                let notes = update.body.unwrap_or_default();
                add_log(
                    "info",
                    "更新",
                    &format!("发现已签名更新 {}，等待用户确认安装", latest_version),
                );
                Ok(AppUpdateResult {
                    status: "available".to_string(),
                    current_version,
                    latest_version: latest_version.clone(),
                    message: format!("发现新版本 {}", latest_version),
                    notes,
                    verification: "signed".to_string(),
                })
            }
            None => Ok(AppUpdateResult {
                status: "up_to_date".to_string(),
                current_version: current_version.clone(),
                latest_version: current_version,
                message: "当前已经是最新版本".to_string(),
                notes: String::new(),
                verification: "signed".to_string(),
            }),
        };
    }

    let asset_kind = current_update_asset_kind()?;
    let client = reqwest::Client::new();
    let release = fetch_latest_github_release(&client).await?;
    let latest_version = release.tag_name.trim().to_string();
    if parse_release_version(&latest_version).is_none() {
        return Err(format!("GitHub 最新版本号无效：{}", latest_version));
    }
    if !is_release_version_newer(&current_version, &latest_version) {
        return Ok(AppUpdateResult {
            status: "up_to_date".to_string(),
            current_version,
            latest_version,
            message: "当前已经是最新版本".to_string(),
            notes: String::new(),
            verification: "github_https".to_string(),
        });
    }
    let asset = select_github_update_asset(&release, asset_kind).ok_or_else(|| {
        format!(
            "GitHub Release {} 未找到{}（期望文件后缀 {}）",
            latest_version,
            asset_kind.label(),
            asset_kind.asset_suffix()
        )
    })?;
    verify_inferred_update_asset(&client, &release, asset).await?;
    add_log(
        "warn",
        "更新",
        &format!(
            "发现未配置 updater 签名的更新 {}，等待用户确认是否通过 GitHub HTTPS 安装",
            latest_version
        ),
    );
    Ok(AppUpdateResult {
        status: "available".to_string(),
        current_version,
        latest_version: latest_version.clone(),
        message: format!("发现新版本 {}（未配置更新签名）", latest_version),
        notes: release.body,
        verification: "github_https".to_string(),
    })
}

#[tauri::command]
pub async fn install_app_update(
    app: AppHandle,
    expected_version: String,
) -> Result<AppUpdateResult, String> {
    if embedded_updater_public_key().is_none() {
        return install_unsigned_app_update(app, expected_version).await;
    }

    let current_version = env!("CARGO_PKG_VERSION").to_string();
    let update = available_app_update(&app)
        .await?
        .ok_or_else(|| "当前没有可安装的新版本，请重新检查更新".to_string())?;
    let latest_version = update.version.clone();
    if !is_release_version_newer(&current_version, &latest_version) {
        return Err("更新版本不高于当前版本，请重新检查更新".to_string());
    }
    if !update_versions_match(&expected_version, &latest_version) {
        return Err(format!(
            "可用版本已从 {} 变更为 {}，请重新确认后安装",
            expected_version, latest_version
        ));
    }

    add_log(
        "info",
        "更新",
        &format!(
            "用户已确认安装版本 {}，开始下载并验证更新签名",
            latest_version
        ),
    );

    let bytes = update
        .download(|_, _| {}, || {})
        .await
        .map_err(update_error_message)?;

    add_log(
        "info",
        "更新",
        &format!("版本 {} 的更新签名验证通过，开始安装", latest_version),
    );

    update.install(&bytes).map_err(update_error_message)?;

    #[cfg(target_os = "macos")]
    {
        clear_macos_application_quarantine()?;
    }

    add_log(
        "info",
        "更新",
        &format!("版本 {} 已安装，正在重启应用", latest_version),
    );
    app.restart();
}

async fn install_unsigned_app_update(
    app: AppHandle,
    expected_version: String,
) -> Result<AppUpdateResult, String> {
    let current_version = env!("CARGO_PKG_VERSION").to_string();
    let asset_kind = current_update_asset_kind()?;
    let client = reqwest::Client::new();
    let release = fetch_latest_github_release(&client).await?;
    let latest_version = release.tag_name.trim().to_string();
    if !is_release_version_newer(&current_version, &latest_version) {
        return Err("当前没有可安装的新版本，请重新检查更新".to_string());
    }
    if !update_versions_match(&expected_version, &latest_version) {
        return Err(format!(
            "可用版本已从 {} 变更为 {}，请重新确认后安装",
            expected_version, latest_version
        ));
    }
    let asset = select_github_update_asset(&release, asset_kind).ok_or_else(|| {
        format!(
            "GitHub Release {} 未找到{}（期望文件后缀 {}）",
            latest_version,
            asset_kind.label(),
            asset_kind.asset_suffix()
        )
    })?;
    verify_inferred_update_asset(&client, &release, asset).await?;

    add_log(
        "warn",
        "更新",
        &format!(
            "用户已确认安装未配置 updater 签名的版本 {}，从 GitHub HTTPS 下载 {}",
            latest_version, asset.name
        ),
    );
    let package_path = download_github_update_asset(&client, asset).await?;
    start_unsigned_update_installer(asset_kind, &package_path)?;
    add_log(
        "info",
        "更新",
        &format!(
            "版本 {} 已下载，安装完成后将解除 macOS 隔离属性并重启应用",
            latest_version
        ),
    );

    let app_to_exit = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(500)).await;
        app_to_exit.exit(0);
    });

    Ok(AppUpdateResult {
        status: "installed".to_string(),
        current_version,
        latest_version: latest_version.clone(),
        message: format!("版本 {} 已下载，正在安装并重启应用", latest_version),
        notes: release.body,
        verification: "github_https".to_string(),
    })
}

// ---------------------------------------------------------------------------
// Record process start time (used for uptime calculation)
// ---------------------------------------------------------------------------

static START_TIME: std::sync::OnceLock<u64> = std::sync::OnceLock::new();

#[cfg(test)]
#[path = "commands_regression_tests.rs"]
mod regression_tests;

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
            ipv4: vec!["192.168.110.95".to_string()],
            ipv6: global_ipv6.clone(),
            global_ipv6,
            online: true,
            source: "test".to_string(),
            last_seen: "2026-05-18T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn resolve_device_ipv6_uses_first_candidate_by_default() {
        let config = DeviceDdnsConfig {
            device_id: "device-1".to_string(),
            selected_ipv6: "2408:8200::11".to_string(),
            selected_ip: "2408:8200::11".to_string(),
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
    fn resolve_device_ipv6_uses_configured_candidate_index() {
        let config = DeviceDdnsConfig {
            device_id: "device-1".to_string(),
            ip_candidate_index: 2,
            selected_ipv6: "2408:8200::10".to_string(),
            selected_ip: "2408:8200::10".to_string(),
            ..DeviceDdnsConfig::default()
        };
        let devices = vec![lan_device(
            "device-1",
            "aa:bb:cc:dd:ee:ff",
            vec!["2408:8200::10", "2408:8200::11"],
        )];

        assert_eq!(
            resolve_device_ipv6(&config, &devices),
            Ok("2408:8200::11".to_string())
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
            selected_ipv6: "2408:8200::10".to_string(),
            ..DeviceDdnsConfig::default()
        };
        let devices = vec![lan_device(
            "device-1",
            "aa:bb:cc:dd:ee:ff",
            vec!["2408:8200::20"],
        )];

        assert_eq!(
            resolve_device_ipv6(&config, &devices),
            Ok("2408:8200::20".to_string())
        );
    }

    #[test]
    fn resolve_device_ipv6_ignores_selected_ipv6_outside_stable_candidates() {
        let config = DeviceDdnsConfig {
            device_id: "device-1".to_string(),
            selected_ipv6: "2408:8200::1".to_string(),
            selected_ip: "2408:8200::1".to_string(),
            ..DeviceDdnsConfig::default()
        };
        let mut device = lan_device("device-1", "aa:bb:cc:dd:ee:ff", vec!["2408:8200::2"]);
        device.ipv6.insert(0, "2408:8200::1".to_string());
        let devices = vec![device];

        assert_eq!(
            resolve_device_ipv6(&config, &devices),
            Ok("2408:8200::2".to_string())
        );
    }

    #[test]
    fn device_ddns_record_value_has_changed_when_selected_ipv6_disappeared() {
        let config = DeviceDdnsConfig {
            device_id: "device-1".to_string(),
            selected_ipv6: "2408:8200::10".to_string(),
            selected_ip: "2408:8200::10".to_string(),
            ..DeviceDdnsConfig::default()
        };
        let devices = vec![lan_device(
            "device-1",
            "aa:bb:cc:dd:ee:ff",
            vec!["2408:8200::20"],
        )];

        assert!(device_ddns_record_value_has_changed(&config, &devices));
    }

    #[test]
    fn device_ddns_record_value_has_not_changed_when_selected_ipv6_is_current() {
        let config = DeviceDdnsConfig {
            device_id: "device-1".to_string(),
            selected_ipv6: "2408:8200::10".to_string(),
            selected_ip: "2408:8200::10".to_string(),
            ..DeviceDdnsConfig::default()
        };
        let devices = vec![lan_device(
            "device-1",
            "aa:bb:cc:dd:ee:ff",
            vec!["2408:8200::10"],
        )];

        assert!(!device_ddns_record_value_has_changed(&config, &devices));
    }

    #[test]
    fn resolve_device_record_value_uses_ipv4_for_a_record() {
        let config = DeviceDdnsConfig {
            device_mac: "aa:bb:cc:dd:ee:ff".to_string(),
            record_type: "A".to_string(),
            ..DeviceDdnsConfig::default()
        };
        let devices = vec![lan_device(
            "device-1",
            "aa:bb:cc:dd:ee:ff",
            vec!["2408:8200::10"],
        )];

        assert_eq!(
            resolve_device_record_value(&config, &devices),
            Ok("192.168.110.95".to_string())
        );
    }

    #[test]
    fn device_ddns_device_is_online_matches_bound_mac_case_insensitively() {
        let config = DeviceDdnsConfig {
            device_mac: "AA:BB:CC:DD:EE:FF".to_string(),
            ..DeviceDdnsConfig::default()
        };
        let devices = vec![lan_device(
            "device-1",
            "aa:bb:cc:dd:ee:ff",
            vec!["2408:8200::10"],
        )];

        assert!(device_ddns_device_is_online(&config, &devices));
    }

    #[test]
    fn device_matching_prefers_mac_when_local_machine_id_conflicts() {
        let config = DeviceDdnsConfig {
            device_id: "local-machine".to_string(),
            device_mac: "8c:a6:82:71:b2:19".to_string(),
            ..DeviceDdnsConfig::default()
        };
        let local = lan_device(
            "local-machine",
            "d0:11:e5:a1:cb:dd",
            vec!["240e:358:1a2:df00::699"],
        );
        let target = lan_device(
            "mac-8c-a6-82-71-b2-19",
            "8c:a6:82:71:b2:19",
            vec!["240e:358:1a2:df00::cf0"],
        );

        assert!(!device_matches_config(&local, &config));
        assert!(device_matches_config(&target, &config));
        assert_eq!(
            resolve_device_ipv6(&config, &[local, target]),
            Ok("240e:358:1a2:df00::cf0".to_string())
        );
    }

    #[test]
    fn config_selector_does_not_merge_same_id_with_different_macs() {
        let config = DeviceDdnsConfig {
            device_id: "local-machine".to_string(),
            device_mac: "8c:a6:82:71:b2:19".to_string(),
            ..DeviceDdnsConfig::default()
        };

        assert!(!device_ddns_selector_matches(
            &config,
            "local-machine",
            "d0:11:e5:a1:cb:dd"
        ));
        assert!(device_ddns_selector_matches(
            &config,
            "mac-8c-a6-82-71-b2-19",
            "8C:A6:82:71:B2:19"
        ));
    }

    #[test]
    fn device_ddns_device_is_online_ignores_offline_discovery_rows() {
        let config = DeviceDdnsConfig {
            device_mac: "aa:bb:cc:dd:ee:ff".to_string(),
            ..DeviceDdnsConfig::default()
        };
        let mut device = lan_device("device-1", "aa:bb:cc:dd:ee:ff", vec!["2408:8200::10"]);
        device.online = false;

        assert!(!device_ddns_device_is_online(&config, &[device]));
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
            last_online: true,
            ..DeviceDdnsConfig::default()
        };
        let current = DeviceDdnsConfig {
            selected_ipv6: "2408:8200::2".to_string(),
            last_update_time: "new".to_string(),
            last_result: "new result".to_string(),
            last_online: false,
            ..original.clone()
        };

        let identity = device_ddns_identity(&original);

        assert!(!device_ddns_identity_matches(&current, &identity));

        let current_same_identity = DeviceDdnsConfig {
            last_update_time: "new".to_string(),
            last_result: "new result".to_string(),
            last_online: false,
            ..original.clone()
        };

        assert!(device_ddns_identity_matches(
            &current_same_identity,
            &identity
        ));
    }

    #[test]
    fn device_ddns_identity_tracks_ip_candidate_index() {
        let original = DeviceDdnsConfig {
            device_id: "device-1".to_string(),
            ip_candidate_index: 1,
            ..DeviceDdnsConfig::default()
        };
        let current = DeviceDdnsConfig {
            ip_candidate_index: 2,
            ..original.clone()
        };

        let identity = device_ddns_identity(&original);

        assert!(!device_ddns_identity_matches(&current, &identity));
    }

    #[test]
    fn upsert_device_ddns_config_replaces_only_matching_device() {
        let mut app_config = AppConfig::default();
        app_config.device_ddns_configs = vec![DeviceDdnsConfig {
            device_id: "local-machine".to_string(),
            device_name: "旧名称".to_string(),
            sub_domain: "old".to_string(),
            ..DeviceDdnsConfig::default()
        }];

        upsert_device_ddns_config(
            &mut app_config,
            DeviceDdnsConfig {
                device_id: "local-machine".to_string(),
                device_name: "客厅主机".to_string(),
                sub_domain: "game".to_string(),
                ..DeviceDdnsConfig::default()
            },
        );

        assert_eq!(app_config.device_ddns_configs.len(), 1);
        assert_eq!(app_config.device_ddns_configs[0].device_name, "客厅主机");
        assert_eq!(app_config.device_ddns_configs[0].sub_domain, "game");
    }

    #[test]
    fn effective_bindings_and_deletion_do_not_resurrect_old_nic_aliases() {
        let old = DeviceDdnsConfig {
            device_id: "old".into(),
            device_mac: "02:aa:bb:cc:dd:01".into(),
            domain: "example.test".into(),
            sub_domain: "game".into(),
            ..DeviceDdnsConfig::default()
        };
        let current = DeviceDdnsConfig {
            device_id: "current".into(),
            device_mac: "02:aa:bb:cc:dd:02".into(),
            ..old.clone()
        };
        let mut config = AppConfig {
            device_ddns: old.clone(),
            device_ddns_configs: vec![old, current.clone()],
            ..AppConfig::default()
        };
        assert_eq!(active_device_ddns_configs(&config), vec![current.clone()]);
        assert!(delete_device_ddns_config_from_config(
            &mut config,
            &current.device_id,
            &current.device_mac,
            None
        ));
        assert!(active_device_ddns_configs(&config).is_empty());
    }

    #[test]
    fn saving_a_record_reassigns_its_owner_instead_of_adding_a_duplicate() {
        let old = DeviceDdnsConfig {
            device_id: "old".into(),
            device_mac: "02:aa:bb:cc:dd:01".into(),
            domain: "example.test".into(),
            sub_domain: "game".into(),
            ..DeviceDdnsConfig::default()
        };
        let current = DeviceDdnsConfig {
            device_id: "current".into(),
            device_mac: "02:aa:bb:cc:dd:02".into(),
            ..old.clone()
        };
        let mut config = AppConfig {
            device_ddns_configs: vec![old],
            ..AppConfig::default()
        };
        upsert_device_ddns_config(&mut config, current.clone());
        assert_eq!(config.device_ddns_configs, vec![current.clone()]);
        assert_eq!(config.device_ddns, current);
    }

    #[test]
    fn local_machine_selector_survives_a_corrected_interface_mac() {
        let generated_local_config = DeviceDdnsConfig {
            device_id: "local-machine".to_string(),
            device_mac: "36:d6:8e:98:9f:1f".to_string(),
            device_name: "本机设备（lo0）".to_string(),
            ..DeviceDdnsConfig::default()
        };

        assert!(device_ddns_selector_matches(
            &generated_local_config,
            "local-machine",
            "d0:11:e5:a1:cb:dd"
        ));
        let corrected_local_device = lan_device(
            "local-machine",
            "d0:11:e5:a1:cb:dd",
            vec!["240e:358:1f9:4900::699"],
        );
        assert_eq!(
            resolve_device_ipv6(&generated_local_config, &[corrected_local_device]),
            Ok("240e:358:1f9:4900::699".to_string())
        );
        assert!(!device_selectors_match(
            "device-a",
            "36:d6:8e:98:9f:1f",
            "device-b",
            "d0:11:e5:a1:cb:dd"
        ));
    }

    #[test]
    fn delete_device_ddns_config_from_config_removes_only_matching_device() {
        let mut app_config = AppConfig::default();
        app_config.device_ddns_configs = vec![
            DeviceDdnsConfig {
                device_id: "local-machine".to_string(),
                ..DeviceDdnsConfig::default()
            },
            DeviceDdnsConfig {
                device_id: "router".to_string(),
                ..DeviceDdnsConfig::default()
            },
        ];

        assert!(delete_device_ddns_config_from_config(
            &mut app_config,
            "local-machine",
            "",
            None
        ));
        assert_eq!(app_config.device_ddns_configs.len(), 1);
        assert_eq!(app_config.device_ddns_configs[0].device_id, "router");
    }

    #[test]
    fn empty_ddns_sub_domain_uses_root_domain_and_validates() {
        let config = DdnsConfig {
            access_key_id: "ak".to_string(),
            access_key_secret: "secret".to_string(),
            domain: "example.com".to_string(),
            sub_domain: "  ".to_string(),
            ..DdnsConfig::default()
        };

        assert_eq!(ddns_domain(&config), "example.com");
        assert!(validate_ddns_config(&config).is_ok());
    }

    #[test]
    fn empty_device_ddns_sub_domain_uses_root_domain_and_validates() {
        let config = DeviceDdnsConfig {
            access_key_id: "ak".to_string(),
            access_key_secret: "secret".to_string(),
            domain: "example.com".to_string(),
            sub_domain: "  ".to_string(),
            device_mac: "aa:bb:cc:dd:ee:ff".to_string(),
            ..DeviceDdnsConfig::default()
        };

        assert_eq!(device_ddns_domain(&config), "example.com");
        assert!(validate_device_ddns_config(&config).is_ok());
    }

    #[test]
    fn macos_update_flow_clears_quarantine_in_applications() {
        let script = macos_quarantine_apple_script();

        assert!(script.contains("xattr -dr com.apple.quarantine /Applications/HomeNet.app"));
        assert!(script.contains("with administrator privileges"));
    }

    #[test]
    fn unsigned_macos_update_script_reopens_after_best_effort_quarantine_cleanup() {
        let script = build_unsigned_macos_update_script(std::path::Path::new("/tmp/HomeNet.zip"));

        assert!(script.contains("sudo xattr -dr com.apple.quarantine /Applications/HomeNet.app"));
        assert!(script.contains("osascript -e") && script.contains("|| true"));
        assert!(script.contains("open -n \"$APP_PATH\""));
        assert!(script.contains("pgrep -x \"HomeNet\""));
    }

    #[test]
    fn unsigned_update_selects_platform_release_assets() {
        let release = GithubRelease {
            tag_name: "v0.1.5".to_string(),
            body: String::new(),
            asset_urls_inferred: false,
            assets: vec![
                GithubReleaseAsset {
                    name: "HomeNet_v0.1.5_macos-arm64-app.zip".to_string(),
                    browser_download_url: "https://example.com/HomeNet.app.zip".to_string(),
                },
                GithubReleaseAsset {
                    name: "HomeNet_v0.1.5_windows-x64-setup.exe".to_string(),
                    browser_download_url: "https://example.com/HomeNet.setup.exe".to_string(),
                },
            ],
        };

        assert_eq!(
            select_github_update_asset(&release, UpdateAssetKind::MacosArm64AppZip)
                .map(|asset| asset.name.as_str()),
            Some("HomeNet_v0.1.5_macos-arm64-app.zip")
        );
        assert_eq!(
            select_github_update_asset(&release, UpdateAssetKind::WindowsX64Setup)
                .map(|asset| asset.name.as_str()),
            Some("HomeNet_v0.1.5_windows-x64-setup.exe")
        );
    }

    #[test]
    fn update_installation_requires_the_confirmed_version() {
        assert!(update_versions_match("v0.1.5", "0.1.5"));
        assert!(!update_versions_match("v0.1.5", "0.1.6"));
        assert!(!update_versions_match("latest", "0.1.6"));
    }

    #[test]
    fn release_version_comparison_accepts_github_tag_prefix() {
        assert!(is_release_version_newer("1.2.3", "v1.2.4"));
        assert!(is_release_version_newer("1.2.3", "1.3.0"));
    }

    #[test]
    fn release_version_comparison_rejects_same_or_older_versions() {
        assert!(!is_release_version_newer("1.2.3", "v1.2.3"));
        assert!(!is_release_version_newer("1.2.3", "v1.2.2"));
    }

    #[test]
    fn release_version_comparison_rejects_invalid_versions() {
        assert!(!is_release_version_newer("1.2.3", "latest"));
        assert!(!is_release_version_newer("dev", "v0.1.5"));
    }

    #[test]
    fn validate_reverse_proxy_auto_certificate_requires_https_and_acme_settings() {
        let mut rule = ReverseProxyRule {
            protocol: "HTTP".to_string(),
            domain: "proxy.example.com".to_string(),
            backend_ip: "127.0.0.1".to_string(),
            backend_port: 8080,
            tls: "auto".to_string(),
            ..ReverseProxyRule::default()
        };
        normalize_reverse_proxy_rule(&mut rule);

        let http_error = validate_reverse_proxy_rule(&rule).expect_err("auto cert requires HTTPS");
        assert!(http_error.contains("HTTPS"));

        rule.protocol = "HTTPS".to_string();
        rule.listen_port = 443;
        let settings_error =
            validate_reverse_proxy_rule(&rule).expect_err("auto cert requires ACME settings");
        assert!(settings_error.contains("ACME"));

        rule.acme_email = "admin@example.com".to_string();
        rule.acme_access_key_id = "ak".to_string();
        rule.acme_access_key_secret = "secret".to_string();
        rule.acme_dns_domain = "example.com".to_string();

        assert!(validate_reverse_proxy_rule(&rule).is_ok());
    }

    #[test]
    fn validate_reverse_proxy_manual_certificate_requires_certificate_files() {
        let mut rule = ReverseProxyRule {
            protocol: "HTTPS".to_string(),
            domain: "proxy.example.com".to_string(),
            listen_port: 443,
            backend_ip: "127.0.0.1".to_string(),
            backend_port: 8080,
            tls: "manual".to_string(),
            ..ReverseProxyRule::default()
        };
        normalize_reverse_proxy_rule(&mut rule);

        let error = validate_reverse_proxy_rule(&rule).expect_err("manual cert requires files");
        assert!(error.contains("证书"));

        rule.certificate_path = "C:\\certs\\proxy.pem".to_string();
        rule.private_key_path = "C:\\certs\\proxy.key".to_string();

        assert!(validate_reverse_proxy_rule(&rule).is_ok());
    }

    #[test]
    fn apply_issued_certificate_updates_reverse_proxy_rule_metadata() {
        let mut rule = ReverseProxyRule {
            tls: "auto".to_string(),
            domain: "proxy.example.com".to_string(),
            certificate_last_error: "old error".to_string(),
            ..ReverseProxyRule::default()
        };
        let issued = crate::certificates::IssuedCertificate {
            cert_path: "C:\\certs\\fullchain.pem".into(),
            key_path: "C:\\certs\\private-key.pem".into(),
            issued_at: "2026-05-19T00:00:00Z".to_string(),
            expires_at: "2026-08-17T00:00:00Z".to_string(),
        };

        apply_issued_certificate_to_rule(&mut rule, &issued);

        assert_eq!(rule.certificate_path, "C:\\certs\\fullchain.pem");
        assert_eq!(rule.private_key_path, "C:\\certs\\private-key.pem");
        assert_eq!(rule.certificate_last_issued_at, "2026-05-19T00:00:00Z");
        assert_eq!(rule.certificate_expires_at, "2026-08-17T00:00:00Z");
        assert_eq!(rule.certificate_last_error, "");
        assert!(rule.certificate.contains("2026-08-17T00:00:00Z"));
    }

    #[test]
    fn runtime_ddns_status_runs_when_device_ddns_is_enabled() {
        let mut config = AppConfig::default();
        config.ddns.enabled = false;
        config.device_ddns_configs = vec![DeviceDdnsConfig {
            enabled: true,
            device_id: "device-1".to_string(),
            device_mac: "aa:bb:cc:dd:ee:ff".to_string(),
            domain: "example.com".to_string(),
            sub_domain: "nas".to_string(),
            last_update_time: "2026-05-19 18:58:40".to_string(),
            ..DeviceDdnsConfig::default()
        }];

        assert_eq!(runtime_ddns_status(&config), "运行中");
        assert_eq!(
            latest_runtime_ddns_update_time(&config),
            "2026-05-19 18:58:40"
        );
    }

    #[test]
    fn runtime_ddns_status_stops_when_no_ddns_config_is_enabled() {
        let mut config = AppConfig::default();
        config.ddns.enabled = false;
        config.device_ddns_configs = vec![DeviceDdnsConfig {
            enabled: false,
            device_id: "device-1".to_string(),
            device_mac: "aa:bb:cc:dd:ee:ff".to_string(),
            last_update_time: "2026-05-19 18:58:40".to_string(),
            ..DeviceDdnsConfig::default()
        }];

        assert_eq!(runtime_ddns_status(&config), "已停止");
        assert_eq!(latest_runtime_ddns_update_time(&config), "暂无");
    }

    #[tokio::test]
    async fn webdav_config_update_retains_unchanged_password_hashes() {
        let existing = WebDavConfig {
            accounts: vec![WebDavAccount {
                id: "account-1".to_string(),
                username: "backup".to_string(),
                password_hash: "$argon2id$existing".to_string(),
                read_only: false,
                enabled: true,
            }],
            ..WebDavConfig::default()
        };
        let updated = webdav_config_from_request(
            &existing,
            SaveWebDavConfigRequest {
                enabled: false,
                listen_addr: "127.0.0.1".to_string(),
                listen_port: 19080,
                root_path: String::new(),
                global_read_only: false,
                accounts: vec![SaveWebDavAccountRequest {
                    id: "account-1".to_string(),
                    username: "backup".to_string(),
                    password: String::new(),
                    read_only: true,
                    enabled: true,
                }],
            },
        )
        .await
        .unwrap();

        assert_eq!(updated.accounts[0].password_hash, "$argon2id$existing");
        assert!(updated.accounts[0].read_only);
    }
}
