use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicI32, Ordering};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, Position, Runtime, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};
use tokio::process::Command;

pub const WINDOW_NAME: &str = "overlay_window";
pub const OVERLAY_WIDTH: i32 = 1920;
pub const OVERLAY_HEIGHT: i32 = 1080;

pub const BUBBLE_WINDOW_NAME: &str = "bubble_window";
pub const BUBBLE_WIDTH: i32 = 440;
pub const BUBBLE_HEIGHT: i32 = 260;

static LAST_WIN_X: AtomicI32 = AtomicI32::new(-1);
static LAST_WIN_Y: AtomicI32 = AtomicI32::new(-1);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesktopGeometry {
    pub screen_width: i32,
    pub screen_height: i32,
    pub work_x: i32,
    pub work_y: i32,
    pub work_width: i32,
    pub work_height: i32,
    pub win_x: i32,
    pub win_y: i32,
    pub win_width: i32,
    pub win_height: i32,
    pub floor_y: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowLedge {
    pub id: String,
    pub title: String,
    pub wm_class: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub top_ledge_y: i32,
    pub left_x: i32,
    pub right_x: i32,
    pub maximized: bool,
    pub focused: bool,
    #[serde(default)]
    pub is_maximized: bool,
    #[serde(default)]
    pub is_focused: bool,
}

pub fn query_geometry<R: Runtime>(app_handle: &AppHandle<R>) -> DesktopGeometry {
    let mut screen_width = OVERLAY_WIDTH;
    let mut screen_height = OVERLAY_HEIGHT;

    if let Ok(Some(monitor)) = app_handle.primary_monitor() {
        let size = monitor.size();
        let scale = monitor.scale_factor().max(1.0);
        if size.width > 0 && size.height > 0 {
            screen_width = (size.width as f64 / scale).round() as i32;
            screen_height = (size.height as f64 / scale).round() as i32;
        }
    } else if let Some(window) = app_handle.get_webview_window(WINDOW_NAME) {
        if let Ok(Some(monitor)) = window.current_monitor() {
            let size = monitor.size();
            let scale = monitor.scale_factor().max(1.0);
            if size.width > 0 && size.height > 0 {
                screen_width = (size.width as f64 / scale).round() as i32;
                screen_height = (size.height as f64 / scale).round() as i32;
            }
        }
    }

    let top_bar = 32;
    let bottom_dock = 36;
    let work_x = 0;
    let work_y = top_bar;
    let work_width = screen_width;
    let work_height = (screen_height - top_bar - bottom_dock).max(480);
    let floor_y = (screen_height - 248 - bottom_dock).max(work_y);

    let default_x = (screen_width as f64 * 0.68) as i32;
    let default_y = (screen_height as f64 * 0.25) as i32;

    let saved_x = LAST_WIN_X.load(Ordering::Relaxed);
    let saved_y = LAST_WIN_Y.load(Ordering::Relaxed);

    let (win_x, win_y) = if saved_x >= 0 && saved_y >= 0 {
        (saved_x, saved_y)
    } else {
        (default_x, default_y)
    };

    DesktopGeometry {
        screen_width,
        screen_height,
        work_x,
        work_y,
        work_width,
        work_height,
        win_x,
        win_y,
        win_width: OVERLAY_WIDTH,
        win_height: OVERLAY_HEIGHT,
        floor_y,
    }
}

pub fn setup_window<R: Runtime>(app_handle: &AppHandle<R>) -> Result<(), String> {
    let geom = query_geometry(app_handle);

    let start_x = (geom.screen_width as f64 * 0.68) as i32;
    let start_y = (geom.screen_height as f64 * 0.25) as i32;
    if LAST_WIN_X.load(Ordering::Relaxed) < 0 {
        LAST_WIN_X.store(start_x, Ordering::Relaxed);
        LAST_WIN_Y.store(start_y, Ordering::Relaxed);
    }

    if let Some(existing) = app_handle.get_webview_window(WINDOW_NAME) {
        let _ = existing.set_position(Position::Physical(PhysicalPosition { x: 0, y: 0 }));
        let _ = existing.set_size(tauri::Size::Physical(tauri::PhysicalSize {
            width: geom.screen_width as u32,
            height: geom.screen_height as u32,
        }));
        let _ = existing.set_always_on_top(true);
        let _ = existing.set_skip_taskbar(true);
        let _ = existing.show();
    } else {
        let _ = WebviewWindowBuilder::new(
            app_handle,
            WINDOW_NAME,
            WebviewUrl::App("index.html".into()),
        )
        .title("Fuko")
        .visible(true)
        .fullscreen(false)
        .resizable(true)
        .decorations(false)
        .transparent(true)
        .skip_taskbar(true)
        .always_on_top(true)
        .inner_size(geom.screen_width as f64, geom.screen_height as f64)
        .position(0.0, 0.0)
        .build()
        .map_err(|e| format!("Failed to build overlay window: {}", e))?;
    }

    let bubble_start_x = (start_x - BUBBLE_WIDTH - 16).max(16);
    let bubble_start_y = (start_y + 40).clamp(36, (geom.screen_height - BUBBLE_HEIGHT - 40).max(36));

    if let Some(existing_bubble) = app_handle.get_webview_window(BUBBLE_WINDOW_NAME) {
        let _ = existing_bubble.set_size(tauri::Size::Physical(tauri::PhysicalSize {
            width: BUBBLE_WIDTH as u32,
            height: BUBBLE_HEIGHT as u32,
        }));
        let _ = existing_bubble.set_position(Position::Physical(PhysicalPosition {
            x: bubble_start_x,
            y: bubble_start_y,
        }));
        let _ = existing_bubble.set_always_on_top(true);
        let _ = existing_bubble.set_skip_taskbar(true);
    } else {
        let _ = WebviewWindowBuilder::new(
            app_handle,
            BUBBLE_WINDOW_NAME,
            WebviewUrl::App("bubble.html".into()),
        )
        .title("Fuko Dialogue")
        .visible(false)
        .resizable(false)
        .decorations(false)
        .transparent(true)
        .skip_taskbar(true)
        .always_on_top(true)
        .inner_size(BUBBLE_WIDTH as f64, BUBBLE_HEIGHT as f64)
        .position(bubble_start_x as f64, bubble_start_y as f64)
        .build()
        .map_err(|e| format!("Failed to build bubble window: {}", e))?;
    }

    Ok(())
}

pub fn toggle_window<R: Runtime>(window: &WebviewWindow<R>, show: bool) -> Result<(), String> {
    if show {
        window.show().map_err(|e| e.to_string())?;
        window.emit("window_shown", true).map_err(|e| e.to_string())?;
    } else {
        window.hide().map_err(|e| e.to_string())?;
        window.emit("window_hidden", false).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn position_bubble_near_avatar<R: Runtime>(app_handle: &AppHandle<R>) -> Result<(), String> {
    let geom = query_geometry(app_handle);
    let av_x = LAST_WIN_X.load(Ordering::Relaxed).max(0);
    let av_y = LAST_WIN_Y.load(Ordering::Relaxed).max(0);

    let mut bubble_x = av_x - BUBBLE_WIDTH + 20;
    if bubble_x < 12 {
        bubble_x = (av_x + 240).min(geom.screen_width - BUBBLE_WIDTH - 12);
    }
    let bubble_y = (av_y + 30).clamp(36, (geom.screen_height - BUBBLE_HEIGHT - 36).max(36));

    if let Some(bubble) = app_handle.get_webview_window(BUBBLE_WINDOW_NAME) {
        let _ = bubble.set_position(Position::Physical(PhysicalPosition {
            x: bubble_x,
            y: bubble_y,
        }));
    }
    Ok(())
}

pub fn open_mainframe<R: Runtime>(app_handle: &AppHandle<R>) -> Result<(), String> {
    if let Some(window) = app_handle.get_webview_window("mainframe") {
        window.show().map_err(|e| e.to_string())?;
        window.set_focus().map_err(|e| e.to_string())?;
    } else {
        let win = WebviewWindowBuilder::new(
            app_handle,
            "mainframe",
            WebviewUrl::App("mainframe.html".into()),
        )
        .title("Fuko Mainframe")
        .inner_size(920.0, 720.0)
        .min_inner_size(700.0, 500.0)
        .center()
        .resizable(true)
        .decorations(true)
        .visible(true)
        .build()
        .map_err(|e| e.to_string())?;
        let _ = win.set_focus();
    }
    Ok(())
}

#[tauri::command]
pub async fn set_ignore_cursor_events_cmd(
    ignore: bool,
    app_handle: AppHandle<tauri::Wry>,
) -> Result<(), String> {
    if let Some(window) = app_handle.get_webview_window(WINDOW_NAME) {
        window.set_ignore_cursor_events(ignore).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn get_screen_bounds_cmd(
    app_handle: AppHandle<tauri::Wry>,
) -> Result<DesktopGeometry, String> {
    Ok(query_geometry(&app_handle))
}

#[tauri::command]
pub async fn get_desktop_geometry_cmd(
    app_handle: AppHandle<tauri::Wry>,
) -> Result<DesktopGeometry, String> {
    Ok(query_geometry(&app_handle))
}

#[tauri::command]
pub async fn move_overlay_window_cmd(
    x: i32,
    y: i32,
    app_handle: AppHandle<tauri::Wry>,
) -> Result<(), String> {
    let geom = query_geometry(&app_handle);
    let clamped_x = x.clamp(-50, geom.screen_width + 50);
    let clamped_y = y.clamp(0, geom.screen_height);
    LAST_WIN_X.store(clamped_x, Ordering::Relaxed);
    LAST_WIN_Y.store(clamped_y, Ordering::Relaxed);
    let _ = position_bubble_near_avatar(&app_handle);
    Ok(())
}

#[tauri::command]
pub async fn get_window_ledges_cmd() -> Result<Vec<WindowLedge>, String> {
    Ok(fetch_window_ledges().await)
}

#[tauri::command]
pub async fn get_active_window_cmd() -> Result<Option<WindowLedge>, String> {
    let ledges = fetch_window_ledges().await;
    Ok(ledges
        .iter()
        .find(|w| w.focused || w.is_focused)
        .cloned()
        .or_else(|| ledges.into_iter().next()))
}

#[tauri::command]
pub async fn show_bubble_window_cmd(
    show: bool,
    focus: Option<bool>,
    app_handle: AppHandle<tauri::Wry>,
) -> Result<(), String> {
    if show {
        let _ = position_bubble_near_avatar(&app_handle);
    }
    if let Some(bubble) = app_handle.get_webview_window(BUBBLE_WINDOW_NAME) {
        if show {
            let _ = bubble.show();
            let _ = bubble.set_always_on_top(true);
            if focus.unwrap_or(false) {
                let _ = bubble.set_focus();
                let _ = app_handle.emit("focus_bubble_input", true);
            }
        } else {
            let _ = bubble.hide();
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn position_bubble_near_avatar_cmd(
    app_handle: AppHandle<tauri::Wry>,
) -> Result<(), String> {
    position_bubble_near_avatar(&app_handle)
}

#[tauri::command]
pub async fn move_bubble_window_cmd(
    x: i32,
    y: i32,
    app_handle: AppHandle<tauri::Wry>,
) -> Result<(), String> {
    if let Some(bubble) = app_handle.get_webview_window(BUBBLE_WINDOW_NAME) {
        bubble
            .set_position(Position::Physical(PhysicalPosition { x, y }))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn open_mainframe_cmd(app_handle: AppHandle<tauri::Wry>) -> Result<(), String> {
    open_mainframe(&app_handle)
}

#[tauri::command]
pub async fn quit_app_cmd(app_handle: AppHandle<tauri::Wry>) -> Result<(), String> {
    app_handle.exit(0);
    Ok(())
}

pub async fn fetch_window_ledges() -> Vec<WindowLedge> {
    if let Ok(ledges) = fetch_gnome_shell_ledges().await {
        if !ledges.is_empty() {
            return ledges;
        }
    }
    if let Ok(ledges) = fetch_hyprland_ledges().await {
        if !ledges.is_empty() {
            return ledges;
        }
    }
    if let Ok(ledges) = fetch_sway_ledges().await {
        if !ledges.is_empty() {
            return ledges;
        }
    }
    fetch_x11_ledges().await.unwrap_or_default()
}

async fn fetch_gnome_shell_ledges() -> Result<Vec<WindowLedge>, String> {
    let out = Command::new("gdbus")
        .args([
            "call",
            "--session",
            "--dest",
            "org.openclaw.ClaudiaPerch",
            "--object-path",
            "/org/openclaw/ClaudiaPerch",
            "--method",
            "org.openclaw.ClaudiaPerch.GetLedges",
        ])
        .output()
        .await
        .map_err(|e| e.to_string())?;

    if !out.status.success() {
        return Err("gdbus failed".into());
    }

    let raw = String::from_utf8_lossy(&out.stdout);
    if let (Some(start), Some(end)) = (raw.find('['), raw.rfind(']')) {
        let slice = &raw[start..=end];
        let unescaped = slice.replace("\\\"", "\"");
        if let Ok(items) = serde_json::from_str::<Vec<serde_json::Value>>(&unescaped) {
            let mut ledges = Vec::new();
            for (idx, item) in items.iter().enumerate() {
                let title = item
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let wm_class = item
                    .get("wm_class")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let x = item.get("x").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                let y = item.get("y").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                let width = item.get("width").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                let height = item.get("height").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                let maximized = item
                    .get("maximized")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let focused = item.get("focused").and_then(|v| v.as_bool()).unwrap_or(false);
                if width > 100 && height > 100 {
                    ledges.push(WindowLedge {
                        id: format!("gnome_{}", idx),
                        title,
                        wm_class,
                        x,
                        y,
                        width,
                        height,
                        top_ledge_y: y,
                        left_x: x,
                        right_x: x + width,
                        maximized,
                        focused,
                        is_maximized: maximized,
                        is_focused: focused,
                    });
                }
            }
            if !ledges.is_empty() {
                return Ok(ledges);
            }
        }
    }
    Err("no gnome ledges".into())
}

async fn fetch_hyprland_ledges() -> Result<Vec<WindowLedge>, String> {
    let output = Command::new("hyprctl")
        .args(["clients", "-j"])
        .output()
        .await
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err("hyprctl failed".into());
    }
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())?;
    let mut ledges = Vec::new();
    if let Some(arr) = json.as_array() {
        for w in arr {
            let mapped = w.get("mapped").and_then(|v| v.as_bool()).unwrap_or(true);
            let hidden = w.get("hidden").and_then(|v| v.as_bool()).unwrap_or(false);
            if !mapped || hidden {
                continue;
            }
            let at = match w.get("at").and_then(|v| v.as_array()) {
                Some(a) => a,
                None => continue,
            };
            let size = match w.get("size").and_then(|v| v.as_array()) {
                Some(a) => a,
                None => continue,
            };
            let x = at.get(0).and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let y = at.get(1).and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let width = size.get(0).and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let height = size.get(1).and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            if width <= 100 || height <= 100 {
                continue;
            }
            let focused = w.get("focusHistoryID").and_then(|v| v.as_i64()).unwrap_or(-1) == 0;
            ledges.push(WindowLedge {
                id: w.get("address").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                title: w.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                wm_class: w.get("class").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                x,
                y,
                width,
                height,
                top_ledge_y: y,
                left_x: x,
                right_x: x + width,
                maximized: false,
                focused,
                is_maximized: false,
                is_focused: focused,
            });
        }
    }
    Ok(ledges)
}

async fn fetch_sway_ledges() -> Result<Vec<WindowLedge>, String> {
    let output = Command::new("swaymsg")
        .args(["-t", "get_tree"])
        .output()
        .await
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err("swaymsg failed".into());
    }
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())?;
    let mut ledges = Vec::new();
    parse_sway_node(&json, &mut ledges);
    Ok(ledges)
}

fn parse_sway_node(node: &serde_json::Value, ledges: &mut Vec<WindowLedge>) {
    if let Some(rect) = node.get("rect") {
        let x = rect.get("x").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
        let y = rect.get("y").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
        let width = rect.get("width").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
        let height = rect.get("height").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
        if width > 100 && height > 100 {
            let title = node.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let app_id = node.get("app_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let id = node.get("id").map(|v| v.to_string()).unwrap_or_default();
            let focused = node.get("focused").and_then(|v| v.as_bool()).unwrap_or(false);
            ledges.push(WindowLedge {
                id,
                title,
                wm_class: app_id,
                x,
                y,
                width,
                height,
                top_ledge_y: y,
                left_x: x,
                right_x: x + width,
                maximized: false,
                focused,
                is_maximized: false,
                is_focused: focused,
            });
        }
    }
    if let Some(nodes) = node.get("nodes").and_then(|v| v.as_array()) {
        for child in nodes {
            parse_sway_node(child, ledges);
        }
    }
}

async fn fetch_x11_ledges() -> Result<Vec<WindowLedge>, String> {
    let output = Command::new("wmctrl")
        .args(["-lG"])
        .output()
        .await
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err("wmctrl failed".into());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut ledges = Vec::new();
    for line in stdout.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 7 {
            let x = parts[2].parse::<i32>().unwrap_or(0);
            let y = parts[3].parse::<i32>().unwrap_or(0);
            let width = parts[4].parse::<i32>().unwrap_or(0);
            let height = parts[5].parse::<i32>().unwrap_or(0);
            let title = parts[7..].join(" ");
            if width > 100 && height > 100 {
                ledges.push(WindowLedge {
                    id: parts[0].to_string(),
                    title,
                    wm_class: String::new(),
                    x,
                    y,
                    width,
                    height,
                    top_ledge_y: y,
                    left_x: x,
                    right_x: x + width,
                    maximized: false,
                    focused: false,
                    is_maximized: false,
                    is_focused: false,
                });
            }
        }
    }
    Ok(ledges)
}
