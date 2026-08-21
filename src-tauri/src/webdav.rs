use crate::config::{add_log, WebDavAccount, WebDavConfig};
use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use chrono::Local;
use dav_server::{
    body::Body, davpath::DavPath, localfs::LocalFs, memls::MemLs, DavConfig, DavHandler,
    DavMethodSet,
};
use http::{
    header::{AUTHORIZATION, WWW_AUTHENTICATE},
    Request, Response, StatusCode,
};
use hyper::{body::Incoming, server::conn::http1, service::service_fn};
use hyper_util::rt::TokioIo;
use rand_core::OsRng;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    convert::Infallible,
    fs,
    net::IpAddr,
    path::{Path, PathBuf},
    str::FromStr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::{net::TcpListener, task::JoinHandle};
use tokio_util::sync::CancellationToken;

const AUTH_CACHE_TTL: Duration = Duration::from_secs(5 * 60);
const MAX_AUTH_CACHE_ENTRIES: usize = 128;
const MAX_ACCOUNTS: usize = 32;

#[derive(Debug, Clone, Serialize)]
pub struct WebDavRuntimeStatus {
    pub state: String,
    pub message: String,
    pub endpoint: String,
    pub active_since: String,
}

impl WebDavRuntimeStatus {
    fn disabled(config: &WebDavConfig) -> Self {
        Self {
            state: "disabled".to_string(),
            message: "WebDAV 服务已停用".to_string(),
            endpoint: webdav_endpoint(config),
            active_since: String::new(),
        }
    }

    fn running(config: &WebDavConfig) -> Self {
        Self {
            state: "running".to_string(),
            message: "WebDAV 服务运行中".to_string(),
            endpoint: webdav_endpoint(config),
            active_since: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        }
    }

    fn error(config: &WebDavConfig, message: impl Into<String>) -> Self {
        Self {
            state: "error".to_string(),
            message: message.into(),
            endpoint: webdav_endpoint(config),
            active_since: String::new(),
        }
    }
}

#[derive(Clone)]
struct AuthenticatedAccount {
    id: String,
    read_only: bool,
}

struct CachedAuthentication {
    account: AuthenticatedAccount,
    expires_at: Instant,
}

struct ActiveWebDavServer {
    cancel: CancellationToken,
    join: JoinHandle<()>,
    signature: String,
    last_used: Arc<Mutex<HashMap<String, String>>>,
}

#[derive(Clone)]
struct WebDavServerContext {
    handler: DavHandler,
    accounts: Arc<Vec<WebDavAccount>>,
    auth_cache: Arc<Mutex<HashMap<String, CachedAuthentication>>>,
    last_used: Arc<Mutex<HashMap<String, String>>>,
    canonical_root: Arc<PathBuf>,
    global_read_only: bool,
    cancel: CancellationToken,
}

pub struct WebDavManager {
    active: Option<ActiveWebDavServer>,
    status: WebDavRuntimeStatus,
}

impl WebDavManager {
    pub fn new() -> Self {
        Self {
            active: None,
            status: WebDavRuntimeStatus {
                state: "disabled".to_string(),
                message: "WebDAV 服务尚未启动".to_string(),
                endpoint: String::new(),
                active_since: String::new(),
            },
        }
    }

    pub fn status(&self) -> WebDavRuntimeStatus {
        self.status.clone()
    }

    pub fn account_last_used(&self) -> HashMap<String, String> {
        self.active
            .as_ref()
            .and_then(|active| active.last_used.lock().ok().map(|value| value.clone()))
            .unwrap_or_default()
    }

    pub async fn apply(&mut self, config: &WebDavConfig) -> WebDavRuntimeStatus {
        if !config.enabled {
            self.stop_active().await;
            self.status = WebDavRuntimeStatus::disabled(config);
            return self.status();
        }

        if let Err(error) = validate_webdav_config(config) {
            self.stop_active().await;
            self.status = WebDavRuntimeStatus::error(config, error.clone());
            add_log("error", "WebDAV", &format!("WebDAV 配置无效：{}", error));
            return self.status();
        }

        let signature = webdav_config_signature(config);
        if let Some(active) = self.active.as_ref() {
            if active.signature == signature && !active.join.is_finished() {
                return self.status();
            }
        }

        self.stop_active().await;

        let canonical_root = match fs::canonicalize(config.root_path.trim()) {
            Ok(path) => path,
            Err(error) => {
                let message = format!("访问共享目录失败：{}", error);
                self.status = WebDavRuntimeStatus::error(config, message.clone());
                add_log("error", "WebDAV", &message);
                return self.status();
            }
        };
        let bind_endpoint = socket_endpoint(&config.listen_addr, config.listen_port);
        let listener = match TcpListener::bind(&bind_endpoint).await {
            Ok(listener) => listener,
            Err(error) => {
                let message = if error.kind() == std::io::ErrorKind::AddrInUse {
                    format!("监听端口 {} 已被占用", config.listen_port)
                } else {
                    format!("监听 {} 失败：{}", bind_endpoint, error)
                };
                self.status = WebDavRuntimeStatus::error(config, message.clone());
                add_log("error", "WebDAV", &message);
                return self.status();
            }
        };

        let handler = DavHandler::builder()
            .filesystem(LocalFs::new(
                &canonical_root,
                false,
                false,
                cfg!(target_os = "macos"),
            ))
            .locksystem(MemLs::new())
            .hide_symlinks(true)
            .autoindex(false)
            .build_handler();
        let accounts = Arc::new(config.accounts.clone());
        let auth_cache = Arc::new(Mutex::new(HashMap::new()));
        let last_used = Arc::new(Mutex::new(HashMap::new()));
        let global_read_only = config.global_read_only;
        let cancel = CancellationToken::new();
        let server_context = WebDavServerContext {
            handler,
            accounts,
            auth_cache,
            last_used: last_used.clone(),
            canonical_root: Arc::new(canonical_root),
            global_read_only,
            cancel: cancel.clone(),
        };
        let join = tokio::spawn(run_server(listener, server_context));

        self.active = Some(ActiveWebDavServer {
            cancel,
            join,
            signature,
            last_used,
        });
        self.status = WebDavRuntimeStatus::running(config);
        add_log(
            "info",
            "WebDAV",
            &format!(
                "WebDAV 服务已启动，监听 {}，共享目录 {}",
                bind_endpoint, config.root_path
            ),
        );
        self.status()
    }

    async fn stop_active(&mut self) {
        let Some(active) = self.active.take() else {
            return;
        };
        active.cancel.cancel();
        let mut join = active.join;
        if tokio::time::timeout(Duration::from_secs(3), &mut join)
            .await
            .is_err()
        {
            join.abort();
        }
        add_log("info", "WebDAV", "WebDAV 监听已停止");
    }
}

async fn run_server(listener: TcpListener, context: WebDavServerContext) {
    loop {
        tokio::select! {
            _ = context.cancel.cancelled() => return,
            accepted = listener.accept() => {
                match accepted {
                    Ok((stream, _peer_addr)) => {
                        let request_context = context.clone();
                        let connection_cancel = context.cancel.clone();
                        tokio::spawn(async move {
                            let io = TokioIo::new(stream);
                            let service = service_fn(move |request| handle_request(request, request_context.clone()));
                            let connection = http1::Builder::new()
                                .keep_alive(true)
                                .serve_connection(io, service);
                            tokio::select! {
                                _ = connection_cancel.cancelled() => {}
                                result = connection => {
                                    if let Err(error) = result {
                                        add_log("debug", "WebDAV", &format!("WebDAV 连接已结束：{}", error));
                                    }
                                }
                            }
                        });
                    }
                    Err(error) => {
                        add_log("error", "WebDAV", &format!("WebDAV 接收连接失败：{}", error));
                    }
                }
            }
        }
    }
}

async fn handle_request(
    request: Request<Incoming>,
    context: WebDavServerContext,
) -> Result<Response<Body>, Infallible> {
    let account = match authenticate_request(
        &request,
        context.accounts.clone(),
        context.auth_cache.clone(),
    )
    .await
    {
        Some(account) => account,
        None => return Ok(authentication_required_response()),
    };

    if !request_paths_are_safe(&request, &context.canonical_root).await {
        return Ok(forbidden_symlink_response());
    }

    if let Ok(mut activity) = context.last_used.lock() {
        activity.insert(
            account.id.clone(),
            Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        );
    }

    let mut request_config =
        DavConfig::new().principal(format!("/principals/users/{}", account.id));
    if context.global_read_only || account.read_only {
        request_config = request_config.methods(DavMethodSet::WEBDAV_RO);
    }
    Ok(context.handler.handle_with(request_config, request).await)
}

async fn authenticate_request(
    request: &Request<Incoming>,
    accounts: Arc<Vec<WebDavAccount>>,
    auth_cache: Arc<Mutex<HashMap<String, CachedAuthentication>>>,
) -> Option<AuthenticatedAccount> {
    let authorization = request.headers().get(AUTHORIZATION)?.to_str().ok()?;
    let cache_key = authentication_cache_key(authorization);
    if let Ok(mut cache) = auth_cache.lock() {
        let now = Instant::now();
        cache.retain(|_, entry| entry.expires_at > now);
        if let Some(entry) = cache.get(&cache_key) {
            return Some(entry.account.clone());
        }
    }

    let (username, password) = parse_basic_authorization(authorization)?;
    let account = accounts
        .iter()
        .find(|account| account.enabled && account.username == username)?
        .clone();
    let password_hash = account.password_hash.clone();
    let verified =
        tokio::task::spawn_blocking(move || verify_webdav_password(&password_hash, &password))
            .await
            .ok()
            .unwrap_or(false);
    if !verified {
        return None;
    }

    let authenticated = AuthenticatedAccount {
        id: account.id,
        read_only: account.read_only,
    };
    if let Ok(mut cache) = auth_cache.lock() {
        if cache.len() >= MAX_AUTH_CACHE_ENTRIES {
            cache.clear();
        }
        cache.insert(
            cache_key,
            CachedAuthentication {
                account: authenticated.clone(),
                expires_at: Instant::now() + AUTH_CACHE_TTL,
            },
        );
    }
    Some(authenticated)
}

fn authentication_required_response() -> Response<Body> {
    Response::builder()
        .status(StatusCode::UNAUTHORIZED)
        .header(
            WWW_AUTHENTICATE,
            "Basic realm=\"HomeNet WebDAV\", charset=\"UTF-8\"",
        )
        .body("需要 WebDAV 账号和密码".into())
        .expect("static WebDAV authentication response")
}

fn forbidden_symlink_response() -> Response<Body> {
    Response::builder()
        .status(StatusCode::FORBIDDEN)
        .body("WebDAV 不允许访问符号链接".into())
        .expect("static WebDAV symlink response")
}

async fn request_paths_are_safe(request: &Request<Incoming>, canonical_root: &Path) -> bool {
    let source_path = request.uri().path();
    if source_path != "*" {
        let Ok(source) = DavPath::new(source_path) else {
            return false;
        };
        if path_contains_symlink(canonical_root, source.as_rel_ospath()).await {
            return false;
        }

        if request.method().as_str().eq_ignore_ascii_case("COPY") {
            let source_on_disk = canonical_root.join(source.as_rel_ospath());
            if tree_contains_symlink(&source_on_disk).await {
                return false;
            }
        }
    }

    if matches!(request.method().as_str(), "COPY" | "MOVE") {
        let Some(destination) = request
            .headers()
            .get("destination")
            .and_then(|value| value.to_str().ok())
            .and_then(destination_path)
        else {
            return false;
        };
        let Ok(destination) = DavPath::new(&destination) else {
            return false;
        };
        if path_contains_symlink(canonical_root, destination.as_rel_ospath()).await {
            return false;
        }
    }

    true
}

fn destination_path(value: &str) -> Option<String> {
    let value = value.trim();
    if value.starts_with('/') {
        return Some(value.to_string());
    }
    url::Url::parse(value)
        .ok()
        .map(|url| url.path().to_string())
}

async fn path_contains_symlink(root: &Path, relative: &Path) -> bool {
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let std::path::Component::Normal(component) = component else {
            return true;
        };
        current.push(component);
        match tokio::fs::symlink_metadata(&current).await {
            Ok(metadata) if metadata.file_type().is_symlink() => return true,
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return false,
            Err(_) => return true,
        }
    }
    false
}

async fn tree_contains_symlink(path: &Path) -> bool {
    let metadata = match tokio::fs::symlink_metadata(path).await {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return false,
        Err(_) => return true,
    };
    if metadata.file_type().is_symlink() {
        return true;
    }
    if !metadata.is_dir() {
        return false;
    }

    let mut pending = vec![path.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let mut entries = match tokio::fs::read_dir(directory).await {
            Ok(entries) => entries,
            Err(_) => return true,
        };
        loop {
            let entry = match entries.next_entry().await {
                Ok(Some(entry)) => entry,
                Ok(None) => break,
                Err(_) => return true,
            };
            let file_type = match entry.file_type().await {
                Ok(file_type) => file_type,
                Err(_) => return true,
            };
            if file_type.is_symlink() {
                return true;
            }
            if file_type.is_dir() {
                pending.push(entry.path());
            }
        }
    }
    false
}

fn parse_basic_authorization(value: &str) -> Option<(String, String)> {
    let (scheme, encoded) = value.trim().split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("basic") {
        return None;
    }
    let decoded = STANDARD.decode(encoded.trim()).ok()?;
    let credentials = String::from_utf8(decoded).ok()?;
    let (username, password) = credentials.split_once(':')?;
    Some((username.to_string(), password.to_string()))
}

fn authentication_cache_key(value: &str) -> String {
    STANDARD.encode(Sha256::digest(value.as_bytes()))
}

pub fn hash_webdav_password(password: &str) -> Result<String, String> {
    if password.len() < 8 {
        return Err("WebDAV 密码至少需要 8 位".to_string());
    }
    if password.len() > 256 {
        return Err("WebDAV 密码不能超过 256 位".to_string());
    }
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|error| format!("生成 WebDAV 密码哈希失败：{}", error))
}

pub fn verify_webdav_password(password_hash: &str, password: &str) -> bool {
    let Ok(parsed_hash) = PasswordHash::new(password_hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok()
}

pub fn validate_webdav_config(config: &WebDavConfig) -> Result<(), String> {
    if config.listen_port == 0 {
        return Err("WebDAV 监听端口范围：1-65535".to_string());
    }
    IpAddr::from_str(config.listen_addr.trim())
        .map_err(|_| "WebDAV 监听地址必须是有效的 IPv4 或 IPv6 地址".to_string())?;
    if config.accounts.len() > MAX_ACCOUNTS {
        return Err(format!("WebDAV 账号数量不能超过 {} 个", MAX_ACCOUNTS));
    }

    let mut account_ids = HashSet::new();
    let mut usernames = HashSet::new();
    for account in &config.accounts {
        if account.id.trim().is_empty() || !account_ids.insert(account.id.trim()) {
            return Err("WebDAV 账号标识无效或重复，请重新保存账号配置".to_string());
        }
        let username = account.username.trim();
        if username.is_empty() {
            return Err("WebDAV 账号名称不能为空".to_string());
        }
        if username.len() > 64 || username.contains(':') || username.chars().any(char::is_control) {
            return Err(format!("WebDAV 账号名称不合法：{}", username));
        }
        if !usernames.insert(username.to_ascii_lowercase()) {
            return Err(format!("WebDAV 账号名称重复：{}", username));
        }
        if account.enabled && account.password_hash.trim().is_empty() {
            return Err(format!("WebDAV 账号尚未设置密码：{}", username));
        }
    }

    if !config.enabled {
        return Ok(());
    }
    if config.root_path.trim().is_empty() {
        return Err("请选择 WebDAV 共享目录".to_string());
    }
    let metadata = fs::metadata(Path::new(config.root_path.trim()))
        .map_err(|error| format!("读取 WebDAV 共享目录失败：{}", error))?;
    if !metadata.is_dir() {
        return Err("WebDAV 共享路径必须是目录".to_string());
    }
    if !config.accounts.iter().any(|account| account.enabled) {
        return Err("启用 WebDAV 服务前至少需要一个已启用账号".to_string());
    }
    Ok(())
}

pub fn webdav_endpoint(config: &WebDavConfig) -> String {
    let access_host = webdav_access_host(&config.listen_addr);
    format!(
        "http://{}/",
        socket_endpoint(&access_host, config.listen_port)
    )
}

fn webdav_access_host(listen_addr: &str) -> String {
    let listen_addr = listen_addr.trim();
    if !matches!(listen_addr, "0.0.0.0" | "::") {
        return listen_addr.to_string();
    }
    let prefer_ipv6 = listen_addr == "::";
    local_ip_address::list_afinet_netifas()
        .ok()
        .into_iter()
        .flatten()
        .map(|(_, address)| address)
        .find(|address| match address {
            IpAddr::V4(address) => {
                !prefer_ipv6 && !address.is_loopback() && !address.is_unspecified()
            }
            IpAddr::V6(address) => {
                prefer_ipv6
                    && !address.is_loopback()
                    && !address.is_unspecified()
                    && !address.is_multicast()
                    && !address.is_unicast_link_local()
            }
        })
        .map(|address| address.to_string())
        .unwrap_or_else(|| if prefer_ipv6 { "::1" } else { "127.0.0.1" }.to_string())
}

fn socket_endpoint(host: &str, port: u16) -> String {
    let host = host.trim();
    if host.contains(':') && !host.starts_with('[') {
        format!("[{}]:{}", host, port)
    } else {
        format!("{}:{}", host, port)
    }
}

fn webdav_config_signature(config: &WebDavConfig) -> String {
    serde_json::to_string(config).unwrap_or_else(|_| format!("{:?}", config))
}

pub async fn probe_webdav_listener(config: &WebDavConfig) -> Result<String, String> {
    if !config.enabled {
        return Err("WebDAV 服务尚未启用".to_string());
    }
    let probe_host = match config.listen_addr.trim() {
        "0.0.0.0" => "127.0.0.1",
        "::" => "::1",
        value => value,
    };
    let endpoint = format!(
        "http://{}/",
        socket_endpoint(probe_host, config.listen_port)
    );
    let method = reqwest::Method::from_bytes(b"PROPFIND")
        .map_err(|error| format!("创建 WebDAV 测试请求失败：{}", error))?;
    let response = reqwest::Client::new()
        .request(method, &endpoint)
        .header("Depth", "0")
        .timeout(Duration::from_secs(3))
        .send()
        .await
        .map_err(|error| format!("WebDAV 连接测试失败：{}", error))?;
    if response.status() != StatusCode::UNAUTHORIZED
        || !response.headers().contains_key(WWW_AUTHENTICATE)
    {
        return Err(format!(
            "监听端口未返回预期的 WebDAV 认证响应（HTTP {}）",
            response.status()
        ));
    }
    Ok(format!("连接测试成功：{}", webdav_endpoint(config)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn available_port() -> u16 {
        std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    }

    fn test_root() -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("homenet-webdav-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn password_hash_round_trip_does_not_store_plaintext() {
        let hash = hash_webdav_password("correct-horse-battery-staple").unwrap();
        assert!(!hash.contains("correct-horse-battery-staple"));
        assert!(verify_webdav_password(
            &hash,
            "correct-horse-battery-staple"
        ));
        assert!(!verify_webdav_password(&hash, "wrong-password"));
    }

    #[test]
    fn basic_authorization_preserves_colons_in_password() {
        let encoded = STANDARD.encode("admin:pass:word");
        assert_eq!(
            parse_basic_authorization(&format!("Basic {}", encoded)),
            Some(("admin".to_string(), "pass:word".to_string()))
        );
    }

    #[test]
    fn enabled_config_requires_directory_and_account() {
        let mut config = WebDavConfig {
            enabled: true,
            ..WebDavConfig::default()
        };
        assert!(validate_webdav_config(&config)
            .unwrap_err()
            .contains("共享目录"));

        config.enabled = false;
        assert!(validate_webdav_config(&config).is_ok());
    }

    #[test]
    fn endpoint_formats_ipv6_with_brackets() {
        let config = WebDavConfig {
            listen_addr: "::1".to_string(),
            ..WebDavConfig::default()
        };
        assert_eq!(webdav_endpoint(&config), "http://[::1]:19080/");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn manager_enforces_authentication_and_account_permissions() {
        let root = test_root();
        let port = available_port();
        let mut config = WebDavConfig {
            enabled: true,
            listen_addr: "127.0.0.1".to_string(),
            listen_port: port,
            root_path: root.to_string_lossy().to_string(),
            global_read_only: false,
            accounts: vec![
                WebDavAccount {
                    id: "writer".to_string(),
                    username: "writer".to_string(),
                    password_hash: hash_webdav_password("writer-password").unwrap(),
                    read_only: false,
                    enabled: true,
                },
                WebDavAccount {
                    id: "reader".to_string(),
                    username: "reader".to_string(),
                    password_hash: hash_webdav_password("reader-password").unwrap(),
                    read_only: true,
                    enabled: true,
                },
            ],
        };
        let mut manager = WebDavManager::new();
        assert_eq!(manager.apply(&config).await.state, "running");
        assert!(probe_webdav_listener(&config)
            .await
            .unwrap()
            .contains("连接测试成功"));

        let client = reqwest::Client::new();
        let endpoint = webdav_endpoint(&config);
        let propfind = reqwest::Method::from_bytes(b"PROPFIND").unwrap();
        let unauthorized = client
            .request(propfind.clone(), &endpoint)
            .header("Depth", "0")
            .send()
            .await
            .unwrap();
        assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
        assert!(unauthorized.headers().contains_key(WWW_AUTHENTICATE));

        let listing = client
            .request(propfind, &endpoint)
            .basic_auth("writer", Some("writer-password"))
            .header("Depth", "0")
            .send()
            .await
            .unwrap();
        assert_eq!(listing.status().as_u16(), 207);

        let file_url = format!("{}hello.txt", endpoint);
        let uploaded = client
            .put(&file_url)
            .basic_auth("writer", Some("writer-password"))
            .body("hello from HomeNet")
            .send()
            .await
            .unwrap();
        assert!(uploaded.status().is_success());
        let downloaded = client
            .get(&file_url)
            .basic_auth("reader", Some("reader-password"))
            .send()
            .await
            .unwrap();
        assert_eq!(downloaded.text().await.unwrap(), "hello from HomeNet");

        let denied = client
            .put(format!("{}denied.txt", endpoint))
            .basic_auth("reader", Some("reader-password"))
            .body("must not be written")
            .send()
            .await
            .unwrap();
        assert!(matches!(denied.status().as_u16(), 403 | 405));
        assert!(!root.join("denied.txt").exists());

        config.global_read_only = true;
        assert_eq!(manager.apply(&config).await.state, "running");
        let globally_denied = client
            .put(format!("{}globally-denied.txt", endpoint))
            .basic_auth("writer", Some("writer-password"))
            .body("global read-only must win")
            .send()
            .await
            .unwrap();
        assert!(matches!(globally_denied.status().as_u16(), 403 | 405));
        assert!(!root.join("globally-denied.txt").exists());
        config.global_read_only = false;
        assert_eq!(manager.apply(&config).await.state, "running");

        #[cfg(unix)]
        {
            let outside = test_root();
            fs::write(outside.join("secret.txt"), "must stay outside").unwrap();
            fs::create_dir(root.join("source")).unwrap();
            std::os::unix::fs::symlink(
                outside.join("secret.txt"),
                root.join("source/external.txt"),
            )
            .unwrap();

            let linked = client
                .get(format!("{}source/external.txt", endpoint))
                .basic_auth("writer", Some("writer-password"))
                .send()
                .await
                .unwrap();
            assert_eq!(linked.status(), StatusCode::FORBIDDEN);

            let copy = reqwest::Method::from_bytes(b"COPY").unwrap();
            let copied = client
                .request(copy, format!("{}source/", endpoint))
                .basic_auth("writer", Some("writer-password"))
                .header("Destination", format!("{}copied/", endpoint))
                .send()
                .await
                .unwrap();
            assert_eq!(copied.status(), StatusCode::FORBIDDEN);
            assert!(!root.join("copied/external.txt").exists());
            fs::remove_dir_all(outside).unwrap();
        }

        config.enabled = false;
        manager.apply(&config).await;
        fs::remove_dir_all(root).unwrap();
    }
}
