// src/context_watcher.rs
// Observador reactivo autónomo en tiempo real para Fuko Desktop:
// - Vigila la pantalla y la ventana/pestaña activa cada 1.5s (en cualquier navegador: Chrome, Firefox, Brave, Edge, Opera, Chromium, Vivaldi, Zen)
// - Toma capturas de pantalla en tiempo real y comenta automáticamente cuando abres Google, haces una búsqueda o entras a cualquier página web
// - Detecta música (playerctl), compilación (cargo/rustc), modificaciones de archivos y batería

use crate::avatar::{AvatarState, AvatarSystem};
use crate::openclaw_bridge::{ask_claudia, glance_at_screen_realtime, OpenClawBridge};
use crate::overlay_window::fetch_window_ledges;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tokio::process::Command;
use tokio::sync::mpsc;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ContextData {
    pub app: String,
    pub file: Option<String>,
    pub folder: Option<String>,
    pub summary: String,
    pub timestamp: u64,
}

#[derive(Clone, Debug)]
struct ActiveWindowInfo {
    pub raw_title: String,
    pub clean_title: String,
    pub browser_name: Option<String>,
}

struct ContextWatcherState {
    watchers: HashMap<String, RecommendedWatcher>,
    watched_paths: HashSet<PathBuf>,
    tx: mpsc::UnboundedSender<notify::Result<Event>>,
}

static GLOBAL_STATE: Mutex<Option<Arc<Mutex<ContextWatcherState>>>> = Mutex::new(None);

fn is_ignored_path(path: &Path) -> bool {
    let s = path.to_string_lossy();
    if s.contains("/node_modules/")
        || s.contains("/target/")
        || s.contains("/.git/")
        || s.contains("/.system_generated/")
        || s.contains("/.gemini/")
        || s.contains("openclaw.json")
        || s.contains(".sqlite")
        || s.contains(".log")
        || s.contains(".bak")
        || s.contains("/tmp/")
        || s.ends_with('~')
        || s.ends_with(".swp")
    {
        return true;
    }
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        if name.starts_with('.') {
            return true;
        }
    }
    false
}

/// Detecta si una ventana pertenece a CUALQUIER navegador web y extrae el nombre del navegador y el título limpio de la página/pestaña
fn classify_browser_window(title: &str, wm_class: &str) -> Option<(String, String)> {
    let t_low = title.to_lowercase();
    let c_low = wm_class.to_lowercase();

    let browsers = [
        ("google-chrome", "Google Chrome", " - Google Chrome"),
        ("chrome", "Google Chrome", " - Google Chrome"),
        ("chromium", "Chromium", " - Chromium"),
        ("firefox", "Mozilla Firefox", " — Mozilla Firefox"),
        ("mozilla firefox", "Mozilla Firefox", " - Mozilla Firefox"),
        ("brave", "Brave", " - Brave"),
        ("msedge", "Microsoft Edge", " - Microsoft Edge"),
        ("microsoft-edge", "Microsoft Edge", " - Microsoft Edge"),
        ("opera", "Opera", " - Opera"),
        ("vivaldi", "Vivaldi", " - Vivaldi"),
        ("zen", "Zen Browser", " — Zen Browser"),
        ("epiphany", "GNOME Web", " - Web"),
    ];

    for (class_pat, b_label, suffix) in browsers {
        if c_low.contains(class_pat) || t_low.ends_with(&suffix.to_lowercase()) || t_low.contains(&b_label.to_lowercase()) {
            let mut clean = title.to_string();
            for suf in [
                " - Google Chrome",
                " - Chromium",
                " — Mozilla Firefox",
                " - Mozilla Firefox",
                " - Brave",
                " - Microsoft Edge",
                " - Opera",
                " - Vivaldi",
                " — Zen Browser",
                " - Zen Browser",
            ] {
                if clean.ends_with(suf) {
                    clean = clean.trim_end_matches(suf).trim().to_string();
                }
            }
            if clean.is_empty() {
                clean = "Nueva pestaña".to_string();
            }
            return Some((b_label.to_string(), clean));
        }
    }

    // Si el título mismo contiene una búsqueda de Google aunque wm_class sea genérico (ej. X11)
    if t_low.contains("buscar con google") || t_low.contains("google search") || t_low == "google" {
        return Some(("Navegador".to_string(), title.trim().to_string()));
    }

    None
}

/// Obtiene la ventana activa en tiempo real (Wayland ClaudiaPerch + fallback X11 xprop / xdotool)
async fn get_active_window_info() -> Option<ActiveWindowInfo> {
    // 1. Usar la lista de ventanas reales (ordenadas con la ventana enfocada primero)
    let ledges = fetch_window_ledges().await;
    if let Some(focused_win) = ledges.iter().find(|w| w.focused).or_else(|| ledges.first()) {
        let title = focused_win.title.trim().to_string();
        if !title.is_empty() {
            if let Some((b_name, clean_tab)) = classify_browser_window(&title, &focused_win.wm_class) {
                return Some(ActiveWindowInfo {
                    raw_title: title,
                    clean_title: clean_tab,
                    browser_name: Some(b_name),
                });
            }
            return Some(ActiveWindowInfo {
                raw_title: title.clone(),
                clean_title: title,
                browser_name: None,
            });
        }
    }

    // 2. Fallback directo con xprop (_NET_ACTIVE_WINDOW)
    let root_out = Command::new("xprop")
        .args(["-root", "_NET_ACTIVE_WINDOW"])
        .output()
        .await
        .ok()?;
    let root_str = String::from_utf8_lossy(&root_out.stdout);
    let win_id = root_str.split_whitespace().last()?.trim();
    if win_id.is_empty() || win_id == "0x0" {
        return None;
    }
    let name_out = Command::new("xprop")
        .args(["-id", win_id, "_NET_WM_NAME", "WM_NAME", "WM_CLASS"])
        .output()
        .await
        .ok()?;
    let name_str = String::from_utf8_lossy(&name_out.stdout);
    let mut found_title = String::new();
    let mut found_class = String::new();
    for line in name_str.lines() {
        if line.contains("WM_CLASS") {
            if let Some((_, val)) = line.split_once('=') {
                found_class = val.trim().to_string();
            }
        } else if let Some((_, val)) = line.split_once('=') {
            let clean = val.trim().trim_matches('"').trim().to_string();
            if !clean.is_empty()
                && !clean.eq_ignore_ascii_case("Claudia")
                && !clean.eq_ignore_ascii_case("Fuko")
                && !clean.contains("Fuko Mainframe")
            {
                found_title = clean;
            }
        }
    }

    if !found_title.is_empty() {
        if let Some((b_name, clean_tab)) = classify_browser_window(&found_title, &found_class) {
            return Some(ActiveWindowInfo {
                raw_title: found_title,
                clean_title: clean_tab,
                browser_name: Some(b_name),
            });
        }
        return Some(ActiveWindowInfo {
            raw_title: found_title.clone(),
            clean_title: found_title,
            browser_name: None,
        });
    }

    None
}

/// Inicia el watcher de archivos y el monitor visual en tiempo real estilo Capri
pub fn start_watcher<R: Runtime>(
    app_handle: AppHandle<R>,
    initial_paths: Vec<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    let (tx, mut rx) = mpsc::unbounded_channel();

    let state = Arc::new(Mutex::new(ContextWatcherState {
        watchers: HashMap::new(),
        watched_paths: HashSet::new(),
        tx: tx.clone(),
    }));

    if let Ok(mut global) = GLOBAL_STATE.lock() {
        *global = Some(state.clone());
    }

    for path in initial_paths {
        if path.exists() {
            let is_huge = path.to_string_lossy().ends_with("/workspace");
            let _ = watch_path(state.clone(), &path, !is_huge);
        }
    }

    // 1. Loop de eventos de archivos (notify)
    let app_for_files = app_handle.clone();
    tauri::async_runtime::spawn(async move {
        let mut last_emitted_file = String::new();
        let mut last_emit_time = Instant::now() - Duration::from_secs(10);
        let mut last_ai_reaction = Instant::now() - Duration::from_secs(60);

        while let Some(res) = rx.recv().await {
            if let Ok(event) = res {
                match event.kind {
                    EventKind::Modify(_) | EventKind::Create(_) => {}
                    _ => continue,
                }

                for path in event.paths {
                    if is_ignored_path(&path) || !path.is_file() {
                        continue;
                    }

                    let path_str = path.to_string_lossy().to_string();
                    if path_str == last_emitted_file
                        && last_emit_time.elapsed() < Duration::from_secs(4)
                    {
                        continue;
                    }

                    last_emitted_file = path_str.clone();
                    last_emit_time = Instant::now();

                    let file_name = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("archivo")
                        .to_string();
                    let folder_str = path.parent().map(|p| p.to_string_lossy().to_string());

                    let timestamp = SystemTime::now()
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64;

                    let summary = format!("Editando {}", file_name);
                    let data = ContextData {
                        app: "Archivo modificado".to_string(),
                        file: Some(path_str.clone()),
                        folder: folder_str,
                        summary: summary.clone(),
                        timestamp,
                    };

                    if let Some(bridge) = app_for_files.try_state::<Arc<OpenClawBridge>>() {
                        bridge.update_context(Some(path_str.clone()), None);
                    }

                    let _ = app_for_files.emit("context_received", &data);

                    if last_ai_reaction.elapsed() >= Duration::from_secs(30) {
                        if let Some(bridge) = app_for_files.try_state::<Arc<OpenClawBridge>>() {
                            if bridge.get_status().proactive {
                                last_ai_reaction = Instant::now();
                                let app_clone = app_for_files.clone();
                                let bridge_clone = bridge.inner().clone();
                                let fname = file_name.clone();
                                tauri::async_runtime::spawn(async move {
                                    let prompt = format!(
                                        "Acabo de modificar el archivo '{}' en mi notebook. Haz un comentario cortito (1-2 oraciones, estilo cyberpunk/compañera como Fuko) reaccionando a lo que estoy haciendo.",
                                        fname
                                    );
                                    if let Ok(res) = ask_claudia(
                                        &app_clone,
                                        &bridge_clone,
                                        prompt,
                                        Some("openclaw".to_string()),
                                        true,
                                    )
                                    .await
                                    {
                                        let _ = app_clone.emit(
                                            "avatar_speak",
                                            serde_json::json!({
                                                "heard": format!("(editando {})", fname),
                                                "message": res.reply,
                                                "model": res.model_used
                                            }),
                                        );
                                    }
                                });
                            }
                        }
                    }
                }
            }
        }
    });

    // 2. Loop visual en tiempo real (cada 1.5s):
    // Detecta al instante cuando abres una página en Google o cualquier navegador, toma captura de pantalla y comenta en vivo
    let app_for_sys = app_handle.clone();
    tauri::async_runtime::spawn(async move {
        let mut last_song = String::new();
        let mut music_was_playing = false;
        let mut was_compiling = false;
        let mut warned_low_battery = false;

        let mut last_commented_window = String::new();
        let mut pending_window_title = String::new();
        let mut pending_since = Instant::now();
        let glance_in_flight = Arc::new(Mutex::new(false));
        let mut last_glance_time = Instant::now() - Duration::from_secs(25);

        let current_user = std::env::var("USER").unwrap_or_else(|_| "jejej".to_string());
        let mut interval = tokio::time::interval(Duration::from_millis(1500));

        loop {
            interval.tick().await;

            // A. Detectar música con playerctl
            let mut music_info: Option<String> = None;
            let mut is_music_playing = false;
            if let Ok(status_out) = Command::new("playerctl").arg("status").output().await {
                let st = String::from_utf8_lossy(&status_out.stdout).trim().to_string();
                if st.eq_ignore_ascii_case("Playing") {
                    is_music_playing = true;
                    if let Ok(meta_out) = Command::new("playerctl")
                        .args(["metadata", "--format", "{{artist}} - {{title}}"])
                        .output()
                        .await
                    {
                        let song = String::from_utf8_lossy(&meta_out.stdout)
                            .trim()
                            .trim_matches('-')
                            .trim()
                            .to_string();
                        if !song.is_empty() {
                            music_info = Some(song.clone());
                            if song != last_song {
                                last_song = song.clone();
                                let _ = app_for_sys.emit(
                                    "avatar_speak",
                                    serde_json::json!({
                                        "heard": "(música detectada)",
                                        "message": format!("Ooh, buen tema choom: {}. ¡Subámosle el volumen!", song),
                                        "model": "sensor-audio"
                                    }),
                                );
                            }
                        }
                    }
                }
            }

            if is_music_playing != music_was_playing {
                music_was_playing = is_music_playing;
                let _ = app_for_sys.emit(
                    "music_state_changed",
                    serde_json::json!({
                        "playing": is_music_playing,
                        "song": music_info
                    }),
                );
            }

            // B. Detectar batería del notebook
            let mut battery_info = String::new();
            for bat_dir in ["/sys/class/power_supply/BAT0", "/sys/class/power_supply/BAT1"] {
                let cap_path = Path::new(bat_dir).join("capacity");
                let st_path = Path::new(bat_dir).join("status");
                if let (Ok(cap_str), Ok(st_str)) = (
                    std::fs::read_to_string(&cap_path),
                    std::fs::read_to_string(&st_path),
                ) {
                    let cap: u32 = cap_str.trim().parse().unwrap_or(100);
                    let st = st_str.trim();
                    battery_info = format!("Bateria: {}% ({})", cap, st);
                    if cap <= 20 && st == "Discharging" && !warned_low_battery {
                        warned_low_battery = true;
                        let _ = app_for_sys.emit(
                            "avatar_speak",
                            serde_json::json!({
                                "heard": "(batería baja)",
                                "message": format!("Oye choom, te queda {}% de batería. Enchufa el cargador antes de que me apague.", cap),
                                "model": "sensor-energia"
                            }),
                        );
                    } else if cap > 25 || st == "Charging" {
                        warned_low_battery = false;
                    }
                    break;
                }
            }

            // C. Detectar ventana activa, navegador en tiempo real y procesos
            let win_info = get_active_window_info().await;
            let mut active_apps = Vec::new();
            let mut is_compiling = false;
            if let Ok(ps_out) = Command::new("ps")
                .args(["-u", &current_user, "-o", "comm="])
                .output()
                .await
            {
                let procs = String::from_utf8_lossy(&ps_out.stdout);
                let proc_set: HashSet<&str> = procs.lines().map(|l| l.trim()).collect();

                if proc_set.contains("cargo") || proc_set.contains("rustc") {
                    is_compiling = true;
                    active_apps.push("Rust/Cargo (Compilando)");
                }
                if proc_set.contains("code") || proc_set.contains("antigravity") {
                    active_apps.push("VS Code / Antigravity");
                }
                if proc_set.iter().any(|p| {
                    p.contains("chrome")
                        || p.contains("chromium")
                        || p.contains("firefox")
                        || p.contains("brave")
                        || p.contains("msedge")
                        || p.contains("opera")
                        || p.contains("vivaldi")
                        || p.contains("zen")
                }) {
                    active_apps.push("Navegador");
                }
                if proc_set.contains("libreoffice") || proc_set.contains("soffice.bin") {
                    active_apps.push("LibreOffice");
                }
            }

            if is_compiling && !was_compiling {
                was_compiling = true;
                if let Some(avatar) = app_for_sys.try_state::<Arc<AvatarSystem<R>>>() {
                    avatar.set_state(AvatarState::Thinking);
                }
                let _ = app_for_sys.emit(
                    "avatar_speak",
                    serde_json::json!({
                        "heard": "(cargo build)",
                        "message": "Veo que estás compilando en Rust, choom. Vigilando que el compilador se porte bien.",
                        "model": "sensor-sistema"
                    }),
                );
            } else if !is_compiling && was_compiling {
                was_compiling = false;
                if let Some(avatar) = app_for_sys.try_state::<Arc<AvatarSystem<R>>>() {
                    avatar.set_state(AvatarState::Idle);
                }
            }

            let win_display = win_info.as_ref().map(|w| {
                if let Some(ref b) = w.browser_name {
                    format!("{}: {}", b, w.clean_title)
                } else {
                    w.clean_title.clone()
                }
            });

            let mut summary_parts = Vec::new();
            if let Some(ref wd) = win_display {
                summary_parts.push(format!("Pantalla activa: {}", wd));
            }
            if !active_apps.is_empty() {
                summary_parts.push(format!("Apps: {}", active_apps.join(", ")));
            }
            if let Some(ref m) = music_info {
                summary_parts.push(format!("Musica: {}", m));
            }
            if !battery_info.is_empty() {
                summary_parts.push(battery_info);
            }
            let summary = if summary_parts.is_empty() {
                "Escritorio activo".to_string()
            } else {
                summary_parts.join(" · ")
            };

            if let Some(bridge) = app_for_sys.try_state::<Arc<OpenClawBridge>>() {
                bridge.update_context(None, Some(summary.clone()));
            }

            let _ = app_for_sys.emit(
                "notebook_activity",
                serde_json::json!({
                    "summary": summary,
                    "music": music_info,
                    "compiling": is_compiling,
                    "window_title": win_display,
                }),
            );

            // D. Visión en tiempo real y comentarios automáticos cuando abres páginas en Google / navegador o cambias de ventana
            let Some(ref info) = win_info else {
                continue;
            };
            let current_key = format!(
                "{}|{}",
                info.browser_name.as_deref().unwrap_or("app"),
                info.clean_title
            );

            if current_key != pending_window_title {
                pending_window_title = current_key.clone();
                pending_since = Instant::now();
            }

            let is_browser = info.browser_name.is_some();
            let is_new_page = !info.clean_title.is_empty() && current_key != last_commented_window;
            // Si abriste una página nueva en un navegador, reaccionar tras ~2.0s de estabilidad (con cooldown corto de 10s).
            // Si es otra aplicación de escritorio o sigues en la misma pantalla, comentar cada ~38s.
            let ready_for_browser_reaction = is_browser
                && is_new_page
                && pending_since.elapsed() >= Duration::from_millis(1800)
                && last_glance_time.elapsed() >= Duration::from_secs(10);

            let ready_for_general_glance = (is_new_page
                && pending_since.elapsed() >= Duration::from_secs(3)
                && last_glance_time.elapsed() >= Duration::from_secs(22))
                || last_glance_time.elapsed() >= Duration::from_secs(42);

            if ready_for_browser_reaction || ready_for_general_glance {
                let busy = *glance_in_flight.lock().unwrap();
                if !busy {
                    if let Some(bridge) = app_for_sys.try_state::<Arc<OpenClawBridge>>() {
                        if bridge.get_status().proactive {
                            *glance_in_flight.lock().unwrap() = true;
                            last_glance_time = Instant::now();
                            last_commented_window = current_key;

                            let app_clone = app_for_sys.clone();
                            let bridge_clone = bridge.inner().clone();
                            let summary_snap = summary.clone();
                            let clean_title_snap = info.clean_title.clone();
                            let browser_snap = info.browser_name.clone();
                            let in_flight_clone = glance_in_flight.clone();

                            tauri::async_runtime::spawn(async move {
                                let heard_tag = if let Some(ref b) = browser_snap {
                                    format!("(viendo {}: {})", b, clean_title_snap)
                                } else {
                                    format!("(viendo: {})", clean_title_snap)
                                };

                                if let Ok(res) = glance_at_screen_realtime(
                                    &app_clone,
                                    &bridge_clone,
                                    &clean_title_snap,
                                    browser_snap.as_deref(),
                                    &summary_snap,
                                )
                                .await
                                {
                                    let _ = app_clone.emit(
                                        "avatar_speak",
                                        serde_json::json!({
                                            "heard": heard_tag,
                                            "message": res.reply,
                                            "model": res.model_used
                                        }),
                                    );
                                }
                                *in_flight_clone.lock().unwrap() = false;
                            });
                        }
                    }
                }
            }
        }
    });

    Ok(())
}

/// Vigila una ruta específica
fn watch_path(
    state: Arc<Mutex<ContextWatcherState>>,
    path: &Path,
    recursive: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if !path.exists() {
        return Ok(());
    }
    let mut state_lock = state.lock().unwrap();
    let path_buf = path.to_path_buf();

    if state_lock.watched_paths.contains(&path_buf) {
        return Ok(());
    }

    let state_clone = state.clone();
    let mut watcher = RecommendedWatcher::new(
        move |res| {
            if let Ok(st) = state_clone.lock() {
                let _ = st.tx.send(res);
            }
        },
        notify::Config::default().with_poll_interval(Duration::from_secs(2)),
    )?;

    watcher.watch(
        path,
        if recursive {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        },
    )?;

    let path_str = path.to_string_lossy().to_string();
    state_lock.watchers.insert(path_str.clone(), watcher);
    state_lock.watched_paths.insert(path_buf);

    println!("Watching: {} (recursive: {})", path_str, recursive);
    Ok(())
}

pub fn add_watched_path(path: String, recursive: bool) -> Result<(), String> {
    let path_buf = PathBuf::from(&path);
    if !path_buf.exists() {
        return Err(format!("La ruta no existe: {}", path));
    }

    let state_opt = GLOBAL_STATE.lock().unwrap().clone();
    if let Some(state) = state_opt {
        watch_path(state, &path_buf, recursive).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn remove_watched_path(path: String) -> Result<(), String> {
    let path_buf = PathBuf::from(&path);

    let state_opt = GLOBAL_STATE.lock().unwrap().clone();
    if let Some(state) = state_opt {
        let mut state_lock = state.lock().unwrap();
        let path_str = path_buf.to_string_lossy().to_string();
        state_lock.watchers.remove(&path_str);
        state_lock.watched_paths.remove(&path_buf);
    }
    Ok(())
}

pub fn get_watched_paths() -> Vec<String> {
    let state_opt = GLOBAL_STATE.lock().unwrap().clone();
    if let Some(state) = state_opt {
        let state_lock = state.lock().unwrap();
        state_lock
            .watched_paths
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect()
    } else {
        Vec::new()
    }
}

#[tauri::command]
pub async fn get_watched_paths_cmd() -> Result<Vec<String>, String> {
    Ok(get_watched_paths())
}

#[tauri::command]
pub async fn add_watched_path_cmd(path: String, recursive: bool) -> Result<(), String> {
    add_watched_path(path, recursive)
}

#[tauri::command]
pub async fn remove_watched_path_cmd(path: String) -> Result<(), String> {
    remove_watched_path(path)
}
