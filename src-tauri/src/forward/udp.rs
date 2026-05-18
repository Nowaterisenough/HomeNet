use crate::config::{add_log, ForwardRule};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::Mutex;
use tokio::time::{self, Duration};
use tokio_util::sync::CancellationToken;

const UDP_BUFFER_SIZE: usize = 65_535;
const CLIENT_IDLE_TIMEOUT: Duration = Duration::from_secs(60);

struct ClientRelay {
    target_socket: Arc<UdpSocket>,
}

type ClientRelays = Arc<Mutex<HashMap<SocketAddr, ClientRelay>>>;

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

fn udp_bind_host_for_target(target_addr: &str) -> &'static str {
    if target_addr.starts_with('[') {
        "[::]:0"
    } else {
        "0.0.0.0:0"
    }
}

pub async fn spawn_forwarder(rule: &ForwardRule) -> Result<CancellationToken, String> {
    let listen_addr = format_socket_endpoint(&rule.listen_addr, rule.listen_port, "::");
    let socket = Arc::new(
        UdpSocket::bind(&listen_addr)
            .await
            .map_err(|e| format!("UDP 端口 {} 绑定失败: {}", listen_addr, e))?,
    );

    let target_addr = format_socket_endpoint(&rule.target_ip, rule.target_port, "");
    let rule_id = rule.id.clone();
    let rule_id_for_log = rule_id.clone();
    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();

    tokio::spawn(async move {
        accept_loop(socket, target_addr, rule_id, cancel_clone).await;
    });

    add_log(
        "info",
        "转发",
        &format!("UDP 转发规则 [{}] 正在监听 {}", rule_id_for_log, listen_addr),
    );

    Ok(cancel)
}

async fn accept_loop(
    socket: Arc<UdpSocket>,
    target_addr: String,
    rule_id: String,
    cancel: CancellationToken,
) {
    let clients: ClientRelays = Arc::new(Mutex::new(HashMap::new()));
    let mut buffer = vec![0_u8; UDP_BUFFER_SIZE];

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                add_log("info", "转发", &format!("UDP 转发规则 [{}] 监听已停止", rule_id));
                return;
            }
            result = socket.recv_from(&mut buffer) => {
                match result {
                    Ok((size, client_addr)) => {
                        let target_socket = match get_or_create_target_socket(
                            socket.clone(),
                            clients.clone(),
                            client_addr,
                            &target_addr,
                            &rule_id,
                            cancel.clone(),
                        ).await {
                            Ok(socket) => socket,
                            Err(e) => {
                                add_log("error", "转发", &format!("UDP 转发规则 [{}] 创建目标通道失败：{}", rule_id, e));
                                continue;
                            }
                        };

                        if let Err(e) = target_socket.send(&buffer[..size]).await {
                            add_log("debug", "转发", &format!("UDP 转发规则 [{}] 客户端到目标转发异常：{}", rule_id, e));
                        }
                    }
                    Err(e) => {
                        add_log("error", "转发", &format!("UDP 转发规则 [{}] 接收数据失败：{}", rule_id, e));
                    }
                }
            }
        }
    }
}

async fn get_or_create_target_socket(
    listen_socket: Arc<UdpSocket>,
    clients: ClientRelays,
    client_addr: SocketAddr,
    target_addr: &str,
    rule_id: &str,
    cancel: CancellationToken,
) -> Result<Arc<UdpSocket>, String> {
    {
        let clients_guard = clients.lock().await;
        if let Some(relay) = clients_guard.get(&client_addr) {
            return Ok(relay.target_socket.clone());
        }
    }

    let target_socket = Arc::new(
        UdpSocket::bind(udp_bind_host_for_target(target_addr))
            .await
            .map_err(|e| format!("绑定本地 UDP 出口失败: {}", e))?,
    );
    target_socket
        .connect(target_addr)
        .await
        .map_err(|e| format!("连接目标 {} 失败: {}", target_addr, e))?;

    let mut clients_guard = clients.lock().await;
    if let Some(relay) = clients_guard.get(&client_addr) {
        return Ok(relay.target_socket.clone());
    }

    clients_guard.insert(
        client_addr,
        ClientRelay {
            target_socket: target_socket.clone(),
        },
    );
    drop(clients_guard);

    tokio::spawn(response_loop(
        listen_socket,
        clients,
        client_addr,
        target_socket.clone(),
        rule_id.to_string(),
        cancel,
    ));

    Ok(target_socket)
}

async fn response_loop(
    listen_socket: Arc<UdpSocket>,
    clients: ClientRelays,
    client_addr: SocketAddr,
    target_socket: Arc<UdpSocket>,
    rule_id: String,
    cancel: CancellationToken,
) {
    let mut buffer = vec![0_u8; UDP_BUFFER_SIZE];

    loop {
        tokio::select! {
            _ = cancel.cancelled() => return,
            result = time::timeout(CLIENT_IDLE_TIMEOUT, target_socket.recv(&mut buffer)) => {
                match result {
                    Ok(Ok(size)) => {
                        if let Err(e) = listen_socket.send_to(&buffer[..size], client_addr).await {
                            add_log("debug", "转发", &format!("UDP 转发规则 [{}] 目标到客户端转发异常：{}", rule_id, e));
                        }
                    }
                    Ok(Err(e)) => {
                        add_log("debug", "转发", &format!("UDP 转发规则 [{}] 目标读取异常：{}", rule_id, e));
                        clients.lock().await.remove(&client_addr);
                        return;
                    }
                    Err(_) => {
                        clients.lock().await.remove(&client_addr);
                        return;
                    }
                }
            }
        }
    }
}
