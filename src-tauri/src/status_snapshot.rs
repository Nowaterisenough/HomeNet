use serde::Serialize;
use tauri::State;

use crate::{
    commands::{self, AppState},
    config::{self, AppConfig, LogEntry, RuntimeStatus},
    webdav::WebDavRuntimeStatus,
};

#[derive(Serialize)]
pub struct ServiceSummary {
    id: String,
    page: &'static str,
    name: String,
    detail: String,
    status: String,
    enabled: bool,
}

#[derive(Serialize)]
pub struct TraySnapshot {
    runtime: RuntimeStatus,
    binding_count: usize,
    enabled_binding_count: usize,
    services: Vec<ServiceSummary>,
    logs: Vec<LogEntry>,
}

fn service_summaries(config: &AppConfig, webdav: WebDavRuntimeStatus) -> Vec<ServiceSummary> {
    let mut services = Vec::new();
    if config.ddns.enabled {
        services.push(ServiceSummary {
            id: "ddns".into(),
            page: "devices",
            name: "本机 DDNS".into(),
            detail: config.ddns.domain.clone(),
            status: "已启用".into(),
            enabled: true,
        });
    }
    for binding in commands::active_device_ddns_configs(config) {
        services.push(ServiceSummary {
            id: format!(
                "ddns:{}:{}",
                binding.device_id,
                commands::device_ddns_domain(&binding)
            ),
            page: "devices",
            name: commands::device_ddns_domain(&binding),
            detail: format!("设备 DDNS · {}", binding.device_name),
            status: if !binding.enabled {
                "已停用".into()
            } else if binding.last_result.is_empty() {
                "等待同步".into()
            } else {
                binding.last_result
            },
            enabled: binding.enabled,
        });
    }
    for rule in &config.forward_rules {
        services.push(ServiceSummary {
            id: format!("forward:{}", rule.id),
            page: "forward",
            name: if rule.remark.is_empty() {
                format!("{} :{}", rule.protocol, rule.listen_port)
            } else {
                rule.remark.clone()
            },
            detail: format!(
                "端口转发 · {} → {}:{}",
                rule.listen_port, rule.target_ip, rule.target_port
            ),
            status: rule_status(rule.enabled, &rule.status),
            enabled: rule.enabled,
        });
    }
    for rule in &config.reverse_proxy_rules {
        services.push(ServiceSummary {
            id: format!("proxy:{}", rule.id),
            page: "proxy",
            name: rule.domain.clone(),
            detail: format!(
                "{} 反向代理 · {}:{}",
                rule.protocol, rule.backend_ip, rule.backend_port
            ),
            status: rule_status(rule.enabled, &rule.status),
            enabled: rule.enabled,
        });
    }
    services.push(ServiceSummary {
        id: "webdav".into(),
        page: "webdav",
        name: "WebDAV 文件服务".into(),
        detail: if webdav.endpoint.is_empty() {
            "本地文件共享".into()
        } else {
            webdav.endpoint
        },
        status: match webdav.state.as_str() {
            "running" => "运行中".into(),
            "error" => format!("异常：{}", webdav.message),
            _ => "已停用".into(),
        },
        enabled: config.webdav.enabled,
    });
    services
}

fn rule_status(enabled: bool, status: &str) -> String {
    if !enabled {
        "已停用".into()
    } else if status.is_empty() {
        "等待启动".into()
    } else {
        status.into()
    }
}

/// Project only display fields; credentials and private-key paths never enter the popover.
#[tauri::command]
pub async fn get_tray_snapshot(state: State<'_, AppState>) -> Result<TraySnapshot, String> {
    let runtime = commands::get_runtime_status(state.clone()).await?;
    let config = state.config.lock().map_err(|e| e.to_string())?.clone();
    let webdav = state.webdav_manager.lock().await.status();
    let bindings = commands::active_device_ddns_configs(&config);
    Ok(TraySnapshot {
        runtime,
        binding_count: bindings.len(),
        enabled_binding_count: bindings.iter().filter(|binding| binding.enabled).count(),
        services: service_summaries(&config, webdav),
        logs: config::get_logs().into_iter().rev().take(12).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_rule_does_not_keep_a_stale_running_status() {
        assert_eq!(rule_status(false, "运行中"), "已停用");
        assert_eq!(rule_status(true, ""), "等待启动");
        assert_eq!(rule_status(true, "端口冲突"), "端口冲突");
    }

    #[test]
    fn snapshot_services_exclude_secrets() {
        let mut config = AppConfig::default();
        config.ddns.enabled = true;
        config.ddns.access_key_secret = "secret-not-for-the-popover".into();
        let webdav = WebDavRuntimeStatus {
            state: "disabled".into(),
            message: String::new(),
            endpoint: String::new(),
            active_since: String::new(),
        };
        let summaries = serde_json::to_string(&service_summaries(&config, webdav)).unwrap();
        assert!(!summaries.contains("secret-not-for-the-popover"));
        assert!(!summaries.contains("access_key"));
        assert!(!summaries.contains("private_key"));
    }
}
