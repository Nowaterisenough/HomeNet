use crate::config::{add_log, normalize_reverse_proxy_protocol, ReverseProxyRule};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer};
use tokio_rustls::rustls::server::{ClientHello, ResolvesServerCert};
use tokio_rustls::rustls::sign::CertifiedKey;
use tokio_rustls::rustls::{self, ServerConfig};
use tokio_rustls::TlsAcceptor;
use tokio_util::sync::CancellationToken;

const MAX_HTTP_HEADER_SIZE: usize = 64 * 1024;
const MAX_TLS_CLIENT_HELLO_SIZE: usize = 64 * 1024;

struct ActiveListener {
    cancel: CancellationToken,
    signature: String,
}

#[derive(Clone, Debug)]
struct RouteRule {
    id: String,
    domain: String,
    backend_ip: String,
    backend_port: u16,
    tls: String,
    certificate_path: String,
    private_key_path: String,
    remark: String,
}

#[derive(Clone)]
pub struct ReverseProxyApplyResult {
    pub rule_id: String,
    pub status: String,
}

pub struct ReverseProxyManager {
    active: HashMap<String, ActiveListener>,
}

fn format_socket_endpoint(host: &str, port: u16, default_host: &str) -> String {
    let normalized = if host.trim().is_empty() {
        default_host
    } else {
        host.trim()
    };
    if normalized.contains(':') && !normalized.starts_with('[') {
        format!("[{}]:{}", normalized, port)
    } else {
        format!("{}:{}", normalized, port)
    }
}

fn rule_label(rule: &ReverseProxyRule) -> String {
    if rule.remark.trim().is_empty() {
        rule.domain.trim().to_string()
    } else {
        rule.remark.trim().to_string()
    }
}

fn rule_to_route(rule: &ReverseProxyRule) -> RouteRule {
    RouteRule {
        id: rule.id.clone(),
        domain: normalize_domain(&rule.domain),
        backend_ip: rule.backend_ip.trim().to_string(),
        backend_port: rule.backend_port,
        tls: rule.tls.trim().to_ascii_lowercase(),
        certificate_path: rule.certificate_path.trim().to_string(),
        private_key_path: rule.private_key_path.trim().to_string(),
        remark: rule_label(rule),
    }
}

fn listener_key(rule: &ReverseProxyRule) -> String {
    let protocol = normalize_reverse_proxy_protocol(&rule.protocol);
    let mode = listener_mode(&protocol, &rule.tls);
    let listen_addr = if rule.listen_addr.trim().is_empty() {
        "::"
    } else {
        rule.listen_addr.trim()
    };
    format!("{}|{}|{}|{}", protocol, mode, listen_addr, rule.listen_port)
}

fn listener_signature(
    protocol: &str,
    mode: &str,
    listen_addr: &str,
    listen_port: u16,
    routes: &[RouteRule],
) -> String {
    let mut parts = routes
        .iter()
        .map(|rule| {
            format!(
                "{}>{}:{}:{}:{}:{}:{}",
                rule.domain,
                rule.backend_ip,
                rule.backend_port,
                rule.tls,
                rule.certificate_path,
                rule.private_key_path,
                rule.id
            )
        })
        .collect::<Vec<_>>();
    parts.sort();
    format!(
        "{}|{}|{}|{}|{}",
        protocol,
        mode,
        listen_addr,
        listen_port,
        parts.join(";")
    )
}

fn rule_requires_local_certificate(tls: &str) -> bool {
    matches!(tls.trim().to_ascii_lowercase().as_str(), "auto" | "manual")
}

fn listener_mode(protocol: &str, tls: &str) -> String {
    if protocol == "HTTPS" && rule_requires_local_certificate(tls) {
        "https-terminate".to_string()
    } else if protocol == "HTTPS" {
        "https-passthrough".to_string()
    } else {
        "http".to_string()
    }
}

fn route_matches(rule_domain: &str, host: &str) -> bool {
    if rule_domain == host {
        return true;
    }
    let Some(suffix) = rule_domain.strip_prefix("*.") else {
        return false;
    };
    host.ends_with(suffix) && host.len() > suffix.len()
}

fn normalize_domain(value: &str) -> String {
    let mut normalized = value.trim().trim_end_matches('.').to_ascii_lowercase();
    if normalized.starts_with('[') {
        if let Some(end) = normalized.find(']') {
            normalized = normalized[1..end].to_string();
        }
    } else if let Some((host, port)) = normalized.rsplit_once(':') {
        if !host.contains(':') && port.chars().all(|ch| ch.is_ascii_digit()) {
            normalized = host.to_string();
        }
    }
    normalized
}

struct RouteCertResolver {
    entries: Vec<(String, Arc<CertifiedKey>)>,
    fallback: Option<Arc<CertifiedKey>>,
}

impl fmt::Debug for RouteCertResolver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RouteCertResolver")
            .field(
                "domains",
                &self
                    .entries
                    .iter()
                    .map(|(domain, _)| domain)
                    .collect::<Vec<_>>(),
            )
            .finish()
    }
}

impl ResolvesServerCert for RouteCertResolver {
    fn resolve(&self, client_hello: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
        let Some(server_name) = client_hello.server_name().map(normalize_domain) else {
            return self.fallback.clone();
        };
        self.entries
            .iter()
            .find(|(domain, _)| route_matches(domain, &server_name))
            .map(|(_, key)| key.clone())
            .or_else(|| self.fallback.clone())
    }
}

fn load_cert_chain(path: &str) -> Result<Vec<CertificateDer<'static>>, String> {
    let bytes = fs::read(path).map_err(|e| format!("读取证书文件失败 {}: {}", path, e))?;
    rustls_pemfile::certs(&mut bytes.as_slice())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("解析证书文件失败 {}: {}", path, e))
}

fn load_private_key(path: &str) -> Result<PrivateKeyDer<'static>, String> {
    let bytes = fs::read(path).map_err(|e| format!("读取私钥文件失败 {}: {}", path, e))?;
    rustls_pemfile::private_key(&mut bytes.as_slice())
        .map_err(|e| format!("解析私钥文件失败 {}: {}", path, e))?
        .ok_or_else(|| format!("私钥文件未包含可用私钥: {}", path))
}

fn build_tls_acceptor(routes: &[RouteRule]) -> Result<TlsAcceptor, String> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut entries = Vec::new();
    let mut fallback = None;

    for route in routes {
        let cert_chain = load_cert_chain(&route.certificate_path)?;
        let private_key = load_private_key(&route.private_key_path)?;
        let certified_key = CertifiedKey::from_der(cert_chain, private_key, provider.as_ref())
            .map_err(|e| format!("加载 TLS 证书失败 {}: {}", route.domain, e))?;
        let certified_key = Arc::new(certified_key);
        if fallback.is_none() {
            fallback = Some(certified_key.clone());
        }
        entries.push((route.domain.clone(), certified_key));
    }

    let resolver = RouteCertResolver { entries, fallback };
    let config = ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| format!("初始化 TLS 协议失败: {}", e))?
        .with_no_client_auth()
        .with_cert_resolver(Arc::new(resolver));
    Ok(TlsAcceptor::from(Arc::new(config)))
}

fn find_route<'a>(routes: &'a [RouteRule], host: &str) -> Option<&'a RouteRule> {
    let host = normalize_domain(host);
    routes
        .iter()
        .find(|rule| route_matches(&rule.domain, &host))
}

fn find_http_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| index + 4)
}

fn extract_http_host(buf: &[u8], header_end: usize) -> Option<String> {
    let headers = std::str::from_utf8(&buf[..header_end]).ok()?;
    headers.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        if name.eq_ignore_ascii_case("host") {
            Some(value.trim().to_string())
        } else {
            None
        }
    })
}

async fn read_http_preamble<S>(stream: &mut S) -> Result<(Vec<u8>, String), String>
where
    S: AsyncRead + Unpin,
{
    let mut buf = Vec::with_capacity(4096);
    let mut chunk = [0_u8; 4096];

    loop {
        let size = stream
            .read(&mut chunk)
            .await
            .map_err(|e| format!("read HTTP request failed: {}", e))?;
        if size == 0 {
            return Err("client closed before sending HTTP headers".to_string());
        }
        buf.extend_from_slice(&chunk[..size]);
        if buf.len() > MAX_HTTP_HEADER_SIZE {
            return Err("HTTP headers are too large".to_string());
        }
        if let Some(header_end) = find_http_header_end(&buf) {
            let host = extract_http_host(&buf, header_end)
                .ok_or_else(|| "missing Host header".to_string())?;
            return Ok((buf, host));
        }
    }
}

async fn read_tls_preamble(stream: &mut TcpStream) -> Result<(Vec<u8>, String), String> {
    let mut buf = Vec::with_capacity(4096);
    let mut chunk = [0_u8; 4096];

    loop {
        let size = stream
            .read(&mut chunk)
            .await
            .map_err(|e| format!("read TLS ClientHello failed: {}", e))?;
        if size == 0 {
            return Err("client closed before sending TLS ClientHello".to_string());
        }
        buf.extend_from_slice(&chunk[..size]);
        if buf.len() > MAX_TLS_CLIENT_HELLO_SIZE {
            return Err("TLS ClientHello is too large".to_string());
        }
        if buf.len() >= 5 {
            if buf[0] != 0x16 {
                return Err("first TLS record is not a handshake".to_string());
            }
            let record_len = u16::from_be_bytes([buf[3], buf[4]]) as usize;
            let required = 5 + record_len;
            if required > MAX_TLS_CLIENT_HELLO_SIZE {
                return Err("TLS ClientHello record is too large".to_string());
            }
            if buf.len() >= required {
                let sni = extract_tls_sni(&buf[..required])
                    .ok_or_else(|| "TLS ClientHello does not contain SNI".to_string())?;
                return Ok((buf, sni));
            }
        }
    }
}

fn take_u8(data: &[u8], pos: &mut usize) -> Option<u8> {
    let value = *data.get(*pos)?;
    *pos += 1;
    Some(value)
}

fn take_u16(data: &[u8], pos: &mut usize) -> Option<u16> {
    let bytes = [*data.get(*pos)?, *data.get(*pos + 1)?];
    *pos += 2;
    Some(u16::from_be_bytes(bytes))
}

fn skip(data: &[u8], pos: &mut usize, len: usize) -> Option<()> {
    if *pos + len > data.len() {
        return None;
    }
    *pos += len;
    Some(())
}

fn extract_tls_sni(data: &[u8]) -> Option<String> {
    if data.len() < 9 || data[0] != 0x16 {
        return None;
    }
    let record_len = u16::from_be_bytes([data[3], data[4]]) as usize;
    if data.len() < 5 + record_len {
        return None;
    }

    let mut pos = 5;
    if take_u8(data, &mut pos)? != 0x01 {
        return None;
    }
    let handshake_len = ((take_u8(data, &mut pos)? as usize) << 16)
        | ((take_u8(data, &mut pos)? as usize) << 8)
        | (take_u8(data, &mut pos)? as usize);
    let handshake_end = pos.checked_add(handshake_len)?.min(5 + record_len);

    skip(data, &mut pos, 2)?;
    skip(data, &mut pos, 32)?;

    let session_id_len = take_u8(data, &mut pos)? as usize;
    skip(data, &mut pos, session_id_len)?;

    let cipher_suites_len = take_u16(data, &mut pos)? as usize;
    skip(data, &mut pos, cipher_suites_len)?;

    let compression_len = take_u8(data, &mut pos)? as usize;
    skip(data, &mut pos, compression_len)?;

    let extensions_len = take_u16(data, &mut pos)? as usize;
    let extensions_end = pos.checked_add(extensions_len)?.min(handshake_end);

    while pos + 4 <= extensions_end {
        let extension_type = take_u16(data, &mut pos)?;
        let extension_len = take_u16(data, &mut pos)? as usize;
        let extension_end = pos.checked_add(extension_len)?;
        if extension_end > extensions_end {
            return None;
        }

        if extension_type == 0 {
            let mut sni_pos = pos;
            let list_len = take_u16(data, &mut sni_pos)? as usize;
            let list_end = sni_pos.checked_add(list_len)?;
            while sni_pos + 3 <= list_end && list_end <= extension_end {
                let name_type = take_u8(data, &mut sni_pos)?;
                let name_len = take_u16(data, &mut sni_pos)? as usize;
                let name_end = sni_pos.checked_add(name_len)?;
                if name_end > list_end {
                    return None;
                }
                if name_type == 0 {
                    return std::str::from_utf8(&data[sni_pos..name_end])
                        .ok()
                        .map(normalize_domain);
                }
                sni_pos = name_end;
            }
        }

        pos = extension_end;
    }

    None
}

async fn send_http_response<S>(mut stream: S, status: &str, body: &str)
where
    S: AsyncWrite + Unpin,
{
    let response = format!(
        "HTTP/1.1 {}\r\nConnection: close\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\n\r\n{}",
        status,
        body.len(),
        body
    );
    let _ = stream.write_all(response.as_bytes()).await;
}

async fn relay_to_backend<S>(
    mut client: S,
    initial_bytes: Vec<u8>,
    route: &RouteRule,
    source: &str,
) where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let backend_addr = format_socket_endpoint(&route.backend_ip, route.backend_port, "");
    let mut backend = match TcpStream::connect(&backend_addr).await {
        Ok(stream) => stream,
        Err(e) => {
            add_log(
                "error",
                "反代",
                &format!(
                    "reverse proxy [{}] connect backend {} failed: {}",
                    route.remark, backend_addr, e
                ),
            );
            if source != "HTTPS-PASSTHROUGH" {
                send_http_response(client, "502 Bad Gateway", "backend unavailable").await;
            }
            return;
        }
    };

    if let Err(e) = backend.write_all(&initial_bytes).await {
        add_log(
            "debug",
            "反代",
            &format!(
                "reverse proxy [{}] write preamble failed: {}",
                route.remark, e
            ),
        );
        return;
    }

    if let Err(e) = tokio::io::copy_bidirectional(&mut client, &mut backend).await {
        add_log(
            "debug",
            "反代",
            &format!(
                "reverse proxy [{}] relay ended with error: {}",
                route.remark, e
            ),
        );
    }
}

async fn handle_http_stream<S>(mut client: S, routes: Vec<RouteRule>, source: &str)
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let (preamble, host) = match read_http_preamble(&mut client).await {
        Ok(value) => value,
        Err(e) => {
            send_http_response(client, "400 Bad Request", &e).await;
            return;
        }
    };

    let Some(route) = find_route(&routes, &host).cloned() else {
        send_http_response(client, "404 Not Found", "no reverse proxy rule for host").await;
        return;
    };

    relay_to_backend(client, preamble, &route, source).await;
}

async fn handle_http_client(client: TcpStream, routes: Vec<RouteRule>) {
    handle_http_stream(client, routes, "HTTP").await;
}

async fn handle_https_client(mut client: TcpStream, routes: Vec<RouteRule>) {
    let (preamble, sni) = match read_tls_preamble(&mut client).await {
        Ok(value) => value,
        Err(e) => {
            add_log("debug", "反代", &format!("TLS passthrough rejected: {}", e));
            return;
        }
    };

    let Some(route) = find_route(&routes, &sni).cloned() else {
        add_log(
            "debug",
            "反代",
            &format!("no HTTPS passthrough rule for SNI {}", sni),
        );
        return;
    };

    relay_to_backend(client, preamble, &route, "HTTPS-PASSTHROUGH").await;
}

async fn handle_https_terminated_client(
    client: TcpStream,
    routes: Vec<RouteRule>,
    acceptor: TlsAcceptor,
) {
    let tls_stream = match acceptor.accept(client).await {
        Ok(stream) => stream,
        Err(e) => {
            add_log("debug", "鍙嶄唬", &format!("TLS termination rejected: {}", e));
            return;
        }
    };
    handle_http_stream(tls_stream, routes, "HTTPS-TERMINATED").await;
}

async fn accept_loop(
    listener: TcpListener,
    mode: String,
    routes: Vec<RouteRule>,
    tls_acceptor: Option<TlsAcceptor>,
    cancel: CancellationToken,
) {
    loop {
        tokio::select! {
            _ = cancel.cancelled() => return,
            result = listener.accept() => {
                match result {
                    Ok((client, _remote_addr)) => {
                        let routes = routes.clone();
                        match mode.as_str() {
                            "https-passthrough" => {
                                tokio::spawn(handle_https_client(client, routes));
                            }
                            "https-terminate" => {
                                if let Some(acceptor) = tls_acceptor.clone() {
                                    tokio::spawn(handle_https_terminated_client(client, routes, acceptor));
                                }
                            }
                            _ => {
                                tokio::spawn(handle_http_client(client, routes));
                            }
                        }
                    }
                    Err(e) => {
                        add_log("error", "反代", &format!("reverse proxy accept failed: {}", e));
                    }
                }
            }
        }
    }
}

impl ReverseProxyManager {
    pub fn new() -> Self {
        Self {
            active: HashMap::new(),
        }
    }

    pub async fn apply_rules(
        &mut self,
        rules: &[ReverseProxyRule],
    ) -> Vec<ReverseProxyApplyResult> {
        let mut results = Vec::new();
        let mut grouped: HashMap<String, (String, String, String, u16, Vec<RouteRule>)> =
            HashMap::new();

        for rule in rules {
            if !rule.enabled {
                if !rule.id.trim().is_empty() {
                    results.push(ReverseProxyApplyResult {
                        rule_id: rule.id.clone(),
                        status: "已禁用".into(),
                    });
                }
                continue;
            }

            let protocol = normalize_reverse_proxy_protocol(&rule.protocol);
            let mode = listener_mode(&protocol, &rule.tls);
            if rule.id.trim().is_empty()
                || rule.domain.trim().is_empty()
                || rule.listen_port == 0
                || rule.backend_ip.trim().is_empty()
                || rule.backend_port == 0
            {
                results.push(ReverseProxyApplyResult {
                    rule_id: rule.id.clone(),
                    status: "错误".into(),
                });
                continue;
            }
            if mode == "https-terminate"
                && (rule.certificate_path.trim().is_empty()
                    || rule.private_key_path.trim().is_empty()
                    || !Path::new(rule.certificate_path.trim()).exists()
                    || !Path::new(rule.private_key_path.trim()).exists())
            {
                results.push(ReverseProxyApplyResult {
                    rule_id: rule.id.clone(),
                    status: "证书缺失".into(),
                });
                continue;
            }

            let listen_addr = if rule.listen_addr.trim().is_empty() {
                "::".to_string()
            } else {
                rule.listen_addr.trim().to_string()
            };
            let key = listener_key(rule);
            grouped
                .entry(key)
                .or_insert_with(|| (protocol, mode, listen_addr, rule.listen_port, Vec::new()))
                .4
                .push(rule_to_route(rule));
        }

        let desired_keys = grouped.keys().cloned().collect::<HashSet<_>>();
        let to_stop = self
            .active
            .keys()
            .filter(|key| !desired_keys.contains(*key))
            .cloned()
            .collect::<Vec<_>>();
        for key in to_stop {
            if let Some(active) = self.active.remove(&key) {
                active.cancel.cancel();
                add_log(
                    "info",
                    "反代",
                    &format!("reverse proxy listener [{}] stopped", key),
                );
            }
        }

        for (key, (protocol, mode, listen_addr, listen_port, routes)) in grouped {
            let signature = listener_signature(&protocol, &mode, &listen_addr, listen_port, &routes);
            if self
                .active
                .get(&key)
                .is_some_and(|active| active.signature == signature)
            {
                for route in routes {
                    results.push(ReverseProxyApplyResult {
                        rule_id: route.id,
                        status: "正常".into(),
                    });
                }
                continue;
            }

            if let Some(active) = self.active.remove(&key) {
                active.cancel.cancel();
            }

            let tls_acceptor = if mode == "https-terminate" {
                match build_tls_acceptor(&routes) {
                    Ok(acceptor) => Some(acceptor),
                    Err(e) => {
                        add_log(
                            "error",
                            "鍙嶄唬",
                            &format!("reverse proxy TLS config [{}] failed: {}", key, e),
                        );
                        for route in routes {
                            results.push(ReverseProxyApplyResult {
                                rule_id: route.id,
                                status: "证书错误".into(),
                            });
                        }
                        continue;
                    }
                }
            } else {
                None
            };

            let listen_endpoint = format_socket_endpoint(&listen_addr, listen_port, "::");
            match TcpListener::bind(&listen_endpoint).await {
                Ok(listener) => {
                    let cancel = CancellationToken::new();
                    tokio::spawn(accept_loop(
                        listener,
                        mode.clone(),
                        routes.clone(),
                        tls_acceptor,
                        cancel.clone(),
                    ));
                    self.active
                        .insert(key.clone(), ActiveListener { cancel, signature });
                    add_log(
                        "info",
                        "反代",
                        &format!("reverse proxy listener [{}] started", listen_endpoint),
                    );
                    for route in routes {
                        results.push(ReverseProxyApplyResult {
                            rule_id: route.id,
                            status: "正常".into(),
                        });
                    }
                }
                Err(e) => {
                    let status = if e.kind() == std::io::ErrorKind::AddrInUse {
                        "冲突"
                    } else {
                        "错误"
                    };
                    add_log(
                        "error",
                        "反代",
                        &format!("reverse proxy listener [{}] failed: {}", listen_endpoint, e),
                    );
                    for route in routes {
                        results.push(ReverseProxyApplyResult {
                            rule_id: route.id,
                            status: status.into(),
                        });
                    }
                }
            }
        }

        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_domain_strips_port_and_case() {
        assert_eq!(normalize_domain("Example.COM:8080"), "example.com");
        assert_eq!(normalize_domain("example.com."), "example.com");
    }

    #[test]
    fn wildcard_route_matches_subdomain_only() {
        assert!(route_matches("*.example.com", "nas.example.com"));
        assert!(!route_matches("*.example.com", "example.com"));
    }

    #[test]
    fn auto_and_manual_tls_modes_require_local_certificates() {
        assert!(rule_requires_local_certificate("auto"));
        assert!(rule_requires_local_certificate("manual"));
        assert!(!rule_requires_local_certificate("passthrough"));
        assert!(!rule_requires_local_certificate("off"));
    }

    #[tokio::test]
    async fn auto_tls_rule_without_certificate_is_marked_missing_certificate() {
        let mut manager = ReverseProxyManager::new();
        let rule = ReverseProxyRule {
            id: "rule-1".to_string(),
            enabled: true,
            protocol: "HTTPS".to_string(),
            domain: "proxy.example.com".to_string(),
            listen_addr: "127.0.0.1".to_string(),
            listen_port: 43199,
            backend_ip: "127.0.0.1".to_string(),
            backend_port: 8080,
            tls: "auto".to_string(),
            ..ReverseProxyRule::default()
        };

        let results = manager.apply_rules(&[rule]).await;

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].rule_id, "rule-1");
        assert_eq!(results[0].status, "证书缺失");
        assert!(manager.active.is_empty());
    }
}
