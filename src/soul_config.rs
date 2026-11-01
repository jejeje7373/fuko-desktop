// src/soul_config.rs
// Gestor en vivo de los archivos de Alma, Identidad, Memoria y Configuración de OpenClaw
// (`/home/jejej/.openclaw/workspace/{SOUL.md, IDENTITY.md, USER.md, MEMORY.md, AGENTS.md, TOOLS.md, HEARTBEAT.md, DREAMS.md}`).
// Permite tanto al usuario (desde Mainframe / VS Code) como a la propia Claudia (por voz o texto)
// leer y configurar su SOUL.md y demás archivos de OpenClaw en tiempo real.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::Command;

const WORKSPACE_DIR: &str = "/home/jejej/.openclaw/workspace";

pub const OPENCLAW_FILES: &[(&str, &str, &str)] = &[
    (
        "SOUL.md",
        "🧬 SOUL.md",
        "Alma, personalidad, límites y esencia de Fuko en OpenClaw",
    ),
    (
        "IDENTITY.md",
        "🐱 IDENTITY.md",
        "Nombre, criatura, vibe, emoji y avatar",
    ),
    (
        "USER.md",
        "👤 USER.md",
        "Perfil de Aym, ramos de Duoc y preferencias personales",
    ),
    (
        "MEMORY.md",
        "🧠 MEMORY.md",
        "Memoria curada a largo plazo (recuerdos, fechas de pruebas, notas)",
    ),
    (
        "TOOLS.md",
        "🛠️ TOOLS.md",
        "Notas locales del notebook, accesos, sitios frecuentes y atajos",
    ),
    (
        "AGENTS.md",
        "📋 AGENTS.md",
        "Reglas operativas del workspace, reglas HTML y flujo de trabajo",
    ),
    (
        "HEARTBEAT.md",
        "💓 HEARTBEAT.md",
        "Lista de chequeos proactivos, recordatorios y tareas periódicas",
    ),
    (
        "DREAMS.md",
        "🌙 DREAMS.md",
        "Bitácora de sueños y reflexiones autónomas de OpenClaw",
    ),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenClawFileEntry {
    pub name: String,
    pub label: String,
    pub description: String,
    pub path: String,
    pub exists: bool,
    pub size_bytes: u64,
}

fn resolve_openclaw_file(name: &str) -> Result<PathBuf, String> {
    let clean = name.trim().trim_matches('/').trim_matches('\\');
    if clean.contains("..") || clean.contains('/') || clean.contains('\\') {
        return Err("Nombre de archivo inválido.".to_string());
    }
    // Buscar coincidencia insensible a mayúsculas con los archivos conocidos, o permitir .md en el workspace
    for (canonical, _, _) in OPENCLAW_FILES {
        if canonical.eq_ignore_ascii_case(clean) {
            return Ok(Path::new(WORKSPACE_DIR).join(canonical));
        }
    }
    if clean.ends_with(".md") {
        return Ok(Path::new(WORKSPACE_DIR).join(clean));
    }
    Err(format!("Archivo OpenClaw no reconocido: {}", clean))
}

/// Asegura que `MEMORY.md`, `TOOLS.md` y `HEARTBEAT.md` existan en `/home/jejej/.openclaw/workspace`
/// sin sobrescribir jamás los archivos existentes (`SOUL.md`, `IDENTITY.md`, `USER.md`, `AGENTS.md`, etc.).
pub fn ensure_default_openclaw_files() {
    let ws = Path::new(WORKSPACE_DIR);
    let _ = std::fs::create_dir_all(ws);

    let memory_path = ws.join("MEMORY.md");
    if !memory_path.exists() {
        let _ = std::fs::write(
            &memory_path,
            "# MEMORY.md - Long-Term Memory\n\n\
             ## Recuerdos Clave de Aym y Fuko\n\
             - Aym estudia en Duoc UC y usa este notebook Linux (Ubuntu GNOME) para programar y estudiar.\n\
             - Fuko (🐱) vive en el escritorio con apariencia y físicas estilo Capri Desktop, conectada a OpenClaw, Gemini 4 Argon y sus 44 Skills.\n\
             - Tiene control directo del micrófono (`faster-whisper`), voz (`Piper TTS`) y teclado/mouse nativo para programar y entrar a páginas.\n",
        );
    }

    let tools_path = ws.join("TOOLS.md");
    if !tools_path.exists() {
        let _ = std::fs::write(
            &tools_path,
            "# TOOLS.md - Local Notes & Shortcuts\n\n\
             ## Sitios de Inicio de Sesión Frecuentes\n\
             - **Duoc AVA / Experiencia21:** https://experiencia21.duoc.cl\n\
             - **Vivo Duoc:** https://experienciavivo.duoc.cl\n\
             - **GitHub Login:** https://github.com/login\n\
             - **Gmail / Google:** https://accounts.google.com\n\n\
             ## Entorno del Notebook\n\
             - **Carpeta de estudios:** `/home/jejej/Duoc`\n\
             - **Workspace OpenClaw:** `/home/jejej/.openclaw/workspace`\n\
             - **Editor principal:** Visual Studio Code (`code`) y Antigravity (`antigravity`)\n\
             - **Navegador principal:** Google Chrome (`google-chrome`)\n",
        );
    }

    let heartbeat_path = ws.join("HEARTBEAT.md");
    if !heartbeat_path.exists() {
        let _ = std::fs::write(
            &heartbeat_path,
            "# HEARTBEAT.md - Proactive Checks & Reminders\n\n\
             - [ ] Revisar pendientes de ramos en `/home/jejej/Duoc`\n\
             - [ ] Recordar tomar agua y descansar la vista en sesiones largas de programación\n",
        );
    }
}

/// Construye el bloque en vivo con el contenido actual de `SOUL.md`, `IDENTITY.md`, `USER.md`, `MEMORY.md` y `TOOLS.md`
/// para inyectarlo dinámicamente en el prompt de Claudia. Así cualquier cambio en `SOUL.md` surte efecto al instante.
pub fn build_live_soul_context() -> String {
    let ws = Path::new(WORKSPACE_DIR);
    let mut out = String::from("\n=== ARCHIVOS DE ALMA Y MEMORIA OPENCLAW (EN VIVO) ===\n");

    for (fname, max_chars) in [
        ("SOUL.md", 1600usize),
        ("IDENTITY.md", 500usize),
        ("USER.md", 800usize),
        ("MEMORY.md", 1200usize),
        ("TOOLS.md", 900usize),
        ("HEARTBEAT.md", 500usize),
    ] {
        let p = ws.join(fname);
        if let Ok(content) = std::fs::read_to_string(&p) {
            let trimmed = content.trim();
            if !trimmed.is_empty() {
                let clipped: String = trimmed.chars().take(max_chars).collect();
                out.push_str(&format!("\n--- {} ---\n{}\n", fname, clipped));
            }
        }
    }
    out
}

/// Permite a Claudia actualizar o agregar notas a cualquiera de sus archivos de OpenClaw (`SOUL.md`, `MEMORY.md`, etc.)
pub fn modify_openclaw_file(file_name: &str, content: &str, append: bool) -> Result<String, String> {
    let path = resolve_openclaw_file(file_name)?;
    let canon_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(file_name)
        .to_string();

    if append {
        let mut current = std::fs::read_to_string(&path).unwrap_or_default();
        if !current.ends_with('\n') && !current.is_empty() {
            current.push('\n');
        }
        let line = content.trim();
        if line.starts_with('-') || line.starts_with('#') {
            current.push_str(&format!("{}\n", line));
        } else {
            current.push_str(&format!("- {}\n", line));
        }
        std::fs::write(&path, current)
            .map_err(|e| format!("No pude guardar {}: {}", canon_name, e))?;
        Ok(format!("Agregado en `{}`: {}", canon_name, line))
    } else {
        std::fs::write(&path, content)
            .map_err(|e| format!("No pude guardar {}: {}", canon_name, e))?;
        Ok(format!("Archivo `{}` actualizado correctamente.", canon_name))
    }
}

#[tauri::command]
pub fn list_openclaw_files_cmd() -> Result<Vec<OpenClawFileEntry>, String> {
    ensure_default_openclaw_files();
    let ws = Path::new(WORKSPACE_DIR);
    let mut list = Vec::new();
    for (name, label, desc) in OPENCLAW_FILES {
        let p = ws.join(name);
        let meta = std::fs::metadata(&p).ok();
        list.push(OpenClawFileEntry {
            name: (*name).to_string(),
            label: (*label).to_string(),
            description: (*desc).to_string(),
            path: p.to_string_lossy().to_string(),
            exists: meta.is_some(),
            size_bytes: meta.map(|m| m.len()).unwrap_or(0),
        });
    }
    Ok(list)
}

#[tauri::command]
pub fn read_openclaw_file_cmd(name: String) -> Result<String, String> {
    ensure_default_openclaw_files();
    let path = resolve_openclaw_file(&name)?;
    if !path.exists() {
        return Ok(format!("# {}\n\n", name));
    }
    std::fs::read_to_string(&path).map_err(|e| format!("Error leyendo {}: {}", name, e))
}

#[tauri::command]
pub fn save_openclaw_file_cmd(name: String, content: String) -> Result<String, String> {
    modify_openclaw_file(&name, &content, false)
}

#[tauri::command]
pub fn open_openclaw_file_vscode_cmd(name: String) -> Result<String, String> {
    let path = resolve_openclaw_file(&name)?;
    let path_str = path.to_string_lossy().to_string();
    let mut cmd = std::process::Command::new("code");
    cmd.env_remove("SNAP")
        .env_remove("SNAP_NAME")
        .env_remove("SNAP_LIBRARY_PATH")
        .env_remove("GTK_MODULES")
        .env_remove("GDK_BACKEND")
        .arg(&path_str)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    cmd.spawn()
        .map_err(|e| format!("No pude abrir VS Code: {}", e))?;
    Ok(path_str)
}
