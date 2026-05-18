mod autostart;
mod commands;
mod config;
mod ddns;
pub(crate) mod device_discovery;
mod forward;
mod logging;
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
            commands::save_device_ddns_config,
            commands::get_device_ddns_current_record,
            commands::trigger_device_ddns_update,
            commands::list_forward_rules,
            commands::save_forward_rule,
            commands::delete_forward_rule,
            commands::enable_forward_rule,
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
                config::add_log(
                    "warn",
                    "系统",
                    &format!("系统托盘初始化失败：{}", e),
                );
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

/// Device-level IPv6 DDNS periodic-check task.
///
/// It discovers LAN devices, resolves the configured device to its first
/// global IPv6 address, and updates an Aliyun AAAA record.
async fn device_ddns_background_task(app: tauri::AppHandle) {
    config::add_log("info", "设备DDNS", "设备 DDNS 后台任务已启动");
    tokio::time::sleep(std::time::Duration::from_secs(45)).await;

    loop {
        let (enabled, interval_secs, device_config) = {
            let state = app.state::<commands::AppState>();
            let cfg = state.config.lock().unwrap();
            (
                cfg.device_ddns.enabled,
                (cfg.device_ddns.interval_minutes.max(1) as u64) * 60,
                cfg.device_ddns.clone(),
            )
        };
        let identity = commands::device_ddns_identity(&device_config);

        if !enabled {
            tokio::time::sleep(std::time::Duration::from_secs(interval_secs)).await;
            continue;
        }

        let domain = commands::device_ddns_domain(&device_config);
        let devices = device_discovery::discover_lan_devices();
        let update_result = commands::update_device_ddns_record(&device_config, &devices).await;

        match update_result {
            Ok((ipv6, result)) => {
                let state = app.state::<commands::AppState>();
                match commands::apply_device_ddns_result_if_current(
                    &state,
                    &identity,
                    Some(ipv6.clone()),
                    result.clone(),
                ) {
                    Ok(true) => config::add_log(
                        "info",
                        "设备DDNS",
                        &format!("设备 DDNS 定时更新完成：{} -> {}，{}", domain, ipv6, result),
                    ),
                    Ok(false) => config::add_log(
                        "warn",
                        "设备DDNS",
                        &format!(
                            "设备 DDNS 定时更新完成但配置已变化，结果未写入：{} -> {}，{}",
                            domain, ipv6, result
                        ),
                    ),
                    Err(error) => config::add_log(
                        "error",
                        "设备DDNS",
                        &format!(
                            "设备 DDNS 定时更新完成但结果写入失败：{} -> {}，{}；{}",
                            domain, ipv6, result, error
                        ),
                    ),
                }
            }
            Err(error) => {
                let state = app.state::<commands::AppState>();
                match commands::apply_device_ddns_result_if_current(
                    &state,
                    &identity,
                    None,
                    error.clone(),
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

        tokio::time::sleep(std::time::Duration::from_secs(interval_secs)).await;
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
            if let Some(rule) = config.forward_rules.iter_mut().find(|r| r.id == result.rule_id) {
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
            if let Some(rule) = config.forward_rules.iter_mut().find(|r| r.id == result.rule_id) {
                rule.status = result.status.clone();
            }
        }
        let _ = config::save_config(&config);
    }
}
