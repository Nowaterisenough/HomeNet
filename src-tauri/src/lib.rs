mod autostart;
mod commands;
mod config;
mod ddns;
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
            commands::list_forward_rules,
            commands::save_forward_rule,
            commands::delete_forward_rule,
            commands::enable_forward_rule,
            commands::get_recent_logs,
            commands::clear_logs,
            commands::get_auto_start,
            commands::set_auto_start,
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
