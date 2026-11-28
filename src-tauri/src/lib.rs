mod autostart;
mod certificates;
mod commands;
mod config;
mod ddns;
pub(crate) mod device_discovery;
mod forward;
mod logging;
mod reverse_proxy;
mod status_snapshot;
mod tray;
mod webdav;
mod window_chrome;

use std::{collections::HashMap, sync::Mutex, time::Duration};
use tokio::time::Instant;
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

    #[cfg(target_os = "macos")]
    if let Err(error) = autostart::migrate_macos_autostart(app_config.auto_start) {
        config::add_log(
            "warn",
            "自启动",
            &format!("macOS 自启动项迁移失败：{}", error),
        );
    }

    // Wrap config and forward manager in shared state.
    let app_state = commands::AppState {
        ddns_config_changed: tokio::sync::watch::channel(()).0,
        config: Mutex::new(app_config),
        forward_manager: TokioMutex::new(forward::manager::ForwardManager::new()),
        reverse_proxy_manager: TokioMutex::new(reverse_proxy::ReverseProxyManager::new()),
        webdav_manager: TokioMutex::new(webdav::WebDavManager::new()),
    };

    // Build and run the Tauri application.
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
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
            commands::create_device_ddns_config,
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
            commands::get_webdav_config,
            commands::save_webdav_config,
            commands::set_webdav_enabled,
            commands::test_webdav_service,
            commands::select_webdav_directory,
            commands::get_recent_logs,
            commands::clear_logs,
            commands::get_auto_start,
            commands::set_auto_start,
            commands::get_auto_update_check,
            commands::set_auto_update_check,
            commands::check_app_update,
            commands::install_app_update,
            status_snapshot::get_tray_snapshot,
            tray::open_main_window,
            tray::take_main_page,
            tray::quit_app,
        ])
        .setup(|app| {
            // -- close-to-tray: hide window instead of closing ----------
            if let Some(window) = app.get_webview_window("main") {
                window_chrome::install_close_handler(&window);
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

            // -- WebDAV manager background task -----------------------
            let webdav_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                webdav_background_task(webdav_handle).await;
            });

            config::add_log("info", "系统", "应用初始化完成");
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app_handle, event| {
        #[cfg(target_os = "macos")]
        if let tauri::RunEvent::Reopen { .. } = event {
            tray::show_main_window(app_handle);
        }

        #[cfg(not(target_os = "macos"))]
        let _ = (app_handle, event);
    });
}

// ---------------------------------------------------------------------------
// Background tasks (M1 stubs)
// ---------------------------------------------------------------------------

fn ddns_interval(minutes: u32) -> Duration {
    Duration::from_secs(u64::from(minutes.max(1)) * 60)
}

async fn ddns_background_task(app: tauri::AppHandle) {
    use tokio::time::{sleep, sleep_until};
    let state = app.state::<commands::AppState>();
    let mut changed = state.ddns_config_changed.subscribe();

    config::add_log("info", "DDNS", "DDNS 后台任务已启动");
    sleep(Duration::from_secs(30)).await;
    let mut last_attempt: Option<Instant> = None;
    loop {
        let now = Instant::now();
        let (ddns_config, ipv6_interface) = {
            let cfg = state.config.lock().unwrap();
            (cfg.ddns.clone(), cfg.ipv6_interface.clone())
        };
        let interval = ddns_interval(ddns_config.interval_minutes);
        if !ddns_config.enabled {
            last_attempt = None;
        } else if last_attempt.is_none_or(|last| now >= last + interval) {
            last_attempt = Some(now);
            let (ipv4, ipv6) = if ddns_config.record_type == "AAAA" {
                (String::new(), ddns::get_local_ipv6_for_interface(&ipv6_interface))
            } else {
                (ddns::get_public_ipv4().await, String::new())
            };
            // The configuration can change while fetching the public address.
            {
                let cfg = state.config.lock().unwrap();
                if cfg.ddns != ddns_config || cfg.ipv6_interface != ipv6_interface {
                    last_attempt = None;
                    continue;
                }
            }
            let client = ddns::aliyun::AliyunDdns::new(ddns_config);
            match client.update_record(&ipv4, &ipv6).await {
                Ok(msg) => config::add_log("info", "DDNS", &format!("DDNS 定时更新完成：{}", msg)),
                Err(e) => config::add_log("error", "DDNS", &format!("DDNS 定时更新失败：{}", e)),
            }
        }

        tokio::select! {
            _ = sleep_until(last_attempt.unwrap_or(now) + interval), if last_attempt.is_some() => {},
            _ = changed.changed() => {},
        }
    }
}

type RecordKey = (String, String, String);

// Preserve the saved cadence across restarts. In memory we also track unsuccessful
// scans and offline devices, which deliberately do not update the last DNS sync time.
#[derive(Default)]
struct DeviceDdnsSchedule {
    attempts: HashMap<RecordKey, Instant>,
}

impl DeviceDdnsSchedule {
    fn due(
        &mut self,
        configs: Vec<config::DeviceDdnsConfig>,
        now: Instant,
        wall_time: chrono::DateTime<chrono::Local>,
    ) -> (Vec<config::DeviceDdnsConfig>, Option<Instant>) {
        let previous = std::mem::take(&mut self.attempts);
        let mut due = Vec::new();
        let mut next: Option<Instant> = None;
        for binding in configs.into_iter().filter(|item| item.enabled) {
            let Some(key) = config::device_ddns_record_key(&binding) else {
                continue;
            };
            let interval = ddns_interval(binding.interval_minutes);
            let elapsed = match previous.get(&key) {
                Some(attempt) => now.duration_since(*attempt),
                None => chrono::NaiveDateTime::parse_from_str(
                    binding.last_update_time.trim(), "%Y-%m-%d %H:%M:%S",
                )
                .ok()
                .and_then(|time| time.and_local_timezone(chrono::Local).single())
                .and_then(|time| (wall_time - time).to_std().ok())
                .unwrap_or(interval),
            };
            let deadline = if elapsed >= interval {
                self.attempts.insert(key, now);
                due.push(binding);
                now + interval
            } else {
                if let Some(attempt) = previous.get(&key) {
                    self.attempts.insert(key, *attempt);
                }
                now + (interval - elapsed)
            };
            next = Some(next.map_or(deadline, |earlier| earlier.min(deadline)));
        }
        (due, next)
    }
}

const DEVICE_DDNS_OFFLINE_RESULT: &str = "设备离线，等待上线后自动同步";

fn device_ddns_should_write_offline_state(config: &config::DeviceDdnsConfig) -> bool {
    config.last_online || config.last_result.trim() != DEVICE_DDNS_OFFLINE_RESULT
}

fn record_device_ddns_offline(
    state: &commands::AppState,
    config: &config::DeviceDdnsConfig,
    identity: &commands::DeviceDdnsOperationIdentity,
) -> Result<(), String> {
    if device_ddns_should_write_offline_state(config)
        && commands::apply_device_ddns_status_if_current(
            state, identity, None, DEVICE_DDNS_OFFLINE_RESULT.into(), Some(false), false,
        )?
    {
        config::add_log("info", "设备DDNS", &format!(
            "设备 DDNS 检测到设备离线，等待上线后自动同步：{}", commands::device_ddns_domain(config)
        ));
    }
    Ok(())
}

/// One fresh address inventory serves all bindings due in this scheduling round.
async fn device_ddns_background_task(app: tauri::AppHandle) {
    config::add_log("info", "设备DDNS", "设备 DDNS 后台任务已启动");
    let state = app.state::<commands::AppState>();
    let mut changed = state.ddns_config_changed.subscribe();
    let mut schedule = DeviceDdnsSchedule::default();
    loop {
        let configs = commands::active_device_ddns_configs(&state.config.lock().unwrap());
        let (due, next) = schedule.due(configs, Instant::now(), chrono::Local::now());
        if !due.is_empty() {
            match commands::refresh_device_addresses(&due).await {
                Ok(devices) => sync_due_devices(&state, due, &devices).await,
                Err(error) => config::add_log(
                    "error", "设备DDNS", &format!("刷新设备地址失败，本轮跳过同步：{error}"),
                ),
            }
            continue; // Time or configuration may have changed during discovery or DNS calls.
        }
        match next {
            Some(deadline) => tokio::select! {
                _ = tokio::time::sleep_until(deadline) => {},
                _ = changed.changed() => {},
            },
            None => {
                let _ = changed.changed().await;
            }
        }
    }
}

async fn sync_due_devices(
    state: &commands::AppState,
    device_configs: Vec<config::DeviceDdnsConfig>,
    devices: &[device_discovery::LanDevice],
) {
    for device_config in device_configs {
        let identity = commands::device_ddns_identity(&device_config);
        let domain = commands::device_ddns_domain(&device_config);
        // A binding can be edited or disabled while a shared scan is in flight.
        if !commands::device_binding_is_current(state, &device_config, &identity) {
            continue;
        }

        let Some(ip) = commands::resolve_device_record_value(&device_config, devices) else {
            if let Err(error) = record_device_ddns_offline(state, &device_config, &identity) {
                config::add_log("error", "设备DDNS", &format!("设备 DDNS 离线状态写入失败：{domain}，{error}"));
            }
            continue;
        };

        match commands::update_device_ddns_record(&device_config, ip).await {
            Ok(result) => {
                match commands::apply_device_ddns_status_if_current(
                    state,
                    &identity,
                    Some(ip.to_owned()),
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
                match commands::apply_device_ddns_status_if_current(
                    state,
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
}

/// Forward-rule supervision task.
///
/// Applies forwarding rules on startup, then periodically reconciles.
async fn forward_background_task(app: tauri::AppHandle) {
    config::add_log("info", "转发", "转发管理后台任务已启动");

    let state = app.state::<commands::AppState>();
    loop {
        commands::reapply_forward_rules(&state).await;
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
    }
}

async fn reverse_proxy_background_task(app: tauri::AppHandle) {
    config::add_log("info", "反代", "反向代理后台任务已启动");
    let state = app.state::<commands::AppState>();
    loop {
        commands::ensure_reverse_proxy_certificates(&state).await;
        commands::reapply_reverse_proxy_rules(&state).await;
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
    }
}

async fn webdav_background_task(app: tauri::AppHandle) {
    config::add_log("info", "WebDAV", "WebDAV 管理后台任务已启动");

    loop {
        let webdav_config = {
            let state = app.state::<commands::AppState>();
            let config = state.config.lock().unwrap();
            config.webdav.clone()
        };
        let state = app.state::<commands::AppState>();
        state
            .webdav_manager
            .lock()
            .await
            .apply(&webdav_config)
            .await;
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binding(domain: &str, minutes: u32) -> config::DeviceDdnsConfig {
        config::DeviceDdnsConfig {
            enabled: true,
            domain: domain.into(),
            interval_minutes: minutes,
            ..Default::default()
        }
    }

    #[test]
    fn offline_or_failed_scans_wait_for_each_bindings_configured_interval() {
        let start = Instant::now();
        let wall = chrono::Local::now();
        let bindings = vec![binding("a.example", 2), binding("b.example", 30)];
        let mut schedule = DeviceDdnsSchedule::default();
        let round = |schedule: &mut DeviceDdnsSchedule, minutes: u64| {
            let (due, _) = schedule.due(bindings.clone(), start + Duration::from_secs(minutes * 60), wall);
            due.into_iter().map(|c| c.domain).collect::<Vec<_>>()
        };
        assert_eq!(round(&mut schedule, 0), ["a.example", "b.example"]);
        assert!(round(&mut schedule, 1).is_empty());
        assert_eq!(round(&mut schedule, 2), ["a.example"]);
        assert_eq!(round(&mut schedule, 10), ["a.example"]);
        assert_eq!(round(&mut schedule, 30), ["a.example", "b.example"]);
    }

    #[test]
    fn restarting_uses_the_saved_sync_time_and_an_interval_edit_reschedules() {
        let now = Instant::now();
        let wall = chrono::Local::now();
        let mut b = binding("a.example", 30);
        b.last_update_time = (wall - chrono::Duration::minutes(5))
            .format("%Y-%m-%d %H:%M:%S").to_string();
        let mut schedule = DeviceDdnsSchedule::default();
        let (due, next) = schedule.due(vec![b.clone()], now, wall);
        assert!(due.is_empty());
        let remaining = next.unwrap().duration_since(now);
        assert!(remaining <= Duration::from_secs(25 * 60));
        assert!(remaining > Duration::from_secs(25 * 60 - 1));
        b.interval_minutes = 2;
        assert_eq!(schedule.due(vec![b.clone()], now, wall).0.len(), 1);
        b.enabled = false;
        assert_eq!(schedule.due(vec![b], now, wall), (Vec::new(), None));
    }
}
