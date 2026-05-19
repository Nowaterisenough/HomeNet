mod autostart;
mod certificates;
mod commands;
mod config;
mod ddns;
pub(crate) mod device_discovery;
mod forward;
mod logging;
mod reverse_proxy;
mod tray;

use std::sync::Mutex;
use tauri::Manager;
use tokio::sync::Mutex as TokioMutex;

// Re-export types so they are accessible from outside the crate if needed.
pub use config::LogEntry;

// ---------------------------------------------------------------------------
// Entry-point
// ---------------------------------------------------------------------------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Record process start time (used for uptime in runtime status).
    commands::record_start_time();

    // Load (or create) configuration from disk.
    let app_config = config::load_config();

    // Initialise tracing / file logging. The `_log_guard` MUST be kept
    // alive for the entire process lifetime, otherwise the file writer
    // thread is shut down and logs are dropped.
    let _log_guard = logging::setup_logging(&app_config.log_level);

    config::add_log("info", "系统", "应用启动中");
    forward::system::log_capabilities();

    // Wrap config and forward manager in shared state.
    let app_state = commands::AppState {
        config: Mutex::new(app_config),
        forward_manager: TokioMutex::new(forward::manager::ForwardManager::new()),
        reverse_proxy_manager: TokioMutex::new(reverse_proxy::ReverseProxyManager::new()),
    };

    // Build and run the Tauri application.
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            commands::get_runtime_status,
            commands::get_ddns_config,
            commands::save_ddns_config,
            commands::test_ddns_connection,
            commands::trigger_ddns_update,
            commands::get_ddns_current_record,
            commands::list_network_interfaces,
            commands::get_ipv6_interface,
            commands::set_ipv6_interface,
            commands::list_lan_devices,
            commands::get_device_ddns_config,
            commands::list_device_ddns_configs,
            commands::save_device_ddns_config,
            commands::delete_device_ddns_config,
            commands::get_device_ddns_current_record,
            commands::trigger_device_ddns_update,
            commands::list_forward_rules,
            commands::save_forward_rule,
            commands::delete_forward_rule,
            commands::enable_forward_rule,
            commands::list_reverse_proxy_rules,
            commands::save_reverse_proxy_rule,
            commands::delete_reverse_proxy_rule,
            commands::enable_reverse_proxy_rule,
            commands::issue_reverse_proxy_certificate,
            commands::get_recent_logs,
            commands::clear_logs,
            commands::get_auto_start,
            commands::set_auto_start,
            commands::install_app_update,
        ])
        .setup(|app| {
            // -- close-to-tray: hide window instead of closing ----------
            if let Some(window) = app.get_webview_window("main") {
                let window_clone = window.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = window_clone.hide();
                        config::add_log("debug", "系统", "主窗口已隐藏到系统托盘");
                    }
                });
            }

            // -- system tray ---------------------------------------------
            if let Err(e) = tray::setup_tray(app.handle()) {
                config::add_log("warn", "系统", &format!("系统托盘初始化失败：{}", e));
            }

            // -- DDNS background task ----------------------------------
            let ddns_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                ddns_background_task(ddns_handle).await;
            });

            // -- Device DDNS background task ---------------------------
            let device_ddns_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                device_ddns_background_task(device_ddns_handle).await;
            });

            // -- Forward manager background task -----------------------
            let fwd_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                forward_background_task(fwd_handle).await;
            });

            // -- Reverse proxy manager background task -----------------
            let reverse_proxy_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                reverse_proxy_background_task(reverse_proxy_handle).await;
            });

            config::add_log("info", "系统", "应用初始化完成");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

// ---------------------------------------------------------------------------
// Background tasks (M1 stubs)
// ---------------------------------------------------------------------------

/// DDNS periodic-check task.
///
/// Wakes every `interval_minutes`, fetches current IP addresses, and
/// updates the Alibaba Cloud DNS record when the IP has changed.
async fn ddns_background_task(app: tauri::AppHandle) {
    config::add_log("info", "DDNS", "DDNS 后台任务已启动");

    loop {
        // Wait first so the user has time to configure DDNS after startup
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;

        let (enabled, interval_secs, ipv6_interface) = {
            let state = app.state::<commands::AppState>();
            let cfg = state.config.lock().unwrap();
            (
                cfg.ddns.enabled,
                (cfg.ddns.interval_minutes.max(1) as u64) * 60,
                cfg.ipv6_interface.clone(),
            )
        };

        if !enabled {
            tokio::time::sleep(std::time::Duration::from_secs(interval_secs)).await;
            continue;
        }

        // Fetch IPs and trigger update
        let ipv4 = ddns::get_public_ipv4().await;
        let ipv6 = ddns::get_local_ipv6_for_interface(&ipv6_interface);

        let result = {
            let ddns_config = {
                let state = app.state::<commands::AppState>();
                let cfg = state.config.lock().unwrap();
                cfg.ddns.clone()
            };
            let client = ddns::aliyun::AliyunDdns::new(ddns_config);
            client.update_record(&ipv4, &ipv6).await
        };

        match &result {
            Ok(msg) => config::add_log("info", "DDNS", &format!("DDNS 定时更新完成：{}", msg)),
            Err(e) => config::add_log("error", "DDNS", &format!("DDNS 定时更新失败：{}", e)),
        }

        tokio::time::sleep(std::time::Duration::from_secs(interval_secs)).await;
    }
}

fn device_ddns_update_due(config: &config::DeviceDdnsConfig) -> bool {
    let last_update_time = config.last_update_time.trim();
    if last_update_time.is_empty() {
        return true;
    }

    let Ok(last_update_time) =
        chrono::NaiveDateTime::parse_from_str(last_update_time, "%Y-%m-%d %H:%M:%S")
    else {
        return true;
    };
    let Some(last_update_time) = last_update_time.and_local_timezone(chrono::Local).single()
    else {
        return true;
    };

    chrono::Local::now()
        .signed_duration_since(last_update_time)
        .num_minutes()
        >= config.interval_minutes.max(1) as i64
}

const DEVICE_DDNS_OFFLINE_RESULT: &str = "设备离线，等待上线后自动同步";

fn device_ddns_should_sync(config: &config::DeviceDdnsConfig, currently_online: bool) -> bool {
    currently_online && (!config.last_online || device_ddns_update_due(config))
}

fn device_ddns_should_write_offline_state(config: &config::DeviceDdnsConfig) -> bool {
    config.last_online || config.last_result.trim() != DEVICE_DDNS_OFFLINE_RESULT
}

/// Device-level DDNS periodic-check task.
///
/// It scans every enabled device config, records offline state, and syncs the
/// Aliyun A/AAAA record when the device comes online or its interval is due.
async fn device_ddns_background_task(app: tauri::AppHandle) {
    config::add_log("info", "设备DDNS", "设备 DDNS 后台任务已启动");
    tokio::time::sleep(std::time::Duration::from_secs(45)).await;

    loop {
        let device_configs = {
            let state = app.state::<commands::AppState>();
            let cfg = state.config.lock().unwrap();
            commands::active_device_ddns_configs(&cfg)
                .into_iter()
                .filter(|config| config.enabled)
                .collect::<Vec<_>>()
        };

        if device_configs.is_empty() {
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            continue;
        }

        let devices = device_discovery::discover_lan_devices();
        for device_config in device_configs {
            let identity = commands::device_ddns_identity(&device_config);
            let domain = commands::device_ddns_domain(&device_config);
            let currently_online =
                commands::device_ddns_device_is_online(&device_config, &devices);

            if !currently_online {
                if device_ddns_should_write_offline_state(&device_config) {
                    let state = app.state::<commands::AppState>();
                    match commands::apply_device_ddns_status_if_current(
                        &state,
                        &identity,
                        None,
                        DEVICE_DDNS_OFFLINE_RESULT.to_string(),
                        Some(false),
                        false,
                    ) {
                        Ok(true) => config::add_log(
                            "info",
                            "璁惧DDNS",
                            &format!("设备 DDNS 检测到设备离线，等待上线后自动同步：{}", domain),
                        ),
                        Ok(false) => config::add_log(
                            "warn",
                            "璁惧DDNS",
                            &format!(
                                "设备 DDNS 检测到设备离线但配置已变化，跳过状态写入：{}",
                                domain
                            ),
                        ),
                        Err(error) => config::add_log(
                            "error",
                            "璁惧DDNS",
                            &format!("设备 DDNS 离线状态写入失败：{}，{}", domain, error),
                        ),
                    }
                }
                continue;
            }

            if !device_ddns_should_sync(&device_config, currently_online) {
                continue;
            }

            let update_result =
                commands::update_device_ddns_record(&device_config, &devices).await;

            match update_result {
                Ok((ip, result)) => {
                    let state = app.state::<commands::AppState>();
                    match commands::apply_device_ddns_status_if_current(
                        &state,
                        &identity,
                        Some(ip.clone()),
                        result.clone(),
                        Some(true),
                        true,
                    ) {
                        Ok(true) => config::add_log(
                            "info",
                            "设备DDNS",
                            &format!(
                                "设备 DDNS 定时更新完成：{} -> {}，{}",
                                domain, ip, result
                            ),
                        ),
                        Ok(false) => config::add_log(
                            "warn",
                            "设备DDNS",
                            &format!(
                                "设备 DDNS 定时更新完成但配置已变化，结果未写入：{} -> {}，{}",
                                domain, ip, result
                            ),
                        ),
                        Err(error) => config::add_log(
                            "error",
                            "设备DDNS",
                            &format!(
                                "设备 DDNS 定时更新完成但结果写入失败：{} -> {}，{}；{}",
                                domain, ip, result, error
                            ),
                        ),
                    }
                }
                Err(error) => {
                    let state = app.state::<commands::AppState>();
                    match commands::apply_device_ddns_status_if_current(
                        &state,
                        &identity,
                        None,
                        error.clone(),
                        Some(true),
                        true,
                    ) {
                        Ok(true) => config::add_log(
                            "error",
                            "设备DDNS",
                            &format!("设备 DDNS 定时更新失败：{}，{}", domain, error),
                        ),
                        Ok(false) => config::add_log(
                            "warn",
                            "设备DDNS",
                            &format!(
                                "设备 DDNS 定时更新失败但配置已变化，失败结果未写入：{}，{}",
                                domain, error
                            ),
                        ),
                        Err(save_error) => config::add_log(
                            "error",
                            "设备DDNS",
                            &format!(
                                "设备 DDNS 定时更新失败且结果写入失败：{}，{}；{}",
                                domain, error, save_error
                            ),
                        ),
                    }
                }
            }
        }

        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
    }
}

/// Forward-rule supervision task.
///
/// Applies forwarding rules on startup, then periodically reconciles.
async fn forward_background_task(app: tauri::AppHandle) {
    config::add_log("info", "转发", "转发管理后台任务已启动");

    // Apply rules on startup
    {
        let rules = {
            let state = app.state::<commands::AppState>();
            let config = state.config.lock().unwrap();
            config.forward_rules.clone()
        };
        let state = app.state::<commands::AppState>();
        let mut manager = state.forward_manager.lock().await;
        let results = manager.apply_rules(&rules).await;
        drop(manager);

        // Sync status back to config
        let mut config = state.config.lock().unwrap();
        for result in &results {
            if let Some(rule) = config
                .forward_rules
                .iter_mut()
                .find(|r| r.id == result.rule_id)
            {
                rule.status = result.status.clone();
            }
        }
        let _ = config::save_config(&config);
    }

    // Periodically re-apply to catch any changes missed by commands
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;

        let rules = {
            let state = app.state::<commands::AppState>();
            let config = state.config.lock().unwrap();
            config.forward_rules.clone()
        };

        let state = app.state::<commands::AppState>();
        let mut manager = state.forward_manager.lock().await;
        let results = manager.apply_rules(&rules).await;
        drop(manager);

        // Sync status back to config
        let mut config = state.config.lock().unwrap();
        for result in &results {
            if let Some(rule) = config
                .forward_rules
                .iter_mut()
                .find(|r| r.id == result.rule_id)
            {
                rule.status = result.status.clone();
            }
        }
        let _ = config::save_config(&config);
    }
}

async fn reverse_proxy_background_task(app: tauri::AppHandle) {
    config::add_log("info", "反代", "反向代理后台任务已启动");

    {
        {
            let state = app.state::<commands::AppState>();
            commands::ensure_reverse_proxy_certificates(&state).await;
        }
        let rules = {
            let state = app.state::<commands::AppState>();
            let config = state.config.lock().unwrap();
            config.reverse_proxy_rules.clone()
        };
        let state = app.state::<commands::AppState>();
        let mut manager = state.reverse_proxy_manager.lock().await;
        let results = manager.apply_rules(&rules).await;
        drop(manager);

        let mut config = state.config.lock().unwrap();
        for result in &results {
            if let Some(rule) = config
                .reverse_proxy_rules
                .iter_mut()
                .find(|rule| rule.id == result.rule_id)
            {
                rule.status = result.status.clone();
            }
        }
        let _ = config::save_config(&config);
    }

    loop {
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;

        {
            let state = app.state::<commands::AppState>();
            commands::ensure_reverse_proxy_certificates(&state).await;
        }

        let rules = {
            let state = app.state::<commands::AppState>();
            let config = state.config.lock().unwrap();
            config.reverse_proxy_rules.clone()
        };

        let state = app.state::<commands::AppState>();
        let mut manager = state.reverse_proxy_manager.lock().await;
        let results = manager.apply_rules(&rules).await;
        drop(manager);

        let mut config = state.config.lock().unwrap();
        for result in &results {
            if let Some(rule) = config
                .reverse_proxy_rules
                .iter_mut()
                .find(|rule| rule.id == result.rule_id)
            {
                rule.status = result.status.clone();
            }
        }
        let _ = config::save_config(&config);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recent_device_ddns_config(last_online: bool) -> config::DeviceDdnsConfig {
        config::DeviceDdnsConfig {
            enabled: true,
            interval_minutes: 60,
            last_online,
            last_update_time: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            ..config::DeviceDdnsConfig::default()
        }
    }

    #[test]
    fn device_ddns_should_sync_when_device_comes_online_even_if_interval_not_due() {
        let config = recent_device_ddns_config(false);

        assert!(device_ddns_should_sync(&config, true));
    }

    #[test]
    fn device_ddns_should_not_sync_when_online_device_is_not_due() {
        let config = recent_device_ddns_config(true);

        assert!(!device_ddns_should_sync(&config, true));
    }

    #[test]
    fn device_ddns_should_not_sync_when_device_is_offline() {
        let config = recent_device_ddns_config(false);

        assert!(!device_ddns_should_sync(&config, false));
    }
}
