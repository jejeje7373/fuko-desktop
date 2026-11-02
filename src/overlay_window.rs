// src/overlay_window.rs
// Ventana overlay y ventana de dialogo estilo MtGhozt/Capri-Desktop:
// - overlay_window: avatar transparente always-on-top con soporte para flotar y sentarse en ventanas
// - bubble_window: bloque de texto cyberpunk separado (bubble.html) posicionado junto al avatar

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicI32, Ordering};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, Position, Runtime, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};
use tokio::process::Command;

pub const WINDOW_NAME: &str = "overlay_window";
pub const OVERLAY_WIDTH: i32 = 360;
pub const OVERLAY_HEIGHT: i32 = 420;

pub const BUBBLE_WINDOW_NAME: &str = "bubble_window";
pub const BUBBLE_WIDTH: i32 = 460;
pub const BUBBLE_HEIGHT: i32 = 285;

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
    pub maximized: bool,
    pub focused: bool,
}

/// Obtiene las dimensiones reales de pantalla y area de trabajo
pub fn query_geometry<R: Runtime>(app_handle: &AppHandle<R>) -> DesktopGeometry {
    let mut screen_width = 1920;
    let mut screen_height = 1080;

    if let Ok(Some(monitor)) = app_handle.primary_monitor() {
        let size = monitor.size();
        let scale = monitor.scale_factor().max(1.0);
        if size.width > 0 && size.height > 0 {
            screen_width = (size.width as f64 / scale).round() as i32;
            screen_height = (size.height as f64 / scale).round() as i32;
        }
    }

    let top_bar = 32;
    let bottom_dock = 36;
    let work_x = 0;
    let work_y = top_bar;
    let work_width = screen_width;
    let work_height = (screen_height - top_bar - bottom_dock).max(480);
    let floor_y = (screen_height - OVERLAY_HEIGHT - bottom_dock).max(work_y);

    let default_x = (screen_width as f64 * 0.68) as i32;
    let default_y = (screen_height as f64 * 0.25) as i32;

    let saved_x = LAST_WIN_X.load(Ordering::Relaxed);
    let saved_y = LAST_WIN_Y.load(Ordering::Relaxed);

    let (mut win_x, mut win_y) = if saved_x >= 0 && saved_y >= 0 {
        (saved_x, saved_y)
    } else {
        (default_x, default_y)
    };

    if let Some(window) = app_handle.get_webview_window(WINDOW_NAME) {
        if let Ok(pos) = window.outer_position() {
            if pos.x != 0 || pos.y != 0 {
                win_x = pos.x;
                win_y = pos.y;
                LAST_WIN_X.store(win_x, Ordering::Relaxed);
                LAST_WIN_Y.store(win_y, Ordering::Relaxed);
            }
        }
    }

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

/// Obtiene las ventanas abiertas en tiempo real (equivale a windows.js + rects.js de MtGhozt/Capri-Desktop)
pub async fn fetch_window_ledges() -> Vec<WindowLedge> {
    // 1. Extensión GNOME Shell Wayland (org.openclaw.ClaudiaPerch)
    if let Ok(out) = Command::new("gdbus")
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
    {
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
                    let t_low = title.to_lowercase();
                    let c_low = wm_class.to_lowercase();
                    if t_low == "fuko"
                        || t_low.contains("fuko dialogue")
                        || t_low.contains("capri dialogue")
                        || t_low.contains("fuko mainframe")
                        || t_low.contains("claudia")
                        || c_low.contains("fuko")
                        || c_low.contains("claudia")
                        || c_low == "gjs"
                    {
                        continue;
                    }
                    let x = item.get("x").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                    let y = item.get("y").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                    let width = item.get("width").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                    let height = item.get("height").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                    if width < 140 || height < 80 {
                        continue;
                    }
                    let maximized = item
                        .get("maximized")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let focused = item
                        .get("focused")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(idx == 0);
                    let id = item
                        .get("id")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| format!("wl_{}_{}", wm_class, idx));

                    ledges.push(WindowLedge {
                        id,
                        title,
                        wm_class,
                        x,
                        y,
                        width,
                        height,
                        maximized,
                        focused,
                    });
                }
                if !ledges.is_empty() {
                    return ledges;
                }
            }
        }
    }

    // 2. Fallback X11 / XWayland usando wmctrl -lG y xprop
    let active_win_id = if let Ok(act_out) = Command::new("xprop")
        .args(["-root", "_NET_ACTIVE_WINDOW"])
        .output()
        .await
    {
        let s = String::from_utf8_lossy(&act_out.stdout);
        s.split_whitespace()
            .last()
            .unwrap_or("")
            .trim()
            .trim_start_matches("0x")
            .to_lowercase()
    } else {
        String::new()
    };

    if let Ok(out) = Command::new("wmctrl").args(["-lG"]).output().await {
        let stdout = String::from_utf8_lossy(&out.stdout);
        let mut ledges = Vec::new();
        for line in stdout.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 8 {
                continue;
            }
            let win_id = parts[0];
            let desktop = parts[1];
            if desktop == "-1" {
                continue;
            }
            let x: i32 = parts[2].parse().unwrap_or(0);
            let y: i32 = parts[3].parse().unwrap_or(0);
            let width: i32 = parts[4].parse().unwrap_or(0);
            let height: i32 = parts[5].parse().unwrap_or(0);
            let title = parts[7..].join(" ");
            let t_low = title.to_lowercase();
            if width < 140
                || height < 80
                || t_low == "fuko"
                || t_low.contains("fuko dialogue")
                || t_low.contains("capri dialogue")
                || t_low.contains("fuko mainframe")
                || t_low.contains("claudia")
                || t_low.contains("desktop")
            {
                continue;
            }
            let norm_id = win_id.trim_start_matches("0x").trim_start_matches('0').to_lowercase();
            let norm_act = active_win_id.trim_start_matches('0').to_lowercase();
            let focused = !norm_act.is_empty() && norm_id == norm_act;

            ledges.push(WindowLedge {
                id: win_id.to_string(),
                title,
                wm_class: "x11".to_string(),
                x,
                y,
                width,
                height,
                maximized: width >= 1800 && height >= 950,
                focused,
            });
        }
        ledges.sort_by(|a, b| b.focused.cmp(&a.focused));
        return ledges;
    }

    Vec::new()
}

/// Configura tanto overlay_window (avatar) como bubble_window (bloque de texto estilo Capri-Desktop)
pub fn setup_window<R: tauri::Runtime>(app_handle: &AppHandle<R>) -> Result<(), String> {
    let geom = query_geometry(app_handle);

    let start_x = (geom.screen_width as f64 * 0.68) as i32;
    let start_y = (geom.screen_height as f64 * 0.25) as i32;
    LAST_WIN_X.store(start_x, Ordering::Relaxed);
    LAST_WIN_Y.store(start_y, Ordering::Relaxed);

    let window = if let Some(existing) = app_handle.get_webview_window(WINDOW_NAME) {
        let _ = existing.set_size(tauri::Size::Physical(tauri::PhysicalSize {
            width: OVERLAY_WIDTH as u32,
            height: OVERLAY_HEIGHT as u32,
        }));
        let _ = existing.set_position(Position::Physical(PhysicalPosition {
            x: start_x,
            y: start_y,
        }));
        let _ = existing.set_always_on_top(true);
        let _ = existing.set_skip_taskbar(true);
        let _ = existing.show();
        existing
    } else {
        WebviewWindowBuilder::new(app_handle, WINDOW_NAME, WebviewUrl::App("index.html".into()))
            .title("Fuko")
            .visible(true)
            .fullscreen(false)
            .resizable(false)
            .decorations(false)
            .transparent(true)
            .skip_taskbar(true)
            .always_on_top(true)
            .inner_size(OVERLAY_WIDTH as f64, OVERLAY_HEIGHT as f64)
            .position(start_x as f64, start_y as f64)
            .build()
            .map_err(|e| format!("Failed to build overlay window: {}", e))?
    };

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
        .build();
    }

    let window_clone = window.clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::Focused(focused) = event {
            if !focused {
                let _ = window_clone.emit("window_blurred", true);
            }
        }
    });

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

    // Colocar el bloque de texto a la izquierda de Capri; si no cabe, a su derecha
    let mut bubble_x = av_x - BUBBLE_WIDTH + 20;
    if bubble_x < 12 {
        bubble_x = (av_x + OVERLAY_WIDTH - 20).min(geom.screen_width - BUBBLE_WIDTH - 12);
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

#[tauri::command]
pub async fn get_desktop_geometry_cmd(
    app_handle: AppHandle<tauri::Wry>,
) -> Result<DesktopGeometry, String> {
    Ok(query_geometry(&app_handle))
}

#[tauri::command]
pub async fn get_window_ledges_cmd() -> Result<Vec<WindowLedge>, String> {
    Ok(fetch_window_ledges().await)
}

#[tauri::command]
pub async fn get_active_window_cmd() -> Result<Option<WindowLedge>, String> {
    let ledges = fetch_window_ledges().await;
    Ok(ledges.iter().find(|w| w.focused).cloned().or_else(|| ledges.into_iter().next()))
}

#[tauri::command]
pub async fn move_overlay_window_cmd(
    x: i32,
    y: i32,
    app_handle: AppHandle<tauri::Wry>,
) -> Result<(), String> {
    let geom = query_geometry(&app_handle);
    let clamped_x = x.clamp(-60, geom.screen_width - OVERLAY_WIDTH + 60);
    let clamped_y = y.clamp(0, geom.floor_y);
    LAST_WIN_X.store(clamped_x, Ordering::Relaxed);
    LAST_WIN_Y.store(clamped_y, Ordering::Relaxed);
    if let Some(window) = app_handle.get_webview_window(WINDOW_NAME) {
        window
            .set_position(Position::Physical(PhysicalPosition {
                x: clamped_x,
                y: clamped_y,
            }))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
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
pub async fn open_mainframe_cmd(app_handle: AppHandle<tauri::Wry>) -> Result<(), String> {
    open_mainframe(&app_handle)
}

#[tauri::command]
pub async fn quit_app_cmd(app_handle: AppHandle<tauri::Wry>) -> Result<(), String> {
    app_handle.exit(0);
    Ok(())
}
