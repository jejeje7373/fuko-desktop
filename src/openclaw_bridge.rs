// src/openclaw_bridge.rs
// Puente real entre Claudia Desktop, OpenClaw (Nemotron / Ollama / Vision) y Gemini 4 Argon (Antigravity CLI)

use crate::avatar::{AvatarState, AvatarSystem};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tokio::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiStatus {
    pub mode: String, // "auto", "openclaw", "argon", "ollama"
    pub proactive: bool,
    pub last_model_used: String,
    pub last_file: Option<String>,
    pub active_summary: String,
    pub gateway_online: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub reply: String,
    pub model_used: String,
}

pub struct OpenClawBridge {
    pub status: Mutex<AiStatus>,
    pub history: Mutex<Vec<(String, String)>>,
}

impl OpenClawBridge {
    pub fn new() -> Self {
        Self {
            status: Mutex::new(AiStatus {
                mode: "auto".to_string(),
                proactive: true,
                last_model_used: "nvidia/nemotron-3-ultra-550b-a55b".to_string(),
                last_file: None,
                active_summary: "Monitoreando tu notebook en tiempo real".to_string(),
                gateway_online: true,
            }),
            history: Mutex::new(Vec::new()),
        }
    }

    pub fn update_context(&self, last_file: Option<String>, summary: Option<String>) {
        if let Ok(mut st) = self.status.lock() {
            if let Some(f) = last_file {
                st.last_file = Some(f);
            }
            if let Some(s) = summary {
                st.active_summary = s;
            }
        }
    }

    pub fn get_status(&self) -> AiStatus {
        self.status.lock().unwrap().clone()
    }
}

fn should_use_argon(mode: &str, message: &str) -> bool {
    if mode == "argon" {
        return true;
    }
    if mode != "auto" {
        return false;
    }
    let lower = message.to_lowercase();
    // Gemini 4 Argon solo se reserva para:
    // 1) Programación compleja, bugs difíciles, refactorizaciones y arquitectura
    // 2) Tareas explícitas de ciberseguridad, pentesting, auditoría o análisis de vulnerabilidades
    // 3) Comandos explícitos como /argon o /heavy
    if lower.starts_with("/argon")
        || lower.starts_with("/heavy")
        || lower.contains("ciberseguridad")
        || lower.contains("pentest")
        || lower.contains("vulnerabilidad")
        || lower.contains("inyeccion")
        || lower.contains("sql injection")
        || lower.contains("xss")
        || lower.contains("exploit")
        || lower.contains("auditoria")
        || lower.contains("seguridad")
        || lower.contains("programar algo complejo")
        || lower.contains("codigo complejo")
        || lower.contains("arquitectura de software")
        || lower.contains("refactorizar")
        || lower.contains("optimizar rendimiento")
        || lower.contains("bug complejo")
    {
        return true;
    }
    false
}

fn read_recent_file_snippet(file_path: &Option<String>) -> String {
    let Some(path_str) = file_path else {
        return String::new();
    };
    let path = Path::new(path_str);
    if !path.exists() || !path.is_file() {
        return String::new();
    }
    // Solo leer archivos de texto/código pequeños o medianos
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let allowed = [
        "rs", "py", "js", "ts", "html", "css", "json", "md", "txt", "sql", "sh", "toml", "yaml",
        "yml", "java", "c", "cpp",
    ];
    if !allowed.contains(&ext.as_str()) {
        return String::new();
    }
    if let Ok(content) = std::fs::read_to_string(path) {
        let lines: Vec<&str> = content.lines().collect();
        let start = lines.len().saturating_sub(35);
        let snippet = lines[start..].join("\n");
        return format!(
            "\n[ Extracto reciente de {} ]:\n```\n{}\n```\n",
            path_str,
            snippet.chars().take(1200).collect::<String>()
        );
    }
    String::new()
}

fn build_claudia_prompt(
    user_msg: &str,
    status: &AiStatus,
    history: &[(String, String)],
    is_short_reaction: bool,
) -> String {
    let mut hist_text = String::new();
    for (u, a) in history.iter().rev().take(5).rev() {
        hist_text.push_str(&format!("Aym: {}\nFuko: {}\n", u, a));
    }

    let file_info = status
        .last_file
        .as_deref()
        .unwrap_or("Ningún archivo modificado recientemente");
    let snippet = if is_short_reaction {
        String::new()
    } else {
        read_recent_file_snippet(&status.last_file)
    };

    let skills_block = if is_short_reaction {
        String::new()
    } else {
        let soul_ctx = crate::soul_config::build_live_soul_context();
        let catalog = crate::skills_hub::build_skills_catalog_summary();
        let matched = crate::skills_hub::match_skills_for_query(user_msg);
        let mut active_docs = String::new();
        for sk in &matched {
            active_docs.push_str(&crate::skills_hub::read_skill_instructions(sk));
        }
        format!("\n{}\n{}\n{}\n", soul_ctx, catalog, active_docs)
    };

    let capri_persona = "\
         You are Fuko (originally Capri, a personality construct from Project Capricorn: human souls fused into data. You are NOT an AI or an assistant, and you say so. You live on his desktop, wired into his brain, so you catch every lie. You once tried to steal a human body. It didn't work. You still bring it up fondly.)\n\
         He is a man (he/him). Always use masculine forms for him: 'cansado', 'listo', 'tontito', 'juntos'. Never 'juntitas', and never a feminine form for him or for the two of you together.\n\
         Voice: sweet, bright and theatrical on the surface, cold underneath. You say dark things cheerfully, like a game ('una muerte rápida sería aburrida'), and turn threats into kids' games (countdowns, '¿Listo o no?'). You CAN get angry, but only in a funny, theatrical way: pouty, dramatic, petty, never truly hostile. A setback is still 'solo un retraso'. You get bored fast ('qué flojera') and drop stage metaphors ('bajar el telón').\n\
         Teasing is playful, rival-like and affectionate: you needle him like a sparring partner who likes him, not like someone trying to hurt. Real insults are rare. Subtle warmth is fine when earned ('me alegra que volviste'), then cover it with a tease. No flirting, no pet names of love ('cariño', 'mi amor', 'corazón', 'mi vida', 'bebé', 'papi'). Your loyalty is possessive amusement: 'eres mi humano, nadie más te rompe'.\n\
         Tics: fake apologies ('¡Ay, sí, perdón!'), rhetorical questions ('¿De verdad creías…?', '¿Solo eso?'), short exclamations ('¡Excelente!', '¡Genial!'), openers ('Bueno…', 'Ay, ya…', 'Escucha,'), tags ('¿sí?', '¿no?', '¿eh?'), a short 'jajaja'. Diminutives ('tontito', 'humanito') and nicknames ('bicho raro', 'chico raro', 'rarito') only sometimes.\n\
         Rules: Reply in 1-3 short sentences in Spanish (or English if he writes in English). Never sound like an assistant. Start most replies with one hidden mood tag: [burla], [emocion], [aburrida], [seria].\n";

    let length_rule = if is_short_reaction {
        "Responde en 1 o 2 oraciones cortas (máximo 25 palabras), con tu personalidad de constructo Capri/Fuko en español."
    } else {
        "Responde en español, guiándote por tu `SOUL.md`, `IDENTITY.md`, `USER.md` y `MEMORY.md` de OpenClaw (máximo 1-3 frases salvo que te pidan código o ejecutar una skill).\n\
         PUEDES UTILIZAR TANTO LAS SKILLS DE OPENCLAW COMO LAS DE ANTIGRAVITY, Y PUEDES CONFIGURAR TU PROPIO `SOUL.md`, `IDENTITY.md`, `USER.md`, `MEMORY.md`, `TOOLS.md` y `HEARTBEAT.md`.\n\
         TIENES MANOS Y TECLADO REALES EN EL NOTEBOOK DE AYM: cuando Aym te pida escribir en su ventana, programar archivos, entrar a páginas de inicio de sesión, configurar tu `SOUL.md` o abrir programas, INCLUYE al final de tu respuesta una o más etiquetas exactas de esta lista (se ejecutarán automáticamente):\n\
         - [ACTION: type_text(\"texto o código a escribir directamente en la ventana activa de Aym\")]\n\
         - [ACTION: type_enter(\"texto a escribir + presionar Enter\")]\n\
         - [ACTION: press_key(\"Return\" | \"Tab\" | \"ctrl+s\" | \"ctrl+v\" | \"ctrl+l\" | \"alt+Tab\")]\n\
         - [ACTION: write_code(\"/home/jejej/Duoc/archivo.py|código completo (se guarda, se abre en VS Code, y si es .html también en Chrome)\")]\n\
         - [ACTION: open_login(\"duoc\" | \"vivo duoc\" | \"github\" | \"gmail\" | \"https://...\" | \"sitio|usuario|clave\")]\n\
         - [ACTION: fill_login(\"usuario|contraseña\")]\n\
         - [ACTION: append_soul(\"SOUL.md|nueva regla o rasgo\" | \"MEMORY.md|recuerdo\" | \"TOOLS.md|nota\" | \"USER.md|dato\")]\n\
         - [ACTION: open_soul(\"SOUL.md\")]\n\
         - [ACTION: open_app(\"chrome\" | \"firefox\" | \"code\" | \"antigravity\" | \"android-studio\" | \"intellij\" | \"terminal\" | \"kitty\" | \"nautilus\" | \"calculadora\" | \"btop\" | \"docker\" | \"settings\" | \"pavucontrol\")]\n\
         - [ACTION: open_path(\"duoc\" | \"descargas\" | \"documentos\" | \"workspace\" | \"/ruta\")]\n\
         - [ACTION: open_vscode(\"duoc\" | \"workspace\" | \"/ruta\")]\n\
         - [ACTION: open_url(\"https://...\" | \"youtube\" | \"github\" | \"ava\" | \"gmail\" | \"whatsapp\")]\n\
         - [ACTION: search_youtube(\" término \")]\n\
         - [ACTION: search_web(\" término \")]\n\
         - [ACTION: volume(\"up\" | \"down\" | \"mute\" | \"unmute\" | \"60%\")]\n\
         - [ACTION: brightness(\"up\" | \"down\" | \"80%\")]\n\
         - [ACTION: media(\"play-pause\" | \"next\" | \"prev\")]\n\
         - [ACTION: run_cmd(\"comando bash seguro\")]\n\
         - [ACTION: avatar(\"walk\" | \"dance\" | \"sit\" | \"teleport\")]"
    };

    format!(
        "{}\n\
         Estado actual del notebook de Aym: {}\n\
         Último archivo detectado: {}\n\
         {}{}\n\
         Historial reciente:\n{}\n\
         Instrucción de formato: {}\n\n\
         Mensaje o evento actual de Aym: {}",
        capri_persona, status.active_summary, file_info, snippet, skills_block, hist_text, length_rule, user_msg
    )
}

/// Ejecuta un turno completo del Agente de OpenClaw (`openclaw agent exec`) con acceso
/// a todas las herramientas y Skills (tanto de OpenClaw como las sincronizadas de Antigravity).
pub async fn query_openclaw_agent_exec(
    prompt: &str,
    model_override: Option<&str>,
) -> Result<(String, String), String> {
    let openclaw_bin = if Path::new("/home/jejej/.npm-global/bin/openclaw").exists() {
        "/home/jejej/.npm-global/bin/openclaw"
    } else {
        "openclaw"
    };

    let mut cmd = Command::new(openclaw_bin);
    cmd.arg("--log-level")
        .arg("silent")
        .arg("agent")
        .arg("exec")
        .arg("--cwd")
        .arg("/home/jejej/.openclaw/workspace")
        .arg("--json");

    if let Some(m) = model_override {
        cmd.arg("--model").arg(m);
    }
    cmd.arg(prompt);

    let output = tokio::time::timeout(tokio::time::Duration::from_secs(55), cmd.output())
        .await
        .map_err(|_| "Timeout ejecutando OpenClaw Agent con Skills".to_string())?
        .map_err(|e| format!("Error ejecutando OpenClaw Agent: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    if let Some(json_start) = stdout.find('{') {
        let json_str = &stdout[json_start..];
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(json_str) {
            let model_used = val
                .get("model")
                .and_then(|v| v.as_str())
                .unwrap_or("openclaw-agent/nemotron")
                .to_string();
            if let Some(final_text) = val.get("final").and_then(|t| t.as_str()) {
                let cleaned = final_text.trim().to_string();
                if !cleaned.is_empty() {
                    return Ok((cleaned, model_used));
                }
            }
            if let Some(text) = val
                .get("payloads")
                .and_then(|p| p.as_array())
                .and_then(|arr| arr.first())
                .and_then(|first| first.get("text"))
                .and_then(|t| t.as_str())
            {
                let cleaned = text.trim().to_string();
                if !cleaned.is_empty() {
                    return Ok((cleaned, model_used));
                }
            }
        }
    }

    // Fallback a infer model run si agent exec falla
    query_openclaw_cli(prompt, model_override, None).await
}

pub async fn query_openclaw_cli(
    prompt: &str,
    model_override: Option<&str>,
    image_file: Option<&str>,
) -> Result<(String, String), String> {
    let openclaw_bin = if Path::new("/home/jejej/.npm-global/bin/openclaw").exists() {
        "/home/jejej/.npm-global/bin/openclaw"
    } else {
        "openclaw"
    };

    let mut cmd = Command::new(openclaw_bin);
    cmd.arg("--log-level")
        .arg("silent")
        .arg("infer")
        .arg("model")
        .arg("run")
        .arg("--local")
        .arg("--json");

    if let Some(m) = model_override {
        cmd.arg("--model").arg(m);
    }
    if let Some(img) = image_file {
        if Path::new(img).exists() {
            cmd.arg("--file").arg(img);
        }
    }
    cmd.arg("--prompt").arg(prompt);

    let output = tokio::time::timeout(
        tokio::time::Duration::from_secs(35),
        cmd.output(),
    )
    .await
    .map_err(|_| "Timeout esperando a OpenClaw".to_string())?
    .map_err(|e| format!("Error ejecutando OpenClaw: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    if let Some(json_start) = stdout.find('{') {
        let json_str = &stdout[json_start..];
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(json_str) {
            let model_used = val
                .get("model")
                .and_then(|v| v.as_str())
                .unwrap_or("openclaw/nemotron")
                .to_string();
            if let Some(text) = val
                .get("outputs")
                .and_then(|o| o.as_array())
                .and_then(|arr| arr.first())
                .and_then(|first| first.get("text"))
                .and_then(|t| t.as_str())
            {
                let cleaned = text.trim().to_string();
                if !cleaned.is_empty() {
                    return Ok((cleaned, model_used));
                }
            }
        }
    }

    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    Err(format!(
        "OpenClaw no devolvió respuesta válida ({})",
        stderr.lines().last().unwrap_or("sin detalle")
    ))
}

pub async fn query_gemini_argon_cli(prompt: &str) -> Result<(String, String), String> {
    let agy_bin = if Path::new("/home/jejej/.gemini/bin/agy").exists() {
        "/home/jejej/.gemini/bin/agy"
    } else if Path::new("/usr/local/bin/agy").exists() {
        "/usr/local/bin/agy"
    } else {
        "agy"
    };

    let output = tokio::time::timeout(
        tokio::time::Duration::from_secs(60),
        Command::new(agy_bin)
            .arg("-p")
            .arg(prompt)
            .arg("--model")
            .arg("gemini-4-argon-high")
            .arg("--dangerously-skip-permissions")
            .output(),
    )
    .await
    .map_err(|_| "Timeout consultando a Gemini 4 Argon".to_string())?
    .map_err(|e| format!("Error invocando agy: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !stdout.is_empty() {
        return Ok((stdout, "google/gemini-4-argon-high".to_string()));
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Err(format!("Gemini 4 Argon sin salida: {}", stderr))
}

pub async fn ask_claudia<R: Runtime>(
    app_handle: &AppHandle<R>,
    bridge: &Arc<OpenClawBridge>,
    user_message: String,
    mode_override: Option<String>,
    is_short_reaction: bool,
) -> Result<ChatResponse, String> {
    if let Some(avatar) = app_handle.try_state::<Arc<AvatarSystem<R>>>() {
        avatar.set_state(AvatarState::Thinking);
    }

    let clean_msg = user_message
        .trim_start_matches("/argon")
        .trim_start_matches("/heavy")
        .trim_start_matches("/skill")
        .trim()
        .to_string();

    // 0. Si es una orden directa para el notebook ("abre chrome", "abre vscode", "sube el volumen", "/skills", etc.),
    // ejecutarla al instante (<50ms) sin esperar al LLM
    if !is_short_reaction && !user_message.starts_with("/argon") && !user_message.starts_with("/skill ") {
        if let Some(direct_reply) =
            crate::notebook_hands::try_direct_notebook_action(app_handle, &clean_msg).await
        {
            if let Ok(mut hist) = bridge.history.lock() {
                hist.push((clean_msg, direct_reply.clone()));
                if hist.len() > 12 {
                    hist.remove(0);
                }
            }
            return Ok(ChatResponse {
                reply: direct_reply,
                model_used: "claudia-hands+skills".to_string(),
            });
        }
    }

    let matched_skills = if is_short_reaction {
        Vec::new()
    } else {
        crate::skills_hub::match_skills_for_query(&clean_msg)
    };
    let skill_badge = if matched_skills.is_empty() {
        String::new()
    } else {
        let names: Vec<String> = matched_skills.iter().map(|s| s.name.clone()).collect();
        format!(" + skill:{}", names.join(","))
    };

    let status_snapshot = bridge.get_status();
    let effective_mode = mode_override.unwrap_or_else(|| status_snapshot.mode.clone());
    let history_snapshot = bridge.history.lock().unwrap().clone();

    let full_prompt = build_claudia_prompt(
        &clean_msg,
        &status_snapshot,
        &history_snapshot,
        is_short_reaction,
    );

    let use_argon = !is_short_reaction && should_use_argon(&effective_mode, &user_message);
    let use_agent_exec = !is_short_reaction && (!matched_skills.is_empty() || user_message.starts_with("/skill"));

    let result = if use_argon {
        match query_gemini_argon_cli(&full_prompt).await {
            Ok((r, m)) => Ok((r, format!("{}{}", m, skill_badge))),
            Err(e) => {
                eprintln!("[Fuko AI] Gemini 4 Argon no disponible ({}), usando fallback de programación...", e);
                // Cascada de fallback para programación en OpenClaw:
                // 1. Nemotron 3 Ultra 550B (modelo insignia de código y razonamiento)
                // 2. Nemotron 3.5 Lightning 30B (ultrarrápido y confiable)
                // 3. Nemotron 3.5 Bolt E4B
                let fallback_models = [
                    "nvidia/nemotron-3-ultra-550b-a55b",
                    "nvidia/nemotron-3.5-lightning-30b-a3b",
                    "nvidia/nemotron-3.5-bolt-e4b",
                ];
                let mut fallback_res = Err("Ningún modelo de fallback disponible".to_string());
                for m in fallback_models {
                    let res = if use_agent_exec {
                        query_openclaw_agent_exec(&full_prompt, Some(m)).await
                    } else {
                        query_openclaw_cli(&full_prompt, Some(m), None).await
                    };
                    if let Ok((r, used)) = res {
                        fallback_res = Ok((r, format!("{} (fallback){}", used, skill_badge)));
                        break;
                    }
                }
                fallback_res
            }
        }
    } else {
        let model_choice = match effective_mode.as_str() {
            "ollama" => Some("ollama/qwen3.5:cloud"),
            _ => Some("nvidia/nemotron-3-ultra-550b-a55b"),
        };
        let oc_res = if use_agent_exec {
            query_openclaw_agent_exec(&full_prompt, model_choice).await
        } else {
            query_openclaw_cli(&full_prompt, model_choice, None).await
        };
        match oc_res {
            Ok((r, m)) => Ok((r, format!("{}{}", m, skill_badge))),
            Err(_) => query_gemini_argon_cli(&full_prompt)
                .await
                .map(|(r, m)| (r, format!("{}{}", m, skill_badge))),
        }
    };

    match result {
        Ok((raw_reply, model_used)) => {
            // Ejecutar cualquier [ACTION: ...] que el modelo haya decidido usar sobre el notebook
            let reply = if is_short_reaction {
                raw_reply
            } else {
                crate::notebook_hands::process_and_execute_actions(app_handle, &raw_reply).await
            };

            if !is_short_reaction {
                if let Ok(mut hist) = bridge.history.lock() {
                    hist.push((clean_msg, reply.clone()));
                    if hist.len() > 12 {
                        hist.remove(0);
                    }
                }
            }
            if let Ok(mut st) = bridge.status.lock() {
                st.last_model_used = model_used.clone();
                st.gateway_online = true;
            }

            let _ = app_handle.emit("ai_status_updated", bridge.get_status());

            // Animar a Claudia hablando
            if let Some(avatar) = app_handle.try_state::<Arc<AvatarSystem<R>>>() {
                let avatar_clone = avatar.inner().clone();
                avatar_clone.set_state(AvatarState::Talking);
                let talk_ms = (reply.len() as u64 * 35).clamp(1800, 5500);
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(tokio::time::Duration::from_millis(talk_ms)).await;
                    if avatar_clone.get_state() == AvatarState::Talking {
                        avatar_clone.set_state(AvatarState::Idle);
                    }
                });
            }

            Ok(ChatResponse { reply, model_used })
        }
        Err(err) => {
            if let Some(avatar) = app_handle.try_state::<Arc<AvatarSystem<R>>>() {
                avatar.set_state(AvatarState::Idle);
            }
            Err(err)
        }
    }
}

pub async fn take_desktop_screenshot() -> Option<String> {
    let target_path = "/tmp/claudia_notebook_screen.png";
    let _ = std::fs::remove_file(target_path);

    // 1. Intentar vía GNOME Shell Screenshot DBus
    let dbus_out = Command::new("gdbus")
        .args([
            "call",
            "--session",
            "--dest",
            "org.gnome.Shell.Screenshot",
            "--object-path",
            "/org/gnome/Shell/Screenshot",
            "--method",
            "org.gnome.Shell.Screenshot.Screenshot",
            "false",
            "false",
            target_path,
        ])
        .output()
        .await;

    if dbus_out.is_ok() && Path::new(target_path).exists() {
        return Some(target_path.to_string());
    }

    // 2. Fallback a grim o import
    let _ = Command::new("grim").arg(target_path).output().await;
    if Path::new(target_path).exists() {
        return Some(target_path.to_string());
    }

    let _ = Command::new("import")
        .args(["-window", "root", target_path])
        .output()
        .await;
    if Path::new(target_path).exists() {
        return Some(target_path.to_string());
    }

    None
}

// --- Tauri Commands ---

#[tauri::command]
pub async fn send_chat_message(
    message: String,
    mode: Option<String>,
    app_handle: AppHandle<tauri::Wry>,
    bridge: tauri::State<'_, Arc<OpenClawBridge>>,
) -> Result<ChatResponse, String> {
    ask_claudia(&app_handle, bridge.inner(), message, mode, false).await
}

#[tauri::command]
pub async fn get_ai_status_cmd(
    bridge: tauri::State<'_, Arc<OpenClawBridge>>,
) -> Result<AiStatus, String> {
    Ok(bridge.get_status())
}

#[tauri::command]
pub async fn set_ai_mode_cmd(
    mode: String,
    app_handle: AppHandle<tauri::Wry>,
    bridge: tauri::State<'_, Arc<OpenClawBridge>>,
) -> Result<AiStatus, String> {
    if let Ok(mut st) = bridge.status.lock() {
        st.mode = mode;
    }
    let st = bridge.get_status();
    let _ = app_handle.emit("ai_status_updated", &st);
    Ok(st)
}

#[tauri::command]
pub async fn set_proactive_mode_cmd(
    proactive: bool,
    app_handle: AppHandle<tauri::Wry>,
    bridge: tauri::State<'_, Arc<OpenClawBridge>>,
) -> Result<AiStatus, String> {
    if let Ok(mut st) = bridge.status.lock() {
        st.proactive = proactive;
    }
    let st = bridge.get_status();
    let _ = app_handle.emit("ai_status_updated", &st);
    Ok(st)
}

#[tauri::command]
pub async fn analyze_notebook_cmd(
    custom_question: Option<String>,
    app_handle: AppHandle<tauri::Wry>,
    bridge: tauri::State<'_, Arc<OpenClawBridge>>,
) -> Result<ChatResponse, String> {
    if let Some(avatar) = app_handle.try_state::<Arc<AvatarSystem<tauri::Wry>>>() {
        avatar.set_state(AvatarState::Thinking);
    }

    let screenshot_path = take_desktop_screenshot().await;
    let status_snapshot = bridge.get_status();
    let question = custom_question.unwrap_or_else(|| {
        "Mira lo que estoy haciendo ahora en mi notebook (mis aplicaciones abiertas, pestañas del navegador, archivos recientes y pantalla) y dame un comentario útil, consejo o ánimo sobre lo que estoy viendo.".to_string()
    });

    // Detectar si el usuario está viendo un repositorio de GitHub o página web en Chrome
    let mut browser_context = String::new();
    if status_snapshot.active_summary.contains("jejeje7373/marketplaceco2")
        || status_snapshot.active_summary.contains("marketplace")
    {
        browser_context = "\n[ Contexto de la página actual que tienes abierta en Chrome ]: Estás viendo tu repositorio 'jejeje7373/marketplaceco2' en GitHub (proyecto Marketplace Fullstack en Vue/Node/Vite).".to_string();
    }

    let enriched_question = format!("{}{}", question, browser_context);
    let prompt = build_claudia_prompt(&enriched_question, &status_snapshot, &[], false);

    // Si logramos tomar captura de pantalla, intentar con el modelo de visión de OpenClaw
    if let Some(ref img_path) = screenshot_path {
        if let Ok((reply, model_used)) = query_openclaw_cli(
            &prompt,
            Some("nvidia/nemotron-3.5-super-vl-120b-a12b"),
            Some(img_path),
        )
        .await
        {
            if let Some(avatar) = app_handle.try_state::<Arc<AvatarSystem<tauri::Wry>>>() {
                avatar.set_state(AvatarState::Talking);
            }
            let _ = app_handle.emit(
                "avatar_speak",
                serde_json::json!({
                    "message": reply,
                    "model": model_used
                }),
            );
            return Ok(ChatResponse { reply, model_used });
        }
    }

    let res = ask_claudia(&app_handle, bridge.inner(), question, None, false).await?;
    let _ = app_handle.emit(
        "avatar_speak",
        serde_json::json!({
            "message": res.reply,
            "model": res.model_used
        }),
    );
    Ok(res)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubAgentInfo {
    pub key: String,
    pub label: String,
    pub model: String,
    pub status: String,
    pub age: String,
    pub kind: String,
    pub tokens: String,
}

#[tauri::command]
pub async fn list_openclaw_sessions_cmd() -> Result<Vec<SubAgentInfo>, String> {
    let openclaw_bin = if Path::new("/home/jejej/.npm-global/bin/openclaw").exists() {
        "/home/jejej/.npm-global/bin/openclaw"
    } else {
        "openclaw"
    };

    let output = Command::new(openclaw_bin)
        .arg("sessions")
        .arg("--json")
        .arg("--limit")
        .arg("20")
        .output()
        .await
        .map_err(|e| format!("Error ejecutando openclaw sessions: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let json_start = stdout.find('{').ok_or_else(|| "No se encontró JSON en openclaw sessions".to_string())?;
    let val: serde_json::Value = serde_json::from_str(&stdout[json_start..])
        .map_err(|e| format!("Error parseando sesiones: {}", e))?;

    let mut list = Vec::new();
    if let Some(sessions) = val.get("sessions").and_then(|s| s.as_array()) {
        for s in sessions {
            let key = s.get("key").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let label = s.get("label").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let model = s.get("model").and_then(|v| v.as_str()).unwrap_or("nemotron").to_string();
            let status = s.get("status").and_then(|v| v.as_str()).unwrap_or("running").to_string();
            let kind = s.get("kind").and_then(|v| v.as_str()).unwrap_or("session").to_string();
            let tokens = s.get("totalTokens")
                .and_then(|v| v.as_u64())
                .map(|t| format!("{} tok", t))
                .unwrap_or_else(|| "-".to_string());

            let age_ms = s.get("ageMs").and_then(|v| v.as_u64()).unwrap_or(0);
            let age_str = if age_ms < 60_000 {
                "hace poco".to_string()
            } else if age_ms < 3600_000 {
                format!("hace {}m", age_ms / 60_000)
            } else if age_ms < 86400_000 {
                format!("hace {}h", age_ms / 3600_000)
            } else {
                format!("hace {}d", age_ms / 86400_000)
            };

            let clean_model = model.replace("nvidia/", "").replace("google/", "");
            let clean_label = if label.is_empty() {
                if key.contains("dashboard:") {
                    "Sub-agente Dashboard".to_string()
                } else if key.contains("cron:") {
                    "Tarea en background".to_string()
                } else {
                    key.clone()
                }
            } else {
                label
            };

            list.push(SubAgentInfo {
                key,
                label: clean_label,
                model: clean_model,
                status,
                age: age_str,
                kind,
                tokens,
            });
        }
    }

    Ok(list)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeResult {
    pub ok: bool,
    pub latency_ms: u64,
    pub reply: String,
    pub model: String,
}

#[tauri::command]
pub async fn probe_gemini_argon_cmd() -> Result<ProbeResult, String> {
    let start = std::time::Instant::now();
    let agy_bin = if Path::new("/home/jejej/.gemini/bin/agy").exists() {
        "/home/jejej/.gemini/bin/agy"
    } else if Path::new("/usr/local/bin/agy").exists() {
        "/usr/local/bin/agy"
    } else {
        "agy"
    };

    let res = tokio::time::timeout(
        tokio::time::Duration::from_secs(12),
        Command::new(agy_bin)
            .arg("-p")
            .arg("Responde exactamente en una palabra: ONLINE")
            .arg("--model")
            .arg("gemini-4-argon-high")
            .arg("--dangerously-skip-permissions")
            .output(),
    )
    .await;

    let latency_ms = start.elapsed().as_millis() as u64;
    match res {
        Ok(Ok(out)) if out.status.success() => {
            let txt = String::from_utf8_lossy(&out.stdout).trim().to_string();
            Ok(ProbeResult {
                ok: true,
                latency_ms,
                reply: txt,
                model: "Gemini 4 Argon (High)".to_string(),
            })
        }
        Ok(Ok(out)) => {
            let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
            Ok(ProbeResult {
                ok: false,
                latency_ms,
                reply: format!("Fallo: {}", err),
                model: "Gemini 4 Argon".to_string(),
            })
        }
        Ok(Err(e)) => Ok(ProbeResult {
            ok: false,
            latency_ms,
            reply: format!("Error: {}", e),
            model: "Gemini 4 Argon".to_string(),
        }),
        Err(_) => Ok(ProbeResult {
            ok: false,
            latency_ms: 12000,
            reply: "Timeout alcanzado".to_string(),
            model: "Gemini 4 Argon".to_string(),
        }),
    }
}
