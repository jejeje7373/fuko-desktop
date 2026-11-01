// src/context_watcher.rs
// Observador reactivo autónomo estilo Capri Desktop:
// - Vigila archivos (~/Duoc, workspace, Documentos, Descargas)
// - Detecta música (playerctl) y hace que Claudia baile sola como Capri (CheckMusic)
// - Detecta ventana activa en X11/Wayland y hace Glances autónomos (CheckGlance) comentando lo que haces
// - Detecta compilación (cargo/rustc), CPU alta (CheckLag) y batería baja

use crate::avatar::{AvatarState, AvatarSystem};
use crate::openclaw_bridge::{ask_claudia, OpenClawBridge};
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

/// Obtiene la lista de ventanas reales abiertas desde la extensión ClaudiaPerch (GNOME Wayland)
/// y devuelve un resumen de qué ventana o pestaña estás viendo (por ejemplo en Google Chrome, VS Code, etc.)
async fn get_wayland_windows_summary() -> Option<(String, Option<String>)> {
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
        .ok()?;

    let raw = String::from_utf8_lossy(&out.stdout);
    let start = raw.find('[')?;
    let end = raw.rfind(']')?;
    let slice = &raw[start..=end];
    let unescaped = slice.replace("\\\"", "\"");

    let items: Vec<serde_json::Value> = serde_json::from_str(&unescaped).ok()?;
    if items.is_empty() {
        return None;
    }

    // Buscar ventanas activas prioritarias (Chrome, VS Code, etc.)
    let mut top_window = String::new();
    let mut chrome_tab = None;

    for item in &items {
        let title = item.get("title").and_then(|t| t.as_str()).unwrap_or("");
        let wm_class = item.get("wm_class").and_then(|c| c.as_str()).unwrap_or("");

        if wm_class == "gjs" || title.is_empty() {
            continue;
        }

        if top_window.is_empty() {
            top_window = title.to_string();
        }

        if wm_class.contains("chrome") || title.contains("Google Chrome") {
            chrome_tab = Some(title.replace(" - Google Chrome", "").trim().to_string());
        }
    }

    if top_window.is_empty() {
        None
    } else {
        Some((top_window, chrome_tab))
    }
}

/// Obtiene el título de la ventana activa en X11/XWayland o fallback a Wayland
async fn get_active_window_title() -> Option<String> {
    // 1. Prioridad: Extensión nativa de GNOME Wayland
    if let Some((top_win, chrome_tab)) = get_wayland_windows_summary().await {
        if let Some(tab) = chrome_tab {
            return Some(format!("Navegador: {}", tab));
        }
        return Some(top_win);
    }

    // 2. Fallback clásico xprop
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
        .args(["-id", win_id, "_NET_WM_NAME", "WM_NAME"])
        .output()
        .await
        .ok()?;
    let name_str = String::from_utf8_lossy(&name_out.stdout);
    for line in name_str.lines() {
        if let Some((_, val)) = line.split_once('=') {
            let clean = val.trim().trim_matches('"').trim().to_string();
            if !clean.is_empty()
                && !clean.eq_ignore_ascii_case("Claudia")
                && !clean.eq_ignore_ascii_case("Fuko")
                && !clean.contains("Claudia Companion")
            {
                return Some(clean);
            }
        }
    }
    None
}

/// Inicia el watcher de archivos y el monitor autónomo estilo Capri
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
                                        "Acabo de modificar el archivo '{}' en mi notebook. Haz un comentario cortito (1-2 oraciones, estilo cyberpunk/compañera como Capri) reaccionando a lo que estoy haciendo.",
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

    // 2. Loop autónomo del notebook (Música -> Bailar sola, Ventana activa -> Glance autónomo, Compilación, Batería)
    let app_for_sys = app_handle.clone();
    tauri::async_runtime::spawn(async move {
        let mut last_song = String::new();
        let mut music_was_playing = false;
        let mut was_compiling = false;
        let mut warned_low_battery = false;
        let mut last_window_title = String::new();
        // Primer glance autónomo ocurre ~14 segundos después de iniciar para que Aym vea a Claudia reaccionar sola
        let mut last_glance_time = Instant::now() - Duration::from_secs(52);
        let mut interval = tokio::time::interval(Duration::from_secs(4));

        loop {
            interval.tick().await;

            // A. Detectar música con playerctl (Capri CheckMusic: baila sola mientras suene música)
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
                    battery_info = format!("🔋 {}% ({})", cap, st);
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

            // C. Detectar ventana activa y procesos de trabajo
            let win_title = get_active_window_title().await;
            let mut active_apps = Vec::new();
            let mut is_compiling = false;
            if let Ok(ps_out) = Command::new("ps").args(["-u", "jejej", "-o", "comm="]).output().await {
                let procs = String::from_utf8_lossy(&ps_out.stdout);
                let proc_set: HashSet<&str> = procs.lines().map(|l| l.trim()).collect();

                if proc_set.contains("cargo") || proc_set.contains("rustc") {
                    is_compiling = true;
                    active_apps.push("Rust/Cargo (Compilando)");
                }
                if proc_set.contains("code") || proc_set.contains("antigravity") {
                    active_apps.push("Antigravity IDE");
                }
                if proc_set.contains("chrome")
                    || proc_set.contains("chromium")
                    || proc_set.contains("firefox")
                    || proc_set.contains("brave")
                {
                    active_apps.push("Navegador");
                }
                if proc_set.contains("libreoffice") || proc_set.contains("soffice.bin") {
                    active_apps.push("LibreOffice");
                }
                if proc_set.contains("python3") || proc_set.contains("node") {
                    active_apps.push("Node/Python");
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

            let mut summary_parts = Vec::new();
            if let Some(ref wt) = win_title {
                summary_parts.push(format!("Ventana: {}", wt));
            }
            if !active_apps.is_empty() {
                summary_parts.push(format!("Apps: {}", active_apps.join(", ")));
            }
            if let Some(ref m) = music_info {
                summary_parts.push(format!("🎵 {}", m));
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
                    "window_title": win_title,
                }),
            );

            // D. Glance Autónomo estilo Capri (CheckGlance):
            // Cada ~65s, si la ventana activa cambió o hay actividad, Claudia mira y comenta por su cuenta
            let current_wt = win_title.clone().unwrap_or_default();
            let window_changed = !current_wt.is_empty() && current_wt != last_window_title;
            if (window_changed && last_glance_time.elapsed() >= Duration::from_secs(45))
                || last_glance_time.elapsed() >= Duration::from_secs(75)
            {
                if let Some(bridge) = app_for_sys.try_state::<Arc<OpenClawBridge>>() {
                    if bridge.get_status().proactive {
                        last_glance_time = Instant::now();
                        if !current_wt.is_empty() {
                            last_window_title = current_wt.clone();
                        }
                        let app_clone = app_for_sys.clone();
                        let bridge_clone = bridge.inner().clone();
                        let summary_snap = summary.clone();
                        let wt_snap = current_wt.clone();
                        tauri::async_runtime::spawn(async move {
                            let prompt = if !wt_snap.is_empty() {
                                format!(
                                    "Miraste la pantalla de Aym por tu cuenta (Glance). Tiene abierta la ventana '{}' ({}). Hazle un comentario breve, ingenioso y natural (1-2 frases cortas, estilo Capri cyberpunk) sobre lo que está haciendo.",
                                    wt_snap, summary_snap
                                )
                            } else {
                                format!(
                                    "Miraste el notebook de Aym por tu cuenta ({}). Hazle un comentario breve y natural (1-2 frases cortas, estilo Capri cyberpunk) sobre lo que está haciendo.",
                                    summary_snap
                                )
                            };
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
                                        "heard": "(tu pantalla)",
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
        let mut list: Vec<String> = state_lock
            .watched_paths
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();
        list.sort();
        return list;
    }
    vec![]
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
