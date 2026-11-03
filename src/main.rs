// src/main.rs - Capri / Fuko Desktop Companion (Paridad con MtGhozt/Capri-Desktop + OpenClaw + Antigravity + Sin Emojis)

use std::path::PathBuf;
use std::sync::Arc;
use tauri::{Emitter, Manager};

mod avatar;
mod brain_forks;
mod connections_config;
mod context_watcher;
mod notebook_hands;
mod openclaw_bridge;
mod overlay_window;
mod skills_hub;
mod soul_config;
mod tray;
mod voice_ears;

fn configure_linux_display_env() {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::io::FromRawFd;

        if let Ok(entries) = std::fs::read_dir("/proc/self/fd") {
            let fds: Vec<i32> = entries
                .flatten()
                .filter_map(|e| e.file_name().to_string_lossy().parse::<i32>().ok())
                .filter(|&fd| fd > 2)
                .collect();
            for fd in fds {
                if std::fs::read_link(format!("/proc/self/fd/{}", fd)).is_ok() {
                    unsafe {
                        let _ = std::fs::File::from_raw_fd(fd);
                    }
                }
            }
        }

        for var in [
            "GTK_MODULES",
            "SNAP",
            "SNAP_NAME",
            "SNAP_REVISION",
            "SNAP_LIBRARY_PATH",
            "SNAP_CONTEXT",
            "SNAP_COOKIE",
        ] {
            std::env::remove_var(var);
        }

        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        std::env::set_var("GDK_BACKEND", "x11");

        if std::env::var("DISPLAY").unwrap_or_default().is_empty() {
            std::env::set_var("DISPLAY", ":0");
        }

        let need_xauth = match std::env::var("XAUTHORITY") {
            Ok(val) if !val.is_empty() => !std::path::Path::new(&val).exists(),
            _ => true,
        };

        if need_xauth {
            let run_user = "/run/user/1000";
            if let Ok(entries) = std::fs::read_dir(run_user) {
                for entry in entries.flatten() {
                    let fname = entry.file_name().to_string_lossy().to_string();
                    if fname.starts_with(".mutter-Xwaylandauth.") || fname.contains("Xauthority") {
                        std::env::set_var("XAUTHORITY", entry.path());
                        break;
                    }
                }
            }
        }
    }
}

pub fn run() {
    configure_linux_display_env();
    skills_hub::sync_openclaw_and_antigravity_skills();
    soul_config::ensure_default_openclaw_files();

    let rt_handle = tauri::async_runtime::handle();
    let _rt_guard = rt_handle.inner().enter();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            let app_handle = app.handle().clone();

            let bridge = Arc::new(openclaw_bridge::OpenClawBridge::new());
            app.manage(bridge.clone());

            voice_ears::start_ears_daemon(app_handle.clone(), bridge.clone());

            let avatar_system = avatar::init_avatar_system(app_handle.clone());
            app.manage(Arc::new(avatar_system));

            if let Err(e) = overlay_window::setup_window(&app_handle) {
                eprintln!("Warning setting up overlay window: {}", e);
            }

            let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/home/jejej"));
            let duoc_path = home.join("Duoc");
            if !duoc_path.exists() {
                let _ = std::fs::create_dir_all(&duoc_path);
            }
            let initial_paths = vec![
                duoc_path,
                home.join(".openclaw/workspace"),
                home.join("Documentos"),
                home.join("Descargas"),
            ];
            if let Err(e) = context_watcher::start_watcher(app_handle.clone(), initial_paths) {
                eprintln!("Warning starting context watcher: {}", e);
            }

            if let Err(e) = tray::setup_tray(&app_handle) {
                eprintln!("Warning setting up system tray: {}", e);
            }

            let app_handle_clone = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(tokio::time::Duration::from_millis(1800)).await;
                let msg = "Bueno... ya estoy conectada a tu pantalla, a OpenClaw y a Antigravity. No esperes que haga todo el trabajo por ti, tontito, si?";
                voice_ears::speak_out_loud_async(msg.to_string());
                let _ = app_handle_clone.emit(
                    "avatar_speak",
                    serde_json::json!({
                        "heard": "startup",
                        "message": msg,
                        "model": "OPENCLAW + ANTIGRAVITY"
                    }),
                );
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            avatar::set_avatar_state,
            avatar::get_avatar_state,
            avatar::head_pat,
            avatar::set_avatar_position,
            overlay_window::get_desktop_geometry_cmd,
            overlay_window::get_screen_bounds_cmd,
            overlay_window::set_ignore_cursor_events_cmd,
            overlay_window::move_overlay_window_cmd,
            overlay_window::show_bubble_window_cmd,
            overlay_window::show_peek_window_cmd,
            overlay_window::position_bubble_near_avatar_cmd,
            overlay_window::move_bubble_window_cmd,
            overlay_window::get_window_ledges_cmd,
            overlay_window::get_active_window_cmd,
            overlay_window::open_mainframe_cmd,
            overlay_window::quit_app_cmd,
            context_watcher::get_watched_paths_cmd,
            context_watcher::add_watched_path_cmd,
            context_watcher::remove_watched_path_cmd,
            openclaw_bridge::send_chat_message,
            openclaw_bridge::get_ai_status_cmd,
            openclaw_bridge::set_ai_mode_cmd,
            openclaw_bridge::set_proactive_mode_cmd,
            openclaw_bridge::analyze_notebook_cmd,
            openclaw_bridge::list_openclaw_sessions_cmd,
            openclaw_bridge::spawn_brain_task_cmd,
            openclaw_bridge::probe_gemini_argon_cmd,
            openclaw_bridge::probe_antigravity_cmd,
            connections_config::get_connections_config_cmd,
            connections_config::save_connections_config_cmd,
            connections_config::test_openclaw_connection_cmd,
            connections_config::test_antigravity_connection_cmd,
            soul_config::list_openclaw_files_cmd,
            soul_config::read_openclaw_file_cmd,
            soul_config::save_openclaw_file_cmd,
            soul_config::open_openclaw_file_vscode_cmd,
            voice_ears::get_ears_state_cmd,
            voice_ears::toggle_voice_chat_cmd,
            voice_ears::toggle_mic_mute_cmd,
            voice_ears::stop_mic_recording_cmd,
            voice_ears::set_tts_enabled_cmd,
            voice_ears::set_continuous_listen_cmd,
            voice_ears::listen_and_ask_cmd,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app_handle, _event| {});
}

fn main() {
    run();
}
