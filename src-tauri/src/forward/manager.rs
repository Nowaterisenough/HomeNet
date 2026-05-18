use crate::config::{add_log, ForwardRule};
use crate::forward::{tcp, udp};
use std::collections::HashMap;
use tokio_util::sync::CancellationToken;

/// Tracks the listen address that was successfully bound for a rule.
struct ActiveListener {
    cancels: Vec<CancellationToken>,
    signature: String,
    #[allow(dead_code)]
    listen_addr: String,
}

enum ForwardProtocolKind {
    Tcp,
    Udp,
}

/// Manages the lifecycle of all port-forwarding listeners.
pub struct ForwardManager {
    active: HashMap<String, ActiveListener>,
}

/// Status returned for a rule after apply.
#[derive(Clone)]
pub struct RuleApplyResult {
    pub rule_id: String,
    pub status: String,
}

fn rule_label(rule: &ForwardRule) -> String {
    if rule.remark.trim().is_empty() {
        format!("{}:{}", rule.listen_addr.trim(), rule.listen_port)
    } else {
        rule.remark.trim().to_string()
    }
}

fn listen_endpoint(rule: &ForwardRule) -> String {
    let addr = if rule.listen_addr.trim().is_empty() {
        "::"
    } else {
        rule.listen_addr.trim()
    };
    format!("[{}]:{}", addr, rule.listen_port)
}

fn rule_signature(rule: &ForwardRule) -> String {
    let listen_addr = if rule.listen_addr.trim().is_empty() {
        "0.0.0.0"
    } else {
        rule.listen_addr.trim()
    };
    format!(
        "{}|{}|{}|{}|{}|{}",
        rule.protocol.trim().to_uppercase(),
        rule.mode.trim().to_lowercase(),
        listen_addr,
        rule.listen_port,
        rule.target_ip.trim(),
        rule.target_port,
    )
}

fn protocol_kinds(rule: &ForwardRule) -> Vec<ForwardProtocolKind> {
    match rule.protocol.trim().to_uppercase().as_str() {
        "UDP" => vec![ForwardProtocolKind::Udp],
        "TCP+UDP" | "UDP+TCP" => vec![ForwardProtocolKind::Tcp, ForwardProtocolKind::Udp],
        _ => vec![ForwardProtocolKind::Tcp],
    }
}

fn cancel_active(active: ActiveListener) {
    for cancel in active.cancels {
        cancel.cancel();
    }
}

impl ForwardManager {
    pub fn new() -> Self {
        Self {
            active: HashMap::new(),
        }
    }

    /// Apply a set of forwarding rules.
    /// Returns status updates for each rule (so the config can be updated).
    pub async fn apply_rules(&mut self, rules: &[ForwardRule]) -> Vec<RuleApplyResult> {
        let mut results: Vec<RuleApplyResult> = Vec::new();
        let desired_ids: Vec<&str> = rules.iter().map(|r| r.id.as_str()).collect();

        // Stop rules that were removed
        let to_stop: Vec<String> = self
            .active
            .keys()
            .filter(|id| !desired_ids.contains(&id.as_str()))
            .cloned()
            .collect();

        for id in &to_stop {
            if let Some(active) = self.active.remove(id) {
                cancel_active(active);
                add_log("info", "转发", &format!("转发规则 [{}] 已停止", id));
            }
        }

        // Start / restart rules
        for rule in rules {
            if !rule.enabled {
                if let Some(active) = self.active.remove(&rule.id) {
                    cancel_active(active);
                    add_log(
                        "info",
                        "转发",
                        &format!("转发规则 [{}] 已禁用，监听已关闭", rule_label(rule)),
                    );
                }
                results.push(RuleApplyResult {
                    rule_id: rule.id.clone(),
                    status: "已禁用".into(),
                });
                continue;
            }

            let signature = rule_signature(rule);

            // If already active with the same effective config, keep it.
            if let Some(active) = self.active.get(&rule.id) {
                if active.signature == signature {
                    results.push(RuleApplyResult {
                        rule_id: rule.id.clone(),
                        status: "正常".into(),
                    });
                    continue;
                }
            }

            if let Some(active) = self.active.remove(&rule.id) {
                cancel_active(active);
                add_log(
                    "info",
                    "转发",
                    &format!("转发规则 [{}] 配置已变更，正在重启监听", rule_label(rule)),
                );
            }

            // Start the listener
            let mut cancels = Vec::new();
            let mut start_error: Option<String> = None;
            for protocol in protocol_kinds(rule) {
                let result = match protocol {
                    ForwardProtocolKind::Tcp => tcp::spawn_forwarder(rule).await,
                    ForwardProtocolKind::Udp => udp::spawn_forwarder(rule).await,
                };
                match result {
                    Ok(cancel) => cancels.push(cancel),
                    Err(e) => {
                        start_error = Some(e);
                        break;
                    }
                }
            }

            match start_error {
                None => {
                    let listen_addr = format!(
                        "{}:{}",
                        if rule.listen_addr.is_empty() { "::" } else { &rule.listen_addr },
                        rule.listen_port
                    );
                    self.active.insert(
                        rule.id.clone(),
                        ActiveListener {
                            cancels,
                            signature: signature.clone(),
                            listen_addr: listen_addr.clone(),
                        },
                    );
                    add_log(
                        "info",
                        "转发",
                        &format!(
                            "转发规则 [{}] 已启动，监听 {} → {}:{}",
                            rule_label(rule),
                            listen_endpoint(rule),
                            rule.target_ip,
                            rule.target_port,
                        ),
                    );
                    results.push(RuleApplyResult {
                        rule_id: rule.id.clone(),
                        status: "正常".into(),
                    });
                }
                Some(e) => {
                    for cancel in cancels {
                        cancel.cancel();
                    }
                    let is_conflict = e.contains("绑定失败") || e.contains("Address in use");
                    let status = if is_conflict { "冲突" } else { "错误" };
                    if is_conflict {
                        add_log(
                            "warn",
                            "转发",
                            &format!(
                                "转发规则 [{}] 监听端口 {} 冲突，已跳过启动",
                                rule_label(rule),
                                rule.listen_port
                            ),
                        );
                    } else {
                        add_log(
                            "error",
                            "转发",
                            &format!("转发规则 [{}] 启动失败：{}", rule_label(rule), e),
                        );
                    }
                    // Stop any old listener for this rule on error
                    if let Some(active) = self.active.remove(&rule.id) {
                        cancel_active(active);
                    }
                    results.push(RuleApplyResult {
                        rule_id: rule.id.clone(),
                        status: status.into(),
                    });
                }
            }
        }

        results
    }

    /// Stop all active forwarding listeners.
    #[allow(dead_code)]
    pub async fn stop_all(&mut self) {
        for (id, active) in self.active.drain() {
            cancel_active(active);
            add_log("info", "转发", &format!("转发规则 [{}] 已停止", id));
        }
    }

    /// Return the number of active listeners.
    #[allow(dead_code)]
    pub fn active_count(&self) -> usize {
        self.active.len()
    }
}
