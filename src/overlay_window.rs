// src/overlay_window.rs
// Ventana overlay estilo Capri: transparente, always-on-top, sin bordes y con físicas de escritorio

use serde::{Deserialize, Serialize};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, Position, Runtime, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

pub const WINDOW_NAME: &str = "overlay_window";
pub const OVERLAY_WIDTH: i32 = 360;
pub const OVERLAY_HEIGHT: i32 = 420;

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

/// Obtiene las dimensiones de pantalla y área de trabajo
pub fn query_geometry<R: Runtime>(app_handle: &AppHandle<R>) -> DesktopGeometry {
    let mut screen_width = 1920;
    let mut screen_height = 1080;

    if let Ok(Some(monitor)) = app_handle.primary_monitor() {
        let size = monitor.size();
        if size.width > 0 && size.height > 0 {
            screen_width = size.width as i32;
            screen_height = size.height as i32;
        }
    }

    // Margen para barra superior e inferior de Ubuntu GNOME
    let work_x = 0;
    let work_y = 32;
    let work_width = screen_width;
    let work_height = screen_height - 32;
    let floor_y = (screen_height - OVERLAY_HEIGHT).max(work_y);

    let (mut win_x, mut win_y) = (
        screen_width - OVERLAY_WIDTH - 80,
        floor_y,
    );

    if let Some(window) = app_handle.get_webview_window(WINDOW_NAME) {
        if let Ok(pos) = window.outer_position() {
            win_x = pos.x;
            win_y = pos.y;
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

/// Configura la ventana overlay principal al iniciar (empieza arriba para caer con gravedad estilo Capri)
pub fn setup_window<R: tauri::Runtime>(app_handle: &AppHandle<R>) -> Result<(), String> {
    let geom = query_geometry(app_handle);

    // En Capri Desktop, al iniciar aparece arriba/centro-derecha y cae con gravedad hasta el suelo (floor_y)
    let start_x = (geom.screen_width as f64 * 0.68) as i32;
    let start_y = (geom.screen_height as f64 * 0.18) as i32;

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
        let builder =
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
                .position(start_x as f64, start_y as f64);

        builder
            .build()
            .map_err(|e| format!("Failed to build window: {}", e))?
    };

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

/// Alterna visibilidad de la ventana overlay
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

#[tauri::command]
pub async fn get_desktop_geometry_cmd(
    app_handle: AppHandle<tauri::Wry>,
) -> Result<DesktopGeometry, String> {
    let g = query_geometry(&app_handle);
    println!(
        "[Claudia] Frontend conectado: screen={}x{}, pos=({}, {}), floor_y={}",
        g.screen_width, g.screen_height, g.win_x, g.win_y, g.floor_y
    );
    Ok(g)
}

#[tauri::command]
pub async fn move_overlay_window_cmd(
    x: i32,
    y: i32,
    app_handle: AppHandle<tauri::Wry>,
) -> Result<(), String> {
    if let Some(window) = app_handle.get_webview_window(WINDOW_NAME) {
        window
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowLedge {
    pub title: String,
    pub wm_class: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    #[serde(default)]
    pub maximized: bool,
}

/// Obtiene la lista de repisas/bordes de ventanas abiertas (Wayland vía ClaudiaPerch)
#[tauri::command]
pub async fn get_window_ledges_cmd() -> Result<Vec<WindowLedge>, String> {
    let out = tokio::process::Command::new("gdbus")
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

    let raw = String::from_utf8_lossy(&out.stdout);
    if let (Some(start), Some(end)) = (raw.find('['), raw.rfind(']')) {
        let slice = &raw[start..=end];
        let unescaped = slice.replace("\\\"", "\"");
        if let Ok(ledges) = serde_json::from_str::<Vec<WindowLedge>>(&unescaped) {
            let filtered: Vec<WindowLedge> = ledges
                .into_iter()
                .filter(|l| {
                    let w_lower = l.wm_class.to_lowercase();
                    let t_lower = l.title.to_lowercase();
                    w_lower != "gjs"
                        && !w_lower.contains("claudia")
                        && !w_lower.contains("fuko")
                        && !t_lower.contains("desktop icons")
                        && l.width > 200
                        && l.height > 100
                })
                .collect();
            return Ok(filtered);
        }
    }
    Ok(vec![])
}

/// Obtiene la ventana actualmente activa/enfocada
#[tauri::command]
pub async fn get_active_window_cmd() -> Result<Option<WindowLedge>, String> {
    let out = tokio::process::Command::new("gdbus")
        .args([
            "call",
            "--session",
            "--dest",
            "org.openclaw.ClaudiaPerch",
            "--object-path",
            "/org/openclaw/ClaudiaPerch",
            "--method",
            "org.openclaw.ClaudiaPerch.GetActiveWindow",
        ])
        .output()
        .await
        .map_err(|e| e.to_string())?;

    let raw = String::from_utf8_lossy(&out.stdout);
    if let (Some(start), Some(end)) = (raw.find('{'), raw.rfind('}')) {
        let slice = &raw[start..=end];
        let unescaped = slice.replace("\\\"", "\"");
        if let Ok(win) = serde_json::from_str::<WindowLedge>(&unescaped) {
            if win.width > 150 && win.height > 100 {
                return Ok(Some(win));
            }
        }
    }
    Ok(None)
}

