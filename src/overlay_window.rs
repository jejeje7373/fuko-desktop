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
    pub is_maximized: bool,
    pub is_focused: bool,
}

pub fn query_geometry<R: Runtime>(app_handle: &AppHandle<R>) -> DesktopGeometry {
    let mut geom = DesktopGeometry {
        screen_width: OVERLAY_WIDTH,
        screen_height: OVERLAY_HEIGHT,
        work_x: 0,
        work_y: 0,
        work_width: OVERLAY_WIDTH,
        work_height: OVERLAY_HEIGHT,
        win_x: LAST_WIN_X.load(Ordering::Relaxed),
        win_y: LAST_WIN_Y.load(Ordering::Relaxed),
        win_width: OVERLAY_WIDTH,
        win_height: OVERLAY_HEIGHT,
        floor_y: OVERLAY_HEIGHT,
    };

    if let Some(window) = app_handle.get_webview_window(WINDOW_NAME) {
        if let Ok(Some(monitor)) = window.current_monitor() {
            let size = monitor.size();
            let position = monitor.position();
            geom.screen_width = size.width as i32;
            geom.screen_height = size.height as i32;
            geom.work_x = position.x;
            geom.work_y = position.y;
            geom.work_width = size.width as i32;
            geom.work_height = size.height as i32;
            geom.floor_y = size.height as i32;
        }
    }
    geom
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
pub async fn move_overlay_window_cmd(
    x: i32,
    y: i32,
    app_handle: AppHandle<tauri::Wry>,
) -> Result<(), String> {
    LAST_WIN_X.store(x, Ordering::Relaxed);
    LAST_WIN_Y.store(y, Ordering::Relaxed);
    if let Some(bubble) = app_handle.get_webview_window(BUBBLE_WINDOW_NAME) {
        let _ = bubble.set_position(Position::Physical(PhysicalPosition {
            x: x + 20,
            y: y + 20,
        }));
    }
    Ok(())
}

#[tauri::command]
pub async fn get_window_ledges_cmd(
    _app_handle: AppHandle<tauri::Wry>,
) -> Result<Vec<WindowLedge>, String> {
    Ok(fetch_window_ledges().await)
}

pub async fn fetch_window_ledges() -> Vec<WindowLedge> {
    if let Ok(ledges) = fetch_hyprland_ledges().await {
        if !ledges.is_empty() { return ledges; }
    }
    if let Ok(ledges) = fetch_sway_ledges().await {
        if !ledges.is_empty() { return ledges; }
    }
    fetch_x11_ledges().await.unwrap_or_default()
}

async fn fetch_hyprland_ledges() -> Result<Vec<WindowLedge>, String> {
    let output = Command::new("hyprctl")
        .args(["clients", "-j"])
        .output()
        .await
        .map_err(|e| e.to_string())?;
    if !output.status.success() { return Err("hyprctl failed".into()); }
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())?;
    let mut ledges = Vec::new();
    if let Some(arr) = json.as_array() {
        for w in arr {
            let mapped = w.get("mapped").and_then(|v| v.as_bool()).unwrap_or(true);
            let hidden = w.get("hidden").and_then(|v| v.as_bool()).unwrap_or(false);
            if !mapped || hidden { continue; }
            let at = match w.get("at").and_then(|v| v.as_array()) { Some(a) => a, None => continue };
            let size = match w.get("size").and_then(|v| v.as_array()) { Some(a) => a, None => continue };
            let x = at.get(0).and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let y = at.get(1).and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let width = size.get(0).and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let height = size.get(1).and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            if width <= 100 || height <= 100 { continue; }
            ledges.push(WindowLedge {
                id: w.get("address").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                title: w.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                wm_class: w.get("class").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                x, y, width, height, top_ledge_y: y, left_x: x, right_x: x + width,
                is_maximized: false, is_focused: w.get("focusHistoryID").and_then(|v| v.as_i64()).unwrap_or(-1) == 0,
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
    if !output.status.success() { return Err("swaymsg failed".into()); }
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())?;
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
                id, title, wm_class: app_id, x, y, width, height,
                top_ledge_y: y, left_x: x, right_x: x + width,
                is_maximized: false, is_focused: focused,
            });
        }
    }
    if let Some(nodes) = node.get("nodes").and_then(|v| v.as_array()) {
        for child in nodes { parse_sway_node(child, ledges); }
    }
}

async fn fetch_x11_ledges() -> Result<Vec<WindowLedge>, String> {
    let output = Command::new("wmctrl")
        .args(["-lG"])
        .output()
        .await
        .map_err(|e| e.to_string())?;
    if !output.status.success() { return Err("wmctrl failed".into()); }
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
                    id: parts[0].to_string(), title, wm_class: String::new(),
                    x, y, width, height, top_ledge_y: y, left_x: x, right_x: x + width,
                    is_maximized: false, is_focused: false,
                });
            }
        }
    }
    Ok(ledges)
}
