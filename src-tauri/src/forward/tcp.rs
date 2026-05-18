use crate::config::{add_log, ForwardRule};
use tokio::io;
use tokio::net::{TcpListener, TcpStream};
use tokio_util::sync::CancellationToken;

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

/// Spawn a TCP forwarding accept loop for the given rule.
///
/// Returns a `CancellationToken` that can be used to shut down the listener.
/// The accept loop runs in a separate tokio task.
pub async fn spawn_forwarder(rule: &ForwardRule) -> Result<CancellationToken, String> {
    let listen_addr = format_socket_endpoint(&rule.listen_addr, rule.listen_port, "::");

    let listener = TcpListener::bind(&listen_addr)
        .await
        .map_err(|e| format!("端口 {} 绑定失败: {}", listen_addr, e))?;

    let target_ip = rule.target_ip.clone();
    let target_port = rule.target_port;
    let rule_id = rule.id.clone();
    let rule_id_for_log = rule_id.clone();
    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();

    tokio::spawn(async move {
        accept_loop(listener, &target_ip, target_port, &rule_id, cancel_clone).await;
    });

    add_log(
        "info",
        "转发",
        &format!("转发规则 [{}] 正在监听 {}", rule_id_for_log, listen_addr),
    );

    Ok(cancel)
}

async fn accept_loop(
    listener: TcpListener,
    target_ip: &str,
    target_port: u16,
    rule_id: &str,
    cancel: CancellationToken,
) {
    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                add_log("info", "转发", &format!("转发规则 [{}] 监听已停止", rule_id));
                return;
            }
            result = listener.accept() => {
                match result {
                    Ok((incoming, remote_addr)) => {
                        add_log(
                            "debug",
                            "转发",
                            &format!("转发规则 [{}] 收到来自 {} 的连接", rule_id, remote_addr),
                        );
                        let target = format_socket_endpoint(target_ip, target_port, "");
                        let rid = rule_id.to_string();
                        tokio::spawn(async move {
                            relay(incoming, &target, &rid).await;
                        });
                    }
                    Err(e) => {
                        add_log(
                            "error",
                            "转发",
                            &format!("转发规则 [{}] 接收连接失败：{}", rule_id, e),
                        );
                    }
                }
            }
        }
    }
}

/// Bidirectional TCP relay between an incoming client and the target server.
async fn relay(mut incoming: TcpStream, target_addr: &str, rule_id: &str) {
    let mut target = match TcpStream::connect(target_addr).await {
        Ok(t) => t,
        Err(e) => {
            add_log(
                "error",
                "转发",
                &format!(
                    "转发规则 [{}] 连接目标 {} 失败：{}",
                    rule_id, target_addr, e
                ),
            );
            return;
        }
    };

    add_log(
        "debug",
        "转发",
        &format!("转发规则 [{}] 数据转发开始 → {}", rule_id, target_addr),
    );

    let (mut from_in, mut to_target) = incoming.split();
    let (mut from_target, mut to_in) = target.split();

    tokio::select! {
        r = io::copy(&mut from_in, &mut to_target) => {
            if let Err(e) = r {
                add_log("debug", "转发", &format!("转发规则 [{}] 客户端到目标转发异常：{}", rule_id, e));
            }
        }
        r = io::copy(&mut from_target, &mut to_in) => {
            if let Err(e) = r {
                add_log("debug", "转发", &format!("转发规则 [{}] 目标到客户端转发异常：{}", rule_id, e));
            }
        }
    }

    add_log("debug", "转发", &format!("转发规则 [{}] 数据转发结束", rule_id));
}
