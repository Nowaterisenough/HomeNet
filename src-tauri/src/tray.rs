use std::{
    sync::Mutex,
    time::{Duration, Instant},
};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, Rect, WebviewUrl,
    WebviewWindowBuilder,
};

const TRAY_TOOLTIP: &str = "HomeNet";
const POPOVER_LABEL: &str = "tray-status";
const POPOVER_WIDTH: f64 = 460.0;
const POPOVER_HEIGHT: f64 = 600.0;

#[derive(Default)]
struct TrayState {
    blurred_at: Mutex<Option<Instant>>,
    pending_page: Mutex<Option<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrayIconAction {
    TogglePopover,
    RestoreMainWindow,
    ShowContextMenu,
    Ignore,
}

fn tray_icon_action(event: &TrayIconEvent) -> TrayIconAction {
    match event {
        TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } => TrayIconAction::TogglePopover,
        TrayIconEvent::DoubleClick {
            button: MouseButton::Left,
            ..
        } => TrayIconAction::RestoreMainWindow,
        TrayIconEvent::Click {
            button: MouseButton::Right,
            button_state: MouseButtonState::Up,
            ..
        } => TrayIconAction::ShowContextMenu,
        _ => TrayIconAction::Ignore,
    }
}

pub(crate) fn show_main_window(app: &AppHandle) {
    if let Some(popover) = app.get_webview_window(POPOVER_LABEL) {
        let _ = popover.hide();
    }
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[tauri::command]
pub fn open_main_window(app: AppHandle, page: Option<String>) -> Result<(), String> {
    if let Some(ref page) = page {
        if !matches!(
            page.as_str(),
            "overview" | "devices" | "forward" | "proxy" | "webdav" | "logs" | "settings"
        ) {
            return Err("未知页面".into());
        }
    }
    *app.state::<TrayState>()
        .pending_page
        .lock()
        .map_err(|e| e.to_string())? = page;
    // Retain the destination until the main webview is ready to consume it.
    app.emit_to("main", "homenet:navigate", ())
        .map_err(|e| e.to_string())?;
    show_main_window(&app);
    Ok(())
}

#[tauri::command]
pub fn take_main_page(app: AppHandle) -> Result<Option<String>, String> {
    Ok(app
        .state::<TrayState>()
        .pending_page
        .lock()
        .map_err(|e| e.to_string())?
        .take())
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    crate::config::add_log("info", "托盘", "用户从系统托盘退出应用");
    app.exit(0);
}

// All geometry is physical pixels, including negative origins on secondary displays.
fn popover_position(
    anchor: (f64, f64, f64, f64),
    work_area: (f64, f64, f64, f64),
    size: (f64, f64),
    gap: f64,
) -> PhysicalPosition<i32> {
    let (ax, ay, aw, ah) = anchor;
    let (wx, wy, ww, wh) = work_area;
    let (width, height) = size;
    let x = (ax + aw / 2.0 - width / 2.0).clamp(wx, (wx + ww - width).max(wx));
    let below = ay + ah + gap;
    let y = if below + height <= wy + wh {
        below
    } else {
        ay - height - gap
    }
    .clamp(wy, (wy + wh - height).max(wy));
    PhysicalPosition::new(x.round() as i32, y.round() as i32)
}

fn show_popover(
    app: &AppHandle,
    anchor: Option<(Rect, PhysicalPosition<f64>)>,
) -> tauri::Result<()> {
    let Some(window) = app.get_webview_window(POPOVER_LABEL) else {
        return Ok(());
    };
    let monitor = if let Some((_, point)) = anchor {
        app.monitor_from_point(point.x, point.y)?
    } else {
        app.primary_monitor()?
    };
    if let Some(monitor) = monitor {
        let scale = monitor.scale_factor();
        let area = monitor.work_area();
        let width = (POPOVER_WIDTH * scale).min(area.size.width as f64);
        let height = (POPOVER_HEIGHT * scale).min(area.size.height as f64);
        let work_area = (
            area.position.x as f64,
            area.position.y as f64,
            area.size.width as f64,
            area.size.height as f64,
        );
        let anchor = anchor
            .map(|(rect, _)| {
                let position = rect.position.to_physical::<f64>(scale);
                let size = rect.size.to_physical::<f64>(scale);
                (position.x, position.y, size.width, size.height)
            })
            .unwrap_or((
                work_area.0 + work_area.2 - width / 2.0 - 12.0 * scale,
                work_area.1,
                0.0,
                0.0,
            ));
        window.set_size(PhysicalSize::new(
            width.round() as u32,
            height.round() as u32,
        ))?;
        window.set_position(popover_position(
            anchor,
            work_area,
            (width, height),
            6.0 * scale,
        ))?;
    } else {
        window.center()?;
    }
    window.show()?;
    window.set_focus()?;
    Ok(())
}

fn toggle_popover(
    app: &AppHandle,
    rect: Rect,
    position: PhysicalPosition<f64>,
) -> tauri::Result<()> {
    let Some(window) = app.get_webview_window(POPOVER_LABEL) else {
        return Ok(());
    };
    if window.is_visible()? {
        return window.hide();
    }
    // Clicking the icon can blur/hide the window before the mouse-up event arrives.
    let state = app.state::<TrayState>();
    let just_blurred = state
        .blurred_at
        .lock()
        .unwrap()
        .take()
        .is_some_and(|time| time.elapsed() < Duration::from_millis(300));
    if !just_blurred {
        show_popover(app, Some((rect, position)))?;
    }
    Ok(())
}

pub fn setup_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    app.manage(TrayState::default());
    let popover = WebviewWindowBuilder::new(
        app,
        POPOVER_LABEL,
        WebviewUrl::App("index.html?view=tray".into()),
    )
    .title("HomeNet 状态")
    .inner_size(POPOVER_WIDTH, POPOVER_HEIGHT)
    .decorations(false)
    .resizable(false)
    .maximizable(false)
    .minimizable(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .shadow(true)
    .visible(false)
    .focused(false)
    .build()?;
    let popover_handle = popover.clone();
    popover.on_window_event(move |event| match event {
        tauri::WindowEvent::Focused(false) => {
            if popover_handle.is_visible().unwrap_or(false) {
                *popover_handle
                    .app_handle()
                    .state::<TrayState>()
                    .blurred_at
                    .lock()
                    .unwrap() = Some(Instant::now());
                let _ = popover_handle.hide();
            }
        }
        tauri::WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            let _ = popover_handle.hide();
        }
        _ => {}
    });

    // Linux tray implementations may only support a menu, not pointer events.
    let status_item = MenuItem::with_id(app, "status", "查看当前状态", true, None::<&str>)?;
    let show_item = MenuItem::with_id(app, "show", "打开 HomeNet", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&status_item, &show_item, &quit_item])?;
    let builder = TrayIconBuilder::with_id("main-tray")
        .icon(tauri::include_image!("icons/tray-icon.png"))
        .icon_as_template(cfg!(target_os = "macos"))
        .tooltip(TRAY_TOOLTIP);
    // Do not attach an NSStatusItem menu: AppKit can open it before our left-click handler.
    // On macOS only an explicit right-click opens the context menu.
    #[cfg(not(target_os = "macos"))]
    let builder = builder.menu(&menu);
    builder
        .on_menu_event(|app, event| match event.id().as_ref() {
            "status" => {
                if let Err(error) = show_popover(app, None) {
                    crate::config::add_log("warn", "托盘", &error.to_string());
                }
            }
            "show" => show_main_window(app),
            "quit" => quit_app(app.clone()),
            _ => {}
        })
        .show_menu_on_left_click(cfg!(target_os = "linux"))
        .on_tray_icon_event(move |tray, event| match tray_icon_action(&event) {
            TrayIconAction::TogglePopover => {
                if let TrayIconEvent::Click { rect, position, .. } = event {
                    if let Err(error) = toggle_popover(tray.app_handle(), rect, position) {
                        crate::config::add_log("warn", "托盘", &error.to_string());
                    }
                }
            }
            TrayIconAction::RestoreMainWindow => show_main_window(tray.app_handle()),
            TrayIconAction::ShowContextMenu => {
                #[cfg(target_os = "macos")]
                if let Some(window) = tray.app_handle().get_webview_window("main") {
                    if let Err(error) = window.popup_menu(&menu) {
                        crate::config::add_log("warn", "托盘", &error.to_string());
                    }
                }
            }
            TrayIconAction::Ignore => {}
        })
        .build(app)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri::{tray::TrayIconId, Position, Size};

    fn click(button: MouseButton, button_state: MouseButtonState) -> TrayIconEvent {
        TrayIconEvent::Click {
            id: TrayIconId::new("main"),
            position: PhysicalPosition::new(0.0, 0.0),
            rect: Rect {
                position: Position::Physical(PhysicalPosition::new(0, 0)),
                size: Size::Physical(PhysicalSize::new(22, 22)),
            },
            button,
            button_state,
        }
    }

    #[test]
    fn left_mouse_up_toggles_status_not_main_window() {
        assert_eq!(
            tray_icon_action(&click(MouseButton::Left, MouseButtonState::Up)),
            TrayIconAction::TogglePopover
        );
    }

    #[test]
    fn mouse_down_does_not_toggle_status() {
        assert_eq!(
            tray_icon_action(&click(MouseButton::Left, MouseButtonState::Down)),
            TrayIconAction::Ignore
        );
    }

    #[test]
    fn only_right_mouse_up_opens_the_context_menu() {
        assert_eq!(
            tray_icon_action(&click(MouseButton::Right, MouseButtonState::Down)),
            TrayIconAction::Ignore
        );
        assert_eq!(
            tray_icon_action(&click(MouseButton::Right, MouseButtonState::Up)),
            TrayIconAction::ShowContextMenu
        );
    }

    #[test]
    fn top_menu_anchors_below_icon_and_clamps_right_edge() {
        assert_eq!(
            popover_position(
                (1400.0, 0.0, 22.0, 24.0),
                (0.0, 24.0, 1440.0, 850.0),
                (460.0, 600.0),
                6.0
            ),
            PhysicalPosition::new(980, 30)
        );
    }

    #[test]
    fn bottom_taskbar_anchors_above_icon() {
        assert_eq!(
            popover_position(
                (1000.0, 1040.0, 24.0, 40.0),
                (0.0, 0.0, 1920.0, 1040.0),
                (460.0, 600.0),
                6.0
            ),
            PhysicalPosition::new(782, 434)
        );
    }

    #[test]
    fn negative_origin_and_hidpi_use_physical_coordinates() {
        assert_eq!(
            popover_position(
                (-2800.0, 0.0, 44.0, 48.0),
                (-2880.0, 48.0, 2880.0, 1752.0),
                (920.0, 1200.0),
                12.0
            ),
            PhysicalPosition::new(-2880, 60)
        );
    }

    #[test]
    fn small_screen_never_panics_or_places_origin_outside_work_area() {
        assert_eq!(
            popover_position(
                (100.0, 0.0, 20.0, 24.0),
                (0.0, 24.0, 320.0, 400.0),
                (460.0, 600.0),
                6.0
            ),
            PhysicalPosition::new(0, 24)
        );
    }
}
