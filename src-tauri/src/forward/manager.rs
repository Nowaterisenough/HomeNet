use crate::config::{add_log, ForwardRule};
use crate::forward::tcp;
use std::collections::HashMap;
use tokio_util::sync::CancellationToken;

/// Tracks the listen address that was successfully bound for a rule.
struct ActiveListener {
    cancel: CancellationToken,
    #[allow(dead_code)]
    listen_addr: String,
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
                active.cancel.cancel();
                add_log("info", "转发", &format!("转发规则 [{}] 已停止", id));
            }
        }

        // Start / restart rules
        for rule in rules {
            if !rule.enabled {
                if let Some(active) = self.active.remove(&rule.id) {
                    active.cancel.cancel();
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

            // If already active, keep it
            if self.active.contains_key(&rule.id) {
                results.push(RuleApplyResult {
                    rule_id: rule.id.clone(),
                    status: "正常".into(),
                });
                continue;
            }

            // Start the listener
            match tcp::spawn_forwarder(rule).await {
                Ok(cancel) => {
                    let listen_addr = format!(
                        "{}:{}",
                        if rule.listen_addr.is_empty() { "::" } else { &rule.listen_addr },
                        rule.listen_port
                    );
                    self.active.insert(
                        rule.id.clone(),
                        ActiveListener {
                            cancel,
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
                Err(e) => {
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
                        active.cancel.cancel();
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
            active.cancel.cancel();
            add_log("info", "转发", &format!("转发规则 [{}] 已停止", id));
        }
    }

    /// Return the number of active listeners.
    #[allow(dead_code)]
    pub fn active_count(&self) -> usize {
        self.active.len()
    }
}
