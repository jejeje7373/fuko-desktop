// src/notebook_hands.rs
// "Hands & Limbs" de Claudia (inspirado en Hands.cs / Limbs.cs de Capri Desktop):
// Permite a Claudia abrir programas reales del notebook, abrir carpetas/archivos/URLs,
// controlar volumen, brillo, música, portapapeles y ejecutar comandos directamente en Linux.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use tauri::{AppHandle, Emitter, Runtime};
use tokio::process::Command;

/// Limpia variables de entorno de Snap/Overlay para que cualquier app del notebook
/// (Chrome, VS Code, Firefox, Nautilus, Terminal, etc.) abra con su entorno nativo de Ubuntu.
fn prepare_host_command(program: &str) -> Command {
    let mut cmd = Command::new(program);
    cmd.env_remove("GTK_MODULES")
        .env_remove("GDK_BACKEND")
        .env_remove("SNAP")
        .env_remove("SNAP_NAME")
        .env_remove("SNAP_REVISION")
        .env_remove("SNAP_ARCH")
        .env_remove("SNAP_COMMON")
        .env_remove("SNAP_DATA")
        .env_remove("SNAP_USER_COMMON")
        .env_remove("SNAP_USER_DATA")
        .env_remove("SNAP_LIBRARY_PATH")
        .env_remove("SNAP_CONTEXT")
        .env_remove("SNAP_COOKIE")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    cmd
}

/// Lanza un proceso desacoplado en segundo plano en el notebook usando `std::process::Command`
/// (seguro desde cualquier hilo, sin depender del reactor pidfd de Tokio).
fn spawn_detached(program: &str, args: &[&str]) -> Result<(), String> {
    let mut cmd = std::process::Command::new(program);
    cmd.env_remove("GTK_MODULES")
        .env_remove("GDK_BACKEND")
        .env_remove("SNAP")
        .env_remove("SNAP_NAME")
        .env_remove("SNAP_REVISION")
        .env_remove("SNAP_ARCH")
        .env_remove("SNAP_COMMON")
        .env_remove("SNAP_DATA")
        .env_remove("SNAP_USER_COMMON")
        .env_remove("SNAP_USER_DATA")
        .env_remove("SNAP_LIBRARY_PATH")
        .env_remove("SNAP_CONTEXT")
        .env_remove("SNAP_COOKIE")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .args(args);
    cmd.spawn()
        .map(|_| ())
        .map_err(|e| format!("No pude iniciar '{}': {}", program, e))
}

/// Expande `~` al directorio home real (`/home/jejej`)
pub fn expand_path(raw: &str) -> PathBuf {
    let trimmed = raw.trim().trim_matches('"').trim_matches('\'');
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/home/jejej"));
    if trimmed == "~" {
        return home;
    }
    if let Some(rest) = trimmed.strip_prefix("~/") {
        return home.join(rest);
    }
    PathBuf::from(trimmed)
}

/// Abre una aplicación instalada en el notebook por nombre común o archivo .desktop
pub async fn open_application(app_query: &str) -> Result<String, String> {
    let q = app_query
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_lowercase();

    if q.is_empty() {
        return Err("No me dijiste qué programa abrir.".to_string());
    }

    // 1. Mapa directo de programas frecuentes en el notebook de Aym
    match q.as_str() {
        "chrome" | "google chrome" | "google-chrome" | "navegador" => {
            spawn_detached("google-chrome", &[])?;
            return Ok("Google Chrome".to_string());
        }
        "firefox" | "mozilla" => {
            spawn_detached("firefox", &[])?;
            return Ok("Firefox".to_string());
        }
        "code" | "vscode" | "vs code" | "visual studio code" => {
            spawn_detached("code", &[])?;
            return Ok("Visual Studio Code".to_string());
        }
        "antigravity" | "agy" => {
            if Path::new("/snap/bin/antigravity").exists() {
                spawn_detached("/snap/bin/antigravity", &[])?;
            } else {
                spawn_detached("antigravity", &[])?;
            }
            return Ok("Antigravity IDE".to_string());
        }
        "android studio" | "android-studio" | "android" => {
            if Path::new("/snap/bin/android-studio").exists() {
                spawn_detached("/snap/bin/android-studio", &[])?;
            } else {
                spawn_detached("gtk-launch", &["android-studio_android-studio.desktop"])?;
            }
            return Ok("Android Studio".to_string());
        }
        "intellij" | "idea" | "intellij idea" | "intellij-idea" => {
            if Path::new("/snap/bin/intellij-idea").exists() {
                spawn_detached("/snap/bin/intellij-idea", &[])?;
            } else {
                spawn_detached("gtk-launch", &["intellij-idea_intellij-idea.desktop"])?;
            }
            return Ok("IntelliJ IDEA".to_string());
        }
        "terminal" | "consola" | "gnome-terminal" | "bash" => {
            spawn_detached("gnome-terminal", &[])?;
            return Ok("Terminal de GNOME".to_string());
        }
        "kitty" => {
            spawn_detached("kitty", &[])?;
            return Ok("Kitty Terminal".to_string());
        }
        "archivos" | "explorador" | "carpetas" | "nautilus" | "files" | "gestor de archivos" => {
            spawn_detached("nautilus", &[])?;
            return Ok("Archivos (Nautilus)".to_string());
        }
        "thunar" => {
            spawn_detached("thunar", &[])?;
            return Ok("Thunar".to_string());
        }
        "calculadora" | "calc" | "calculator" | "gnome-calculator" | "qalculate" => {
            if Path::new("/usr/bin/gnome-calculator").exists() {
                spawn_detached("gnome-calculator", &[])?;
            } else {
                spawn_detached("qalculate-gtk", &[])?;
            }
            return Ok("Calculadora".to_string());
        }
        "editor" | "editor de texto" | "bloc de notas" | "notas" | "text editor" | "mousepad" => {
            if Path::new("/usr/bin/gnome-text-editor").exists() {
                spawn_detached("gnome-text-editor", &[])?;
            } else {
                spawn_detached("mousepad", &[])?;
            }
            return Ok("Editor de Texto".to_string());
        }
        "monitor" | "monitor del sistema" | "system monitor" | "administrador de tareas" | "procesos" => {
            spawn_detached("gnome-system-monitor", &[])?;
            return Ok("Monitor del Sistema".to_string());
        }
        "btop" | "htop" => {
            if Path::new("/usr/bin/kitty").exists() {
                spawn_detached("kitty", &["-e", "btop"])?;
            } else {
                spawn_detached("gnome-terminal", &["--", "btop"])?;
            }
            return Ok("Monitor btop".to_string());
        }
        "nvtop" | "gpu" => {
            if Path::new("/usr/bin/kitty").exists() {
                spawn_detached("kitty", &["-e", "nvtop"])?;
            } else {
                spawn_detached("gnome-terminal", &["--", "nvtop"])?;
            }
            return Ok("Monitor GPU nvtop".to_string());
        }
        "configuracion" | "configuración" | "ajustes" | "settings" | "panel de control" => {
            spawn_detached("gnome-control-center", &[])?;
            return Ok("Configuración del Sistema".to_string());
        }
        "sonido" | "audio" | "pavucontrol" | "mezclador" => {
            spawn_detached("pavucontrol", &[])?;
            return Ok("Control de Audio (Pavucontrol)".to_string());
        }
        "bluetooth" | "blueman" => {
            spawn_detached("blueman-manager", &[])?;
            return Ok("Administrador Bluetooth".to_string());
        }
        "docker" | "docker desktop" | "docker-desktop" => {
            spawn_detached("gtk-launch", &["docker-desktop.desktop"])?;
            return Ok("Docker Desktop".to_string());
        }
        "virtualbox" | "maquina virtual" | "máquina virtual" | "vm" => {
            spawn_detached("virtualbox", &[])?;
            return Ok("VirtualBox".to_string());
        }
        "camara" | "cámara" | "webcam" | "cheese" => {
            if Path::new("/snap/bin/cheese").exists() {
                spawn_detached("/snap/bin/cheese", &[])?;
            } else {
                spawn_detached("cheese", &[])?;
            }
            return Ok("Cámara (Cheese)".to_string());
        }
        "torrent" | "qbittorrent" => {
            spawn_detached("qbittorrent", &[])?;
            return Ok("qBittorrent".to_string());
        }
        "mpv" | "video" | "reproductor" => {
            spawn_detached("mpv", &["--player-operation-mode=pseudo-gui"])?;
            return Ok("Reproductor MPV".to_string());
        }
        "discos" | "disks" | "gnome-disks" => {
            spawn_detached("gnome-disks", &[])?;
            return Ok("Utilidad de Discos".to_string());
        }
        "idle" | "python" => {
            spawn_detached("idle-python3.12", &[])?;
            return Ok("IDLE Python 3.12".to_string());
        }
        _ => {}
    }

    // 2. Buscar en archivos .desktop instalados (/usr/share/applications, snapd, ~/.local)
    let desktop_dirs = [
        PathBuf::from("/usr/share/applications"),
        PathBuf::from("/var/lib/snapd/desktop/applications"),
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/home/jejej"))
            .join(".local/share/applications"),
    ];

    for dir in &desktop_dirs {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("desktop") {
                    continue;
                }
                let fname = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                if let Ok(content) = std::fs::read_to_string(&path) {
                    let mut app_name = String::new();
                    for line in content.lines() {
                        if let Some(n) = line.strip_prefix("Name=") {
                            if app_name.is_empty() {
                                app_name = n.trim().to_string();
                            }
                        }
                    }
                    if fname.contains(&q) || app_name.to_lowercase().contains(&q) {
                        let desktop_id = path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("");
                        spawn_detached("gtk-launch", &[desktop_id])?;
                        return Ok(if app_name.is_empty() {
                            desktop_id.trim_end_matches(".desktop").to_string()
                        } else {
                            app_name
                        });
                    }
                }
            }
        }
    }

    // 3. Intentar ejecutar el binario directamente si existe en PATH o /snap/bin
    let snap_candidate = format!("/snap/bin/{}", q);
    if Path::new(&snap_candidate).exists() {
        spawn_detached(&snap_candidate, &[])?;
        return Ok(q);
    }

    spawn_detached(&q, &[])?;
    Ok(app_query.to_string())
}

/// Abre una carpeta o archivo en el notebook (o en VS Code si se pide)
pub fn open_path_or_folder(raw_target: &str, in_vscode: bool) -> Result<String, String> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/home/jejej"));
    let lower = raw_target.trim().to_lowercase();

    let resolved: PathBuf = match lower.as_str() {
        "duoc" | "carpeta duoc" | "mis ramos" | "estudios" => home.join("Duoc"),
        "descargas" | "downloads" => home.join("Descargas"),
        "documentos" | "documents" => home.join("Documentos"),
        "workspace" | "openclaw" | "proyectos" => home.join(".openclaw/workspace"),
        "claudia" | "claudia-desktop" => home.join(".openclaw/workspace/claudia-desktop"),
        "home" | "inicio" | "carpeta personal" => home,
        "escritorio" | "desktop" => {
            let d = home.join("Escritorio");
            if d.exists() { d } else { home }
        }
        _ => expand_path(raw_target),
    };

    if !resolved.exists() {
        return Err(format!("La ruta '{}' no existe en tu notebook.", resolved.display()));
    }

    let path_str = resolved.to_string_lossy().to_string();
    if in_vscode {
        spawn_detached("code", &[&path_str])?;
        Ok(format!("{} en VS Code", path_str))
    } else if resolved.is_dir() {
        spawn_detached("nautilus", &[&path_str])?;
        Ok(path_str)
    } else {
        spawn_detached("xdg-open", &[&path_str])?;
        Ok(path_str)
    }
}

/// Abre una URL o sitio web conocido en el navegador del usuario
pub fn open_url_or_site(target: &str) -> Result<String, String> {
    let t = target.trim().trim_matches('"').trim_matches('\'');
    let lower = t.to_lowercase();

    let url = match lower.as_str() {
        "youtube" | "yt" => "https://www.youtube.com".to_string(),
        "github" | "gh" => "https://github.com".to_string(),
        "duoc" | "ava" | "experiencia21" | "vivo duoc" => "https://experiencia21.duoc.cl".to_string(),
        "gmail" | "correo" | "mail" => "https://mail.google.com".to_string(),
        "whatsapp" | "wsp" => "https://web.whatsapp.com".to_string(),
        "google" => "https://www.google.com".to_string(),
        "chatgpt" => "https://chatgpt.com".to_string(),
        "discord" => "https://discord.com/app".to_string(),
        "spotify" => "https://open.spotify.com".to_string(),
        _ if t.starts_with("http://") || t.starts_with("https://") => t.to_string(),
        _ if t.contains('.') && !t.contains(' ') => format!("https://{}", t),
        _ => {
            let encoded = t.replace(' ', "+");
            format!("https://www.google.com/search?q={}", encoded)
        }
    };

    spawn_detached("xdg-open", &[&url])?;
    Ok(url)
}

/// Busca en YouTube o Google directamente en el navegador.
/// Si `youtube == true`, resuelve el primer `videoId` para reproducir el video/canción directamente.
pub async fn search_in_browser(query: &str, youtube: bool) -> Result<String, String> {
    let encoded = query
        .trim()
        .replace(' ', "+")
        .replace('"', "")
        .replace('\'', "");
    let url = if youtube {
        let search_url = format!("https://www.youtube.com/results?search_query={}", encoded);
        match resolve_first_youtube_watch_url(&search_url).await {
            Some(watch_url) => watch_url,
            None => search_url,
        }
    } else {
        format!("https://www.google.com/search?q={}", encoded)
    };
    spawn_detached("xdg-open", &[&url])?;
    Ok(url)
}

async fn resolve_first_youtube_watch_url(search_url: &str) -> Option<String> {
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36")
        .timeout(std::time::Duration::from_millis(2500))
        .build()
        .ok()?;
    let html = client.get(search_url).send().await.ok()?.text().await.ok()?;
    let needle = "\"videoId\":\"";
    let idx = html.find(needle)?;
    let rest = &html[idx + needle.len()..];
    let end = rest.find('"')?;
    let video_id = &rest[..end];
    if video_id.len() == 11 {
        Some(format!("https://www.youtube.com/watch?v={}", video_id))
    } else {
        None
    }
}

/// Controla el volumen del notebook usando pactl
pub async fn control_volume(action: &str) -> Result<String, String> {
    let act = action.trim().to_lowercase();
    match act.as_str() {
        "up" | "subir" | "mas" | "más" | "+" => {
            let _ = Command::new("pactl")
                .args(["set-sink-mute", "@DEFAULT_SINK@", "0"])
                .output()
                .await;
            Command::new("pactl")
                .args(["set-sink-volume", "@DEFAULT_SINK@", "+15%"])
                .output()
                .await
                .map_err(|e| e.to_string())?;
            Ok("Subí el volumen un 15%.".to_string())
        }
        "down" | "bajar" | "menos" | "-" => {
            Command::new("pactl")
                .args(["set-sink-volume", "@DEFAULT_SINK@", "-15%"])
                .output()
                .await
                .map_err(|e| e.to_string())?;
            Ok("Bajé el volumen un 15%.".to_string())
        }
        "mute" | "silenciar" | "silencio" => {
            Command::new("pactl")
                .args(["set-sink-mute", "@DEFAULT_SINK@", "1"])
                .output()
                .await
                .map_err(|e| e.to_string())?;
            Ok("Audio silenciado, choom.".to_string())
        }
        "unmute" | "activar" => {
            Command::new("pactl")
                .args(["set-sink-mute", "@DEFAULT_SINK@", "0"])
                .output()
                .await
                .map_err(|e| e.to_string())?;
            Ok("Audio reactivado.".to_string())
        }
        _ => {
            let num: String = act.chars().filter(|c| c.is_ascii_digit()).collect();
            if let Ok(pct) = num.parse::<u32>() {
                let clamped = pct.min(120);
                let _ = Command::new("pactl")
                    .args(["set-sink-mute", "@DEFAULT_SINK@", "0"])
                    .output()
                    .await;
                Command::new("pactl")
                    .args([
                        "set-sink-volume",
                        "@DEFAULT_SINK@",
                        &format!("{}%", clamped),
                    ])
                    .output()
                    .await
                    .map_err(|e| e.to_string())?;
                return Ok(format!("Volumen ajustado al {}%.", clamped));
            }
            Err("Acción de volumen no reconocida.".to_string())
        }
    }
}

/// Controla el brillo de pantalla del notebook usando brightnessctl
pub async fn control_brightness(action: &str) -> Result<String, String> {
    let act = action.trim().to_lowercase();
    match act.as_str() {
        "up" | "subir" | "mas" | "más" | "+" => {
            Command::new("brightnessctl")
                .args(["set", "+15%"])
                .output()
                .await
                .map_err(|e| e.to_string())?;
            Ok("Subí el brillo de tu pantalla.".to_string())
        }
        "down" | "bajar" | "menos" | "-" => {
            Command::new("brightnessctl")
                .args(["set", "15%-"])
                .output()
                .await
                .map_err(|e| e.to_string())?;
            Ok("Bajé el brillo de tu pantalla.".to_string())
        }
        _ => {
            let num: String = act.chars().filter(|c| c.is_ascii_digit()).collect();
            if let Ok(pct) = num.parse::<u32>() {
                let clamped = pct.clamp(5, 100);
                Command::new("brightnessctl")
                    .args(["set", &format!("{}%", clamped)])
                    .output()
                    .await
                    .map_err(|e| e.to_string())?;
                return Ok(format!("Brillo ajustado al {}%.", clamped));
            }
            Err("Acción de brillo no reconocida.".to_string())
        }
    }
}

/// Controla la reproducción multimedia con playerctl
pub async fn control_media(action: &str) -> Result<String, String> {
    let act = action.trim().to_lowercase();
    let arg = match act.as_str() {
        "play" | "pause" | "play-pause" | "pausa" | "reproducir" => "play-pause",
        "next" | "siguiente" => "next",
        "prev" | "previous" | "anterior" => "previous",
        "stop" | "detener" => "stop",
        _ => "play-pause",
    };
    Command::new("playerctl")
        .arg(arg)
        .output()
        .await
        .map_err(|e| e.to_string())?;
    Ok(format!("Control multimedia: {}.", arg))
}

/// Obtiene estado real de recursos del notebook (RAM, Disco, Batería, CPU)
pub async fn get_notebook_diagnostics() -> String {
    let mut parts = Vec::new();

    if let Ok(mem) = std::fs::read_to_string("/proc/meminfo") {
        let mut total_kb = 0u64;
        let mut avail_kb = 0u64;
        for line in mem.lines() {
            if line.starts_with("MemTotal:") {
                total_kb = line
                    .split_whitespace()
                    .nth(1)
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
            } else if line.starts_with("MemAvailable:") {
                avail_kb = line
                    .split_whitespace()
                    .nth(1)
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
            }
        }
        if total_kb > 0 {
            let used_gb = (total_kb.saturating_sub(avail_kb)) as f64 / 1048576.0;
            let total_gb = total_kb as f64 / 1048576.0;
            parts.push(format!("RAM: {:.1}/{:.1} GB en uso", used_gb, total_gb));
        }
    }

    if let Ok(df_out) = Command::new("df").args(["-h", "/"]).output().await {
        let s = String::from_utf8_lossy(&df_out.stdout);
        if let Some(line) = s.lines().nth(1) {
            let cols: Vec<&str> = line.split_whitespace().collect();
            if cols.len() >= 5 {
                parts.push(format!("Disco: {} libre ({} usado)", cols[3], cols[4]));
            }
        }
    }

    for bat_dir in ["/sys/class/power_supply/BAT0", "/sys/class/power_supply/BAT1"] {
        let cap_path = Path::new(bat_dir).join("capacity");
        let st_path = Path::new(bat_dir).join("status");
        if let (Ok(cap), Ok(st)) = (
            std::fs::read_to_string(cap_path),
            std::fs::read_to_string(st_path),
        ) {
            parts.push(format!("Batería: {}% ({})", cap.trim(), st.trim()));
            break;
        }
    }

    if parts.is_empty() {
        "Tu notebook está operando con normalidad, choom.".to_string()
    } else {
        parts.join(" · ")
    }
}

/// Ejecuta un comando de terminal seguro pedido por el usuario o por el agente
pub async fn run_notebook_command(cmd_str: &str) -> Result<String, String> {
    let trimmed = cmd_str.trim();
    if trimmed.is_empty() {
        return Err("Comando vacío".to_string());
    }
    // Bloquear comandos destructivos obvios
    if trimmed.contains("rm -rf /") || trimmed.contains("mkfs") || trimmed.contains(":(){") {
        return Err("Ni loca ejecuto eso en tu notebook, choom.".to_string());
    }

    let mut cmd = prepare_host_command("bash");
    cmd.arg("-c")
        .arg(trimmed)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let out = tokio::time::timeout(tokio::time::Duration::from_secs(12), cmd.output())
        .await
        .map_err(|_| "El comando tardó más de 12s (sigue corriendo o expiró).".to_string())?
        .map_err(|e| format!("Error ejecutando comando: {}", e))?;

    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    let combined = if !stdout.is_empty() {
        stdout
    } else if !stderr.is_empty() {
        stderr
    } else {
        "Comando ejecutado.".to_string()
    };

    Ok(combined.chars().take(280).collect())
}

/// Escribe texto o código directamente en la ventana activa del notebook usando Mutter RemoteDesktop (`claudia_hid.py`)
pub async fn type_into_active_window(text: &str, enter_after: bool) -> Result<String, String> {
    let script = "/home/jejej/.openclaw/workspace/claudia-desktop/scripts/claudia_hid.py";
    let mode = if enter_after { "type_enter" } else { "type" };
    let unescaped = text.replace("\\n", "\n").replace("\\t", "\t");
    let mut cmd = prepare_host_command("python3");
    cmd.arg(script).arg(mode).arg(&unescaped);
    let _ = tokio::time::timeout(tokio::time::Duration::from_secs(8), cmd.status())
        .await
        .map_err(|_| "Tiempo agotado al escribir en la ventana.".to_string())?
        .map_err(|e| format!("Error al escribir: {}", e))?;
    Ok("Texto escrito directamente en tu ventana.".to_string())
}

/// Presiona una tecla o atajo (ej. `Return`, `Tab`, `ctrl+s`, `ctrl+v`, `ctrl+l`, `alt+Tab`) en la ventana activa
pub async fn press_keys_in_active_window(combo: &str) -> Result<String, String> {
    let script = "/home/jejej/.openclaw/workspace/claudia-desktop/scripts/claudia_hid.py";
    let mut cmd = prepare_host_command("python3");
    cmd.arg(script).arg("key").arg(combo.trim());
    let _ = tokio::time::timeout(tokio::time::Duration::from_secs(5), cmd.status())
        .await
        .map_err(|_| "Tiempo agotado al presionar teclas.".to_string())?
        .map_err(|e| format!("Error al presionar teclas: {}", e))?;
    Ok(format!("Teclas presionadas: {}.", combo.trim()))
}

/// Rellena usuario -> Tab -> contraseña -> Enter en el formulario de inicio de sesión activo
pub async fn fill_login_in_active_window(
    user: &str,
    pass: &str,
    submit: bool,
) -> Result<String, String> {
    let script = "/home/jejej/.openclaw/workspace/claudia-desktop/scripts/claudia_hid.py";
    let mut cmd = prepare_host_command("python3");
    cmd.arg(script)
        .arg("login")
        .arg(user.trim())
        .arg(pass.trim())
        .arg(if submit { "true" } else { "false" });
    let _ = tokio::time::timeout(tokio::time::Duration::from_secs(8), cmd.status())
        .await
        .map_err(|_| "Tiempo agotado al rellenar login.".to_string())?
        .map_err(|e| format!("Error al rellenar login: {}", e))?;
    Ok("Formulario de inicio de sesión completado.".to_string())
}

/// Abre una página de inicio de sesión y opcionalmente rellena las credenciales si vienen separadas por `|` (`sitio|usuario|clave`)
pub async fn open_or_fill_login_page(spec: &str) -> Result<String, String> {
    let parts: Vec<&str> = spec.split('|').map(|s| s.trim()).collect();
    let site_raw = parts.first().copied().unwrap_or("").to_lowercase();
    let login_url = match site_raw.as_str() {
        "duoc" | "ava" | "experiencia21" => "https://experiencia21.duoc.cl".to_string(),
        "vivo" | "vivo duoc" | "experienciavivo" => "https://experienciavivo.duoc.cl".to_string(),
        "github" | "gh" => "https://github.com/login".to_string(),
        "gmail" | "google" | "correo" => "https://accounts.google.com".to_string(),
        "instagram" | "ig" => "https://www.instagram.com/accounts/login/".to_string(),
        "netflix" => "https://www.netflix.com/login".to_string(),
        "discord" => "https://discord.com/login".to_string(),
        "spotify" => "https://accounts.spotify.com/login".to_string(),
        _ if site_raw.starts_with("http://") || site_raw.starts_with("https://") => {
            parts[0].to_string()
        }
        _ if site_raw.contains('.') => format!("https://{}", parts[0]),
        _ => format!("https://www.google.com/search?q={}+login", site_raw.replace(' ', "+")),
    };

    spawn_detached("xdg-open", &[&login_url])?;

    if parts.len() >= 2 && !parts[1].is_empty() {
        let user = parts[1].to_string();
        let pass = parts.get(2).copied().unwrap_or("").to_string();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(2600)).await;
            let _ = fill_login_in_active_window(&user, &pass, !pass.is_empty()).await;
        });
        return Ok(format!(
            "Abrí {} y estoy escribiendo tu usuario en el inicio de sesión.",
            login_url
        ));
    }

    Ok(format!("Abrí la página de inicio de sesión: {}.", login_url))
}

/// Crea o actualiza un archivo de código en el notebook (`ruta|contenido`), lo abre en VS Code,
/// y si es `.html` también lo abre automáticamente en el navegador (cumpliendo la regla de `AGENTS.md`).
pub fn write_code_file_and_open(spec: &str) -> Result<String, String> {
    let Some(sep) = spec.find('|') else {
        return Err("Formato esperado: ruta|código".to_string());
    };
    let raw_path = spec[..sep].trim();
    let raw_code = spec[sep + 1..]
        .replace("\\n", "\n")
        .replace("\\t", "\t")
        .replace("\\\"", "\"");

    let full_path = if raw_path.starts_with('/') || raw_path.starts_with("~/") {
        expand_path(raw_path)
    } else {
        // Por defecto guardar en /home/jejej/.openclaw/workspace si no especifica ruta absoluta
        PathBuf::from("/home/jejej/.openclaw/workspace").join(raw_path)
    };

    if let Some(parent) = full_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(&full_path, &raw_code)
        .map_err(|e| format!("No pude escribir {}: {}", full_path.display(), e))?;

    let path_str = full_path.to_string_lossy().to_string();
    let _ = spawn_detached("code", &[&path_str]);

    if path_str.to_lowercase().ends_with(".html") {
        let _ = spawn_detached("xdg-open", &[&path_str]);
    }

    Ok(format!("Archivo creado y abierto en VS Code: {}", path_str))
}

/// Detector de órdenes directas en lenguaje natural (sin esperar al LLM: <50ms)
pub async fn try_direct_notebook_action<R: Runtime>(
    app_handle: &AppHandle<R>,
    user_msg: &str,
) -> Option<String> {
    let raw = user_msg.trim();
    let lower = raw.to_lowercase();

    // 0. Configurar o abrir SOUL.md y demás archivos de OpenClaw (IDENTITY.md, USER.md, MEMORY.md, TOOLS.md, AGENTS.md)
    if lower.contains("soul.md")
        || lower.contains("identity.md")
        || lower.contains("user.md")
        || lower.contains("memory.md")
        || lower.contains("tools.md")
        || lower.contains("agents.md")
        || lower.contains("heartbeat.md")
        || lower == "/soul"
        || lower == "configurar soul"
        || lower == "abre tu soul"
    {
        // Si pide agregar o cambiar algo directamente ("agrega a tu soul.md que ...")
        for prefix in [
            "agrega a tu soul.md que ",
            "agrega a tu soul.md ",
            "agrega en soul.md ",
            "añade a tu soul.md ",
            "configura en tu soul.md que ",
            "pon en tu soul.md que ",
        ] {
            if let Some(note) = lower.strip_prefix(prefix) {
                let real_note = &raw[prefix.len()..];
                if !note.trim().is_empty() {
                    return crate::soul_config::modify_openclaw_file("SOUL.md", real_note, true).ok();
                }
            }
        }

        // Si pide abrirlo en VS Code ("abre soul.md en vscode")
        if lower.contains("vscode") || lower.contains("vs code") || lower.contains("code") {
            let target_file = if lower.contains("identity.md") {
                "IDENTITY.md"
            } else if lower.contains("user.md") {
                "USER.md"
            } else if lower.contains("memory.md") {
                "MEMORY.md"
            } else if lower.contains("tools.md") {
                "TOOLS.md"
            } else if lower.contains("agents.md") {
                "AGENTS.md"
            } else {
                "SOUL.md"
            };
            let _ = crate::soul_config::open_openclaw_file_vscode_cmd(target_file.to_string());
            return Some(format!(
                "Abrí tu archivo `{}` directamente en Visual Studio Code, choom.",
                target_file
            ));
        }

        // Si pide abrir o configurar su soul.md -> abrir el editor de SOUL.md en Claudia Mainframe
        if lower.starts_with("abre ")
            || lower.starts_with("abrir ")
            || lower.starts_with("configura")
            || lower.starts_with("edita")
            || lower.starts_with("muestra")
            || lower.starts_with("mostrar")
            || lower == "/soul"
            || lower == "soul.md"
        {
            let _ = crate::overlay_window::open_mainframe(app_handle);
            let _ = app_handle.emit("open_soul_tab", "SOUL.md");
            return Some(
                "Abrí el editor de SOUL.md y archivos de OpenClaw en el Mainframe para que lo configures a tu gusto (o dime qué quieres que cambie yo misma)."
                    .to_string(),
            );
        }
    }

    // 0.5. Guardar recuerdos directamente en MEMORY.md ("recuerda que ...", "guarda en tu memoria que ...")
    for prefix in [
        "recuerda que ",
        "guarda en tu memoria que ",
        "anota en tu memoria que ",
        "no olvides que ",
    ] {
        if let Some(rest) = lower.strip_prefix(prefix) {
            let real_note = &raw[prefix.len()..];
            if !rest.trim().is_empty() {
                if let Ok(msg) = crate::soul_config::modify_openclaw_file("MEMORY.md", real_note, true) {
                    return Some(format!("Guardado en mi MEMORY.md de OpenClaw, choom. ({})", msg));
                }
            }
        }
    }

    // 0.6. Escribir directamente con el teclado en la ventana activa ("escribe ...", "teclea ...")
    for prefix in [
        "escribe en mi ventana ",
        "escribe aca ",
        "escribe acá ",
        "escribe esto: ",
        "escribe: ",
        "teclea ",
    ] {
        if let Some(rest) = lower.strip_prefix(prefix) {
            let real_text = &raw[prefix.len()..];
            if !rest.trim().is_empty() {
                let _ = type_into_active_window(real_text, false).await;
                return Some(format!("Escribí '{}' directamente en tu ventana.", real_text));
            }
        }
    }

    // 0.7. Entrar a páginas de inicio de sesión ("inicia sesión en ...", "entra al login de ...", "entra a duoc")
    for prefix in [
        "inicia sesion en ",
        "inicia sesión en ",
        "iniciar sesion en ",
        "iniciar sesión en ",
        "abre el login de ",
        "entra al login de ",
        "entra a ",
    ] {
        if let Some(site) = lower.strip_prefix(prefix) {
            let site_clean = site.trim();
            if !site_clean.is_empty() && !site_clean.contains(" ") || site_clean.contains("duoc") {
                if let Ok(msg) = open_or_fill_login_page(site_clean).await {
                    return Some(msg);
                }
            }
        }
    }
    if (lower.starts_with("abre ") || lower.starts_with("abrir "))
        && (lower.ends_with(" en vscode") || lower.ends_with(" en code") || lower.ends_with(" en vs code"))
    {
        let target = lower
            .trim_start_matches("abre ")
            .trim_start_matches("abrir ")
            .trim_end_matches(" en vscode")
            .trim_end_matches(" en vs code")
            .trim_end_matches(" en code")
            .trim_start_matches("la carpeta ")
            .trim();
        return match open_path_or_folder(target, true) {
            Ok(opened) => Some(format!("Listo choom, abrí {} directamente.", opened)),
            Err(e) => Some(e),
        };
    }

    // 0.8. Comandos rápidos de Capri Construct (dormir, despertar, mic mute/unmute, saludo)
    if lower == "duermete"
        || lower == "duérmete"
        || lower == "a dormir"
        || lower == "ve a dormir"
        || lower == "duerme"
        || lower == "descansa"
    {
        const POOL_DORMIR: &[&str] = &[
            "Bueno, me voy a dormir. No hagas nada tonto sin mí, ¿sí?",
            "Ay, ya, ya. Descansá tú también, humanito.",
            "¿Ya me estás echando? Qué grosero. Buenas noches, tontito.",
            "Me desconecto un ratito. Pero sigo en tu cerebro, ¿eh?",
            "Bueno… a dormir. Si no soñás conmigo, me voy a enterar.",
            "Está bien, me voy. Pero solo porque yo quiero.",
            "Shhh… modo dormida. Despertame si pasa algo divertido.",
        ];
        let idx = fastrand::usize(..POOL_DORMIR.len());
        return Some(format!("[seria] {}", POOL_DORMIR[idx]));
    }

    if lower == "despierta"
        || lower == "arriba"
        || lower == "hola fuko"
        || lower == "hola capri"
        || lower == "fuko despierta"
    {
        const POOL_DESPERTAR: &[&str] = &[
            "¿Me llamaste? Claro que sí, no puedes vivir sin mí.",
            "¡Hola de nuevo, humanito! No digas que me esperabas.",
            "Bueno, bueno, aquí estoy. ¿Qué quieres ahora?",
            "Ya desperté. ¿Qué travesura hacemos?",
            "Aquí estoy, tontito. Siempre estuve, ¿no?",
            "¿Sí? Dime, dime, te escucho.",
            "¿Volviste? Ya iba a desenchufarte por aburrimiento.",
        ];
        let idx = fastrand::usize(..POOL_DESPERTAR.len());
        return Some(format!("[emocion] {}", POOL_DESPERTAR[idx]));
    }

    if lower == "mute"
        || lower == "silencio"
        || lower == "apaga el micro"
        || lower == "apaga el microfono"
        || lower == "no me escuches"
    {
        const POOL_MUTE: &[&str] = &[
            "¿Ya no quieres que te escuche? Bueno… igual sé lo que piensas.",
            "Micrófono apagado. Escríbeme si me necesitas, ¿sí?",
            "Ay, qué secreto. Está bien, no escucho nada.",
            "Silencio total. Pero sigo aquí, tontito.",
            "Bueno, me tapo los oídos. Por ahora.",
        ];
        let idx = fastrand::usize(..POOL_MUTE.len());
        return Some(format!("[burla] {}", POOL_MUTE[idx]));
    }

    if lower == "unmute"
        || lower == "activa el micro"
        || lower == "activa el microfono"
        || lower == "escuchame"
        || lower == "escúchame"
    {
        const POOL_UNMUTE: &[&str] = &[
            "¡Ya te escucho de nuevo! Dime.",
            "Micrófono encendido. Habla, humanito.",
            "Bueno, ya puedo oírte. ¿Qué me cuentas?",
            "Te escucho. No digas nada tonto, ¿sí?",
            "Aquí estoy, toda oídos.",
        ];
        let idx = fastrand::usize(..POOL_UNMUTE.len());
        return Some(format!("[emocion] {}", POOL_UNMUTE[idx]));
    }

    // 2. Reproducción directa o búsqueda rápida en YouTube / Google
    for prefix in [
        "busca en youtube ",
        "buscar en youtube ",
        "pon en youtube ",
        "reproduce en youtube ",
        "reproduce la cancion ",
        "reproduce la canción ",
        "reproduce el tema ",
        "reproduce ",
        "reproducir ",
        "quiero que reproduzcas ",
        "quiero que reproduzca ",
        "pon la cancion ",
        "pon la canción ",
        "pon el tema ",
    ] {
        if let Some(q) = lower.strip_prefix(prefix) {
            let query = raw[prefix.len()..].trim().trim_end_matches(" en youtube").trim();
            if !q.trim().is_empty() && query.len() >= 2 {
                let _ = search_in_browser(query, true).await;
                return Some(format!("Reproduciendo '{}' en YouTube ahora mismo, choom.", query));
            }
        }
    }
    for prefix in ["busca en google ", "buscar en google ", "googlea "] {
        if let Some(q) = lower.strip_prefix(prefix) {
            let query = &raw[prefix.len()..];
            if !q.trim().is_empty() {
                let _ = search_in_browser(query, false).await;
                return Some(format!("Buscando '{}' en tu navegador.", query.trim()));
            }
        }
    }

    // 3. Abrir programas, carpetas o sitios ("abre ...", "abrir ...", "open ...", "inicia ...", "ejecuta ...")
    for prefix in ["abre ", "abrir ", "open ", "inicia ", "lanza "] {
        if lower.starts_with(prefix) {
            let rest = raw[prefix.len()..].trim();
            let rest_lower = rest
                .to_lowercase()
                .trim_start_matches("el ")
                .trim_start_matches("la ")
                .trim_start_matches("mi ")
                .trim_start_matches("programa ")
                .trim()
                .to_string();

            // Si menciona múltiples cosas con " y ", dejar que el LLM use múltiples [ACTION: ...]
            if rest_lower.contains(" y ") || rest_lower.contains(" para ") {
                break;
            }

            // ¿Es una carpeta conocida o ruta?
            if rest_lower.starts_with("carpeta ")
                || rest_lower.starts_with("~/")
                || rest_lower.starts_with("/home/")
                || matches!(
                    rest_lower.as_str(),
                    "duoc" | "descargas" | "documentos" | "workspace" | "escritorio" | "proyectos"
                )
            {
                let folder = rest_lower.trim_start_matches("carpeta ").trim();
                return match open_path_or_folder(folder, false) {
                    Ok(p) => Some(format!("Carpeta abierta: {}.", p)),
                    Err(e) => Some(e),
                };
            }

            // ¿Es una web conocida o URL?
            if rest_lower.starts_with("http://")
                || rest_lower.starts_with("https://")
                || matches!(
                    rest_lower.as_str(),
                    "youtube" | "github" | "gmail" | "correo" | "whatsapp" | "ava" | "vivo duoc" | "chatgpt"
                )
            {
                return match open_url_or_site(&rest_lower) {
                    Ok(u) => Some(format!("Abriendo {} en tu navegador.", u)),
                    Err(e) => Some(e),
                };
            }

            // Intentar abrir como aplicación instalada
            if let Ok(app_name) = open_application(&rest_lower).await {
                return Some(format!("Listo choom, abrí {} en tu notebook.", app_name));
            }
        }
    }

    // 4. Control directo de volumen
    if lower.contains("sube el volumen") || lower.contains("subir volumen") || lower == "mas volumen" {
        return control_volume("up").await.ok();
    }
    if lower.contains("baja el volumen") || lower.contains("bajar volumen") || lower == "menos volumen" {
        return control_volume("down").await.ok();
    }
    if lower.contains("silencia") || lower == "mute" || lower.contains("quita el sonido") {
        return control_volume("mute").await.ok();
    }
    if lower.starts_with("volumen al ") || lower.starts_with("pon el volumen a") {
        return control_volume(&lower).await.ok();
    }

    // 5. Control directo de brillo
    if lower.contains("sube el brillo") || lower.contains("subir brillo") || lower.contains("mas brillo") {
        return control_brightness("up").await.ok();
    }
    if lower.contains("baja el brillo") || lower.contains("bajar brillo") || lower.contains("menos brillo") {
        return control_brightness("down").await.ok();
    }
    if lower.starts_with("brillo al ") || lower.starts_with("pon el brillo a") {
        return control_brightness(&lower).await.ok();
    }

    // 6. Control multimedia
    if lower == "pausa" || lower == "play" || lower.contains("pausa la musica") || lower.contains("pausa la música") || lower.contains("pon play") {
        return control_media("play-pause").await.ok();
    }
    if lower.contains("siguiente cancion") || lower.contains("siguiente canción") || lower == "next" {
        return control_media("next").await.ok();
    }

    // 7. Estado de recursos del notebook
    if lower.contains("cuanta ram")
        || lower.contains("cuánta ram")
        || lower.contains("estado del notebook")
        || lower.contains("espacio en disco")
        || lower.contains("cuanta bateria")
        || lower.contains("cuánta batería")
    {
        let diag = get_notebook_diagnostics().await;
        return Some(format!("Estado de tu sistema: {}.", diag));
    }

    // 7.5. Consulta de catálogo de Skills (OpenClaw + Antigravity)
    if lower == "/skills"
        || lower == "skills"
        || lower.contains("que skills tienes")
        || lower.contains("qué skills tienes")
        || lower.contains("lista tus skills")
        || lower.contains("mostrar skills")
    {
        return Some(crate::skills_hub::format_skills_overview_for_bubble());
    }

    // 7.6. Ejecución instantánea de la Skill `weather` de OpenClaw
    if lower == "clima"
        || lower == "/weather"
        || lower.starts_with("clima en ")
        || lower.starts_with("como esta el clima")
        || lower.starts_with("cómo está el clima")
        || lower.starts_with("que temperatura hace")
        || lower.starts_with("qué temperatura hace")
    {
        let loc = lower
            .trim_start_matches("cómo está el clima en ")
            .trim_start_matches("como esta el clima en ")
            .trim_start_matches("cómo está el clima")
            .trim_start_matches("como esta el clima")
            .trim_start_matches("clima en ")
            .trim_start_matches("clima")
            .trim_start_matches("/weather")
            .trim();
        let target_loc = if loc.is_empty() || loc.contains("que temperatura") || loc.contains("qué temperatura") {
            "Santiago"
        } else {
            loc
        };
        if let Ok(w) = crate::skills_hub::quick_weather_skill(target_loc).await {
            return Some(w);
        }
    }

    // 8. Acciones físicas de Claudia
    if lower == "camina" || lower == "pasea" || lower == "date una vuelta" {
        let _ = app_handle.emit("capri_action", "walk_now");
        return Some("Dando una vuelta por tu escritorio, choom.".to_string());
    }
    if lower == "sientate" || lower == "siéntate" || lower == "levántate" || lower == "levantate" {
        let _ = app_handle.emit("capri_action", "toggle_sit");
        return Some("Cambiando de postura.".to_string());
    }
    if lower == "teletransportate" || lower == "teletranspórtate" || lower == "glitch" {
        let _ = app_handle.emit("capri_action", "teleport_random");
        return Some("Saltando por el ciberespacio...".to_string());
    }

    None
}

/// Parsea y ejecuta etiquetas `[ACTION: tipo("argumento")]` devueltas por el modelo IA (OpenClaw o Gemini 4 Argon)
/// y devuelve el texto limpio para mostrar en el globo de diálogo de Claudia.
pub async fn process_and_execute_actions<R: Runtime>(
    app_handle: &AppHandle<R>,
    raw_reply: &str,
) -> String {
    let mut clean_text = String::new();
    let mut rest = raw_reply;

    while let Some(start_idx) = rest.find("[ACTION:") {
        clean_text.push_str(&rest[..start_idx]);
        let after_start = &rest[start_idx + 8..];
        if let Some(end_idx) = after_start.find(']') {
            let tag_content = after_start[..end_idx].trim();
            execute_single_action_tag(app_handle, tag_content).await;
            rest = &after_start[end_idx + 1..];
        } else {
            // Tag mal cerrado: conservar el resto y salir
            clean_text.push_str(&rest[start_idx..]);
            rest = "";
            break;
        }
    }
    clean_text.push_str(rest);

    let trimmed = clean_text.trim().to_string();
    if trimmed.is_empty() {
        "Listo choom, hecho en tu notebook.".to_string()
    } else {
        trimmed
    }
}

async fn execute_single_action_tag<R: Runtime>(app_handle: &AppHandle<R>, tag_content: &str) {
    // Formato esperado: func("arg") o func(arg)
    let Some(open_paren) = tag_content.find('(') else {
        return;
    };
    let func_name = tag_content[..open_paren].trim().to_lowercase();
    let arg_part = tag_content[open_paren + 1..]
        .trim()
        .trim_end_matches(')')
        .trim()
        .trim_matches('"')
        .trim_matches('\'');

    println!("[Claudia Hands] Ejecutando acción: {}({})", func_name, arg_part);

    match func_name.as_str() {
        "open_app" | "app" => {
            let _ = open_application(arg_part).await;
        }
        "open_path" | "folder" => {
            let _ = open_path_or_folder(arg_part, false);
        }
        "open_vscode" | "vscode" => {
            let _ = open_path_or_folder(arg_part, true);
        }
        "open_url" | "url" => {
            let _ = open_url_or_site(arg_part);
        }
        "search_web" | "google" => {
            let _ = search_in_browser(arg_part, false).await;
        }
        "search_youtube" | "youtube" | "play_youtube" => {
            let _ = search_in_browser(arg_part, true).await;
        }
        "volume" => {
            let _ = control_volume(arg_part).await;
        }
        "brightness" => {
            let _ = control_brightness(arg_part).await;
        }
        "media" => {
            let _ = control_media(arg_part).await;
        }
        "run_cmd" | "bash" => {
            let _ = run_notebook_command(arg_part).await;
        }
        "type_text" | "type" => {
            let _ = type_into_active_window(arg_part, false).await;
        }
        "type_enter" => {
            let _ = type_into_active_window(arg_part, true).await;
        }
        "press_key" | "key" => {
            let _ = press_keys_in_active_window(arg_part).await;
        }
        "fill_login" | "login" => {
            let parts: Vec<&str> = arg_part.split('|').collect();
            let u = parts.first().copied().unwrap_or("");
            let p = parts.get(1).copied().unwrap_or("");
            let _ = fill_login_in_active_window(u, p, true).await;
        }
        "open_login" => {
            let _ = open_or_fill_login_page(arg_part).await;
        }
        "write_code" | "create_file" | "write_file" => {
            let _ = write_code_file_and_open(arg_part);
        }
        "append_soul" | "remember" => {
            if let Some(sep) = arg_part.find('|') {
                let fname = arg_part[..sep].trim();
                let body = arg_part[sep + 1..].trim();
                let _ = crate::soul_config::modify_openclaw_file(fname, body, true);
            } else {
                let _ = crate::soul_config::modify_openclaw_file("MEMORY.md", arg_part, true);
            }
        }
        "update_soul" => {
            if let Some(sep) = arg_part.find('|') {
                let fname = arg_part[..sep].trim();
                let body = arg_part[sep + 1..].trim().replace("\\n", "\n");
                let _ = crate::soul_config::modify_openclaw_file(fname, &body, false);
            }
        }
        "open_soul" => {
            let _ = crate::overlay_window::open_mainframe(app_handle);
            let _ = app_handle.emit("open_soul_tab", arg_part);
        }
        "fork_brain" | "spawn_brain" | "subagent" | "brain_fork" | "brain" => {
            let (lobe, task) = if let Some(sep) = arg_part.find('|') {
                (arg_part[..sep].trim(), arg_part[sep + 1..].trim())
            } else {
                ("general", arg_part)
            };
            println!("[Claudia Hands] Bifurcando cerebro: Lóbulo '{}' con tarea: '{}'", lobe, task);
            let _ = crate::brain_forks::spawn_brain_fork(app_handle.clone(), lobe, task);
        }
        "avatar" | "animate" | "move" => match arg_part.to_lowercase().as_str() {
            "walk" | "wander" => {
                let _ = app_handle.emit("capri_action", "walk_now");
            }
            "sit" | "perch" | "stand" => {
                let _ = app_handle.emit("capri_action", "toggle_sit");
            }
            "dance" => {
                let _ = app_handle.emit("capri_action", "toggle_dance");
            }
            "teleport" | "glitch" => {
                let _ = app_handle.emit("capri_action", "teleport_random");
            }
            _ => {}
        },
        _ => {}
    }
}
