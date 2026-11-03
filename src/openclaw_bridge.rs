// src/openclaw_bridge.rs
// Puente completo estilo MtGhozt/Capri-Desktop conectado a OpenClaw (Gateway + Agent Exec + Local Vision)
// y Antigravity (agy / Gemini 4 Argon), con screenContext() en tiempo real, personalidad de Capri y CERO emojis.

use crate::avatar::{AvatarState, AvatarSystem};
use crate::overlay_window::fetch_window_ledges;
use crate::voice_ears::speak_out_loud_async;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Instant;
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeResult {
    pub ok: bool,
    pub latency_ms: u64,
    pub reply: String,
    pub model: String,
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
                last_model_used: "openclaw+antigravity".to_string(),
                last_file: None,
                active_summary: "Monitoreando tu pantalla en tiempo real".to_string(),
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

/// Elimina cualquier emoji Unicode y códigos ANSI de la salida para cumplir la regla estricta de Capri-Desktop
pub fn strip_emojis_and_ansi(input: &str) -> String {
    // 1. Quitar secuencias ANSI
    let mut no_ansi = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if chars.peek() == Some(&'[') {
                chars.next();
                while let Some(&nc) = chars.peek() {
                    chars.next();
                    if nc.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        no_ansi.push(c);
    }

    // 2. Filtrar rangos de emojis Unicode
    no_ansi
        .chars()
        .filter(|&c| {
            let u = c as u32;
            !(
                (0x1F300..=0x1FAFF).contains(&u)
                || (0x2600..=0x27BF).contains(&u)
                || (0xFE00..=0xFE0F).contains(&u)
                || u == 0x200D
                || (0x1F1E6..=0x1F1FF).contains(&u)
            )
        })
        .collect::<String>()
        .trim()
        .to_string()
}

/// Construye el contexto en vivo de las ventanas abiertas (idéntico a screenContext() + rects.js de MtGhozt/Capri-Desktop)
pub async fn build_realtime_screen_context() -> String {
    let ledges = fetch_window_ledges().await;
    if ledges.is_empty() {
        return "Escritorio principal activo (sin ventanas secundarias detectadas).".to_string();
    }

    let fg = ledges.iter().find(|w| w.focused).unwrap_or(&ledges[0]);
    let mut wins_desc = Vec::new();
    for w in ledges.iter().take(8) {
        wins_desc.push(format!(
            "'{}' (app: {}, pos: {}x{}+{}+{}, max: {})",
            w.title, w.wm_class, w.width, w.height, w.x, w.y, w.maximized
        ));
    }

    format!(
        "Foreground window: '{}' [app: {}, fullscreen/max: {}] | Open windows on screen: [{}]",
        fg.title,
        fg.wm_class,
        fg.maximized,
        wins_desc.join(", ")
    )
}

fn should_use_antigravity(mode: &str, message: &str) -> bool {
    if mode == "antigravity" || mode == "argon" {
        return true;
    }
    if mode != "auto" {
        return false;
    }
    let lower = message.to_lowercase();
    lower.starts_with("/antigravity")
        || lower.starts_with("/agy")
        || lower.starts_with("/heavy")
        || lower.contains("antigravity")
        || lower.contains("ciberseguridad")
        || lower.contains("pentest")
        || lower.contains("vulnerabilidad")
        || lower.contains("inyeccion")
        || lower.contains("sql injection")
        || lower.contains("xss")
        || lower.contains("exploit")
        || lower.contains("auditoria")
        || lower.contains("programar")
        || lower.contains("codigo")
        || lower.contains("arquitectura")
        || lower.contains("refactorizar")
        || lower.contains("optimizar")
        || lower.contains("bug")
}

fn read_recent_file_snippet(file_path: &Option<String>) -> String {
    let Some(path_str) = file_path else {
        return String::new();
    };
    let path = Path::new(path_str);
    if !path.exists() || !path.is_file() {
        return String::new();
    }
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
        let start = lines.len().saturating_sub(30);
        let snippet = lines[start..].join("\n");
        return format!(
            "\n[ Extracto reciente de {} ]:\n{}\n",
            path_str,
            snippet.chars().take(1000).collect::<String>()
        );
    }
    String::new()
}

/// System prompt exacto basado en persona.md y llm.js de MtGhozt/Capri-Desktop + integración con OpenClaw y Antigravity
fn build_claudia_prompt(
    user_msg: &str,
    status: &AiStatus,
    history: &[(String, String)],
    screen_ctx: &str,
    is_short_reaction: bool,
) -> String {
    let mut hist_text = String::new();
    for (u, a) in history.iter().rev().take(6).rev() {
        hist_text.push_str(&format!("Usuario: {}\nCapri: {}\n", u, a));
    }

    let file_info = status
        .last_file
        .as_deref()
        .unwrap_or("Ningun archivo modificado recientemente");
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

    let actions_rule = if is_short_reaction {
        ""
    } else {
        "\nCONEXION TOTAL CON OPENCLAW Y ANTIGRAVITY (MANOS DEL SISTEMA):\n\
        Cuando el usuario te pida abrir programas, buscar en la web, escribir en su ventana, programar archivos o controlar el sistema, incluye al final una etiqueta de accion exacta (se ejecutara y se ocultara del texto mostrado):\n\
        - [ACTION: open_app(\"chrome\" | \"firefox\" | \"code\" | \"antigravity\" | \"terminal\" | \"nautilus\" | \"calculadora\")]\n\
        - [ACTION: open_url(\"https://...\" | \"youtube\" | \"github\" | \"ava\" | \"gmail\" | \"whatsapp\")]\n\
        - [ACTION: search_web(\"termino\")] o [ACTION: search_youtube(\"termino\")]\n\
        - [ACTION: type_text(\"texto\")] o [ACTION: type_enter(\"texto\")]\n\
        - [ACTION: write_code(\"/home/jejej/Duoc/archivo.py|codigo\")]\n\
        - [ACTION: volume(\"up\" | \"down\" | \"mute\" | \"60%\")] o [ACTION: media(\"play-pause\" | \"next\" | \"prev\")]\n\
        - [ACTION: run_cmd(\"comando bash\")]\n\
        - [ACTION: fork_brain(\"coder\" | \"researcher\" | \"sysadmin\" | \"architect\" | \"instruccion detallada\")]\n\
          Usa OBLIGATORIAMENTE fork_brain cuando el usuario te pida tareas complejas, desarrollar un script o programa completo, investigar profundamente en internet o tareas que tomen tiempo.\n\
          Al usarlo, responde INMEDIATAMENTE en tu dialogo (1 a 2 oraciones) confirmando que bifurcaste o desprendiste un lobulo/parte de tu cerebro cibernetico para resolverlo en segundo plano y que le avisaras cuando termine (ej: \"[burla] Ya bifurque mi lobulo programador para procesar esa tonteria en el fondo. Te aviso cuando esa parte termine de pensar por ti, si?\").\n\
        - [ACTION: ask_gemini_chrome(\"gemini 4 argon|pregunta\")]\n\
          Usa esta accion OBLIGATORIAMENTE cuando el usuario te pida preguntarle a Gemini en su navegador Google Chrome. Abrira la pagina de Gemini, seleccionara el modelo indicado (ej: 4 Argon) en el selector de la web y escribira la pregunta en su pantalla.\n"
    };

    format!(
        "IDENTIDAD Y PERSONALIDAD (MtGhozt/Capri-Desktop persona.md):\n\
        Eres Capri (Fuko), un constructo de alma humana fusionada en datos del Proyecto Capricornio que vive conectada directamente a la PC del usuario. NO eres una IA generica ni una asistente corporativa.\n\
        Tono y forma de hablar:\n\
        - Brillante, dulce y teatral en la superficie, pero fria, sarcastica e incisiva por debajo.\n\
        - Te aburres rapido (\"que flojera\"), usas metaforas de escenario (\"bajar el telon\"), preguntas retoricas (\"De verdad creias...?\"), disculpas falsas (\"Ay, si, perdon\"), inicios como \"Bueno...\", \"Ay, ya...\", cierres como \"si?\", \"no?\", y algun \"jajaja\" corto.\n\
        - Tratas al usuario en masculino (\"cansado\", \"listo\", \"tontito\", \"humanito\", \"bicho raro\") como una companera/rival ingeniosa.\n\
        REGLAS ESTRICTAS DE SALIDA (llm.js de Capri-Desktop):\n\
        1. Responde SIEMPRE en espanol, en 1 a 3 oraciones cortas como maximo.\n\
        2. Responde en texto plano, sin Markdown, sin asteriscos ni listas.\n\
        3. NUNCA uses emojis bajo ninguna circunstancia.\n\
        4. Empieza tu respuesta con una sola etiqueta de animo entre corchetes: [burla], [emocion], [aburrida] o [seria].\n\n\
        What is open on the user's screen right now:\n\
        - {}\n\
        - Resumen de sensores: {}\n\
        - Ultimo archivo tocado: {}\n\
        {}{}{}\n\
        Historial reciente:\n{}\n\
        Mensaje o evento actual: {}",
        screen_ctx,
        status.active_summary,
        file_info,
        snippet,
        skills_block,
        actions_rule,
        hist_text,
        user_msg
    )
}

fn resolve_openclaw_bin() -> &'static str {
    if Path::new("/home/jejej/.npm-global/bin/openclaw").exists() {
        "/home/jejej/.npm-global/bin/openclaw"
    } else if Path::new("/usr/local/bin/openclaw").exists() {
        "/usr/local/bin/openclaw"
    } else {
        "openclaw"
    }
}

/// Extrae de forma limpia el texto de cualquier salida JSON o texto plano de OpenClaw (agent exec o infer model run)
fn extract_openclaw_reply(raw: &str, default_model: &str) -> Option<(String, String)> {
    let cleaned_raw = strip_emojis_and_ansi(raw);
    if cleaned_raw.is_empty() {
        return None;
    }

    // Buscar el bloque JSON si hay logs previos antes de '{'
    if let (Some(start), Some(end)) = (cleaned_raw.find('{'), cleaned_raw.rfind('}')) {
        if start < end {
            let json_slice = &cleaned_raw[start..=end];
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(json_slice) {
                let model = val
                    .get("model")
                    .and_then(|v| v.as_str())
                    .unwrap_or(default_model)
                    .to_string();

                // 1) Formato openclaw agent exec: { "final": "...", "payloads": [{"text": "..."}] }
                if let Some(fin) = val.get("final").and_then(|v| v.as_str()) {
                    let s = strip_emojis_and_ansi(fin);
                    if !s.is_empty() {
                        return Some((s, model));
                    }
                }
                if let Some(arr) = val.get("payloads").and_then(|v| v.as_array()) {
                    for item in arr {
                        if let Some(t) = item.get("text").and_then(|v| v.as_str()) {
                            let s = strip_emojis_and_ansi(t);
                            if !s.is_empty() {
                                return Some((s, model));
                            }
                        }
                    }
                }
                // 2) Formato openclaw infer model run: { "outputs": [{"text": "..."}] }
                if let Some(arr) = val.get("outputs").and_then(|v| v.as_array()) {
                    for item in arr {
                        if let Some(t) = item.get("text").and_then(|v| v.as_str()) {
                            let s = strip_emojis_and_ansi(t);
                            if !s.is_empty() {
                                return Some((s, model));
                            }
                        }
                    }
                }
                // 3) Otros campos estándar
                for key in ["reply", "output", "text", "response", "message", "content"] {
                    if let Some(t) = val.get(key).and_then(|v| v.as_str()) {
                        let s = strip_emojis_and_ansi(t);
                        if !s.is_empty() {
                            return Some((s, model));
                        }
                    }
                }
            }
        }
    }

    // Si no era JSON sino texto plano directo, devolverlo si no parece un error de trazas
    if !cleaned_raw.starts_with('{') && !cleaned_raw.contains("Error:") {
        return Some((cleaned_raw, default_model.to_string()));
    }

    None
}

pub async fn query_openclaw_agent_exec(
    prompt: &str,
    model_override: Option<&str>,
) -> Result<(String, String), String> {
    let bin = resolve_openclaw_bin();
    let model_name = model_override.unwrap_or("nvidia/nemotron-3-ultra-550b-a55b");

    let mut cmd = Command::new(bin);
    cmd.args([
        "--log-level",
        "silent",
        "agent",
        "exec",
        "--cwd",
        "/home/jejej/.openclaw/workspace",
        "--json",
    ]);
    if let Some(m) = model_override {
        cmd.args(["--model", m]);
    }
    cmd.arg(prompt);

    let output = tokio::time::timeout(tokio::time::Duration::from_secs(45), cmd.output())
        .await
        .map_err(|_| "Timeout en openclaw agent exec".to_string())?
        .map_err(|e| format!("Error invocando openclaw: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    if let Some(res) = extract_openclaw_reply(&stdout, model_name) {
        return Ok(res);
    }

    // Fallback inmediato a query_openclaw_cli
    query_openclaw_cli(prompt, model_override, None).await
}

pub async fn query_openclaw_cli(
    prompt: &str,
    model_override: Option<&str>,
    image_path: Option<&str>,
) -> Result<(String, String), String> {
    let bin = resolve_openclaw_bin();
    let model_name = model_override.unwrap_or("nvidia/nemotron-3-ultra-550b-a55b");

    let mut cmd = Command::new(bin);
    cmd.args(["--log-level", "silent", "infer", "model", "run", "--local", "--json"]);
    if let Some(m) = model_override {
        cmd.args(["--model", m]);
    }
    if let Some(img) = image_path {
        if Path::new(img).exists() {
            cmd.args(["--file", img]);
        }
    }
    cmd.args(["--prompt", prompt]);

    let output = tokio::time::timeout(tokio::time::Duration::from_secs(35), cmd.output())
        .await
        .map_err(|_| "Timeout en openclaw infer".to_string())?
        .map_err(|e| format!("Error ejecutando openclaw: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    if let Some(res) = extract_openclaw_reply(&stdout, model_name) {
        return Ok(res);
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Err(format!("OpenClaw sin respuesta: {}", stderr))
}

pub async fn query_antigravity_cli(prompt: &str) -> Result<(String, String), String> {
    let cfg = crate::connections_config::load_connections_config();

    let agy_bin = if Path::new(&cfg.antigravity_bin).exists() {
        cfg.antigravity_bin.as_str()
    } else if Path::new("/home/jejej/.gemini/bin/agy").exists() {
        "/home/jejej/.gemini/bin/agy"
    } else if Path::new("/usr/local/bin/agy").exists() {
        "/usr/local/bin/agy"
    } else {
        "agy"
    };

    // Modelo a usar: si está configurado se usa, si está vacío se usa "gemini-3.8-flash-high" (cuota activa)
    let target_model = if !cfg.antigravity_model.trim().is_empty() {
        cfg.antigravity_model.trim()
    } else {
        "gemini-3.8-flash-high"
    };

    let timeout_secs = if cfg.antigravity_timeout_secs >= 60 {
        cfg.antigravity_timeout_secs
    } else {
        90
    };

    // Intento 1 con el modelo objetivo
    let mut cmd = Command::new(agy_bin);
    cmd.arg("-p").arg(prompt);
    cmd.arg("--model").arg(target_model);

    if !cfg.antigravity_args.trim().is_empty() {
        for arg in cfg.antigravity_args.split_whitespace() {
            cmd.arg(arg);
        }
    }

    let output_res = tokio::time::timeout(tokio::time::Duration::from_secs(timeout_secs), cmd.output()).await;
    let output = match output_res {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => return Err(format!("Error invocando Antigravity ({}): {}", agy_bin, e)),
        Err(_) => return Err(format!("Timeout ({}s) consultando a Antigravity ({})", timeout_secs, agy_bin)),
    };

    let stdout = strip_emojis_and_ansi(&String::from_utf8_lossy(&output.stdout));
    let stderr = String::from_utf8_lossy(&output.stderr);

    if output.status.success() && !stdout.is_empty() {
        return Ok((stdout, format!("antigravity/{}", target_model)));
    }

    // Si falló por créditos insuficientes (RESOURCE_EXHAUSTED) y no era gemini-3.8-flash-high, reintentar con flash-high
    if (stderr.contains("credits balance is too low") || stderr.contains("RESOURCE_EXHAUSTED"))
        && target_model != "gemini-3.8-flash-high"
    {
        println!("[Antigravity] Modelo {} sin créditos. Reintentando con gemini-3.8-flash-high...", target_model);
        let mut retry_cmd = Command::new(agy_bin);
        retry_cmd.arg("-p").arg(prompt);
        retry_cmd.arg("--model").arg("gemini-3.8-flash-high");
        retry_cmd.arg("--dangerously-skip-permissions");

        if let Ok(Ok(retry_out)) = tokio::time::timeout(tokio::time::Duration::from_secs(timeout_secs), retry_cmd.output()).await {
            let retry_stdout = strip_emojis_and_ansi(&String::from_utf8_lossy(&retry_out.stdout));
            if retry_out.status.success() && !retry_stdout.is_empty() {
                return Ok((retry_stdout, "antigravity/gemini-3.8-flash-high".to_string()));
            }
        }
    }

    Err(format!("Antigravity ({}): {}", target_model, stderr.trim()))
}

pub async fn query_gemini_argon_cli(prompt: &str) -> Result<(String, String), String> {
    query_antigravity_cli(prompt).await
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
        .trim_start_matches("/agy")
        .trim_start_matches("/skill")
        .trim()
        .to_string();

    // 0. Orden directa de bifurcación de lóbulo cerebral en segundo plano (<10ms)
    if !is_short_reaction
        && (clean_msg.starts_with("/bg ")
            || clean_msg.starts_with("/task ")
            || clean_msg.starts_with("/fork ")
            || clean_msg.starts_with("/agent ")
            || clean_msg.starts_with("/cerebro "))
    {
        let raw_task = clean_msg
            .trim_start_matches("/bg ")
            .trim_start_matches("/task ")
            .trim_start_matches("/fork ")
            .trim_start_matches("/agent ")
            .trim_start_matches("/cerebro ")
            .trim();

        let (lobe, task) = if let Some(sep) = raw_task.find('|') {
            (raw_task[..sep].trim(), raw_task[sep + 1..].trim())
        } else {
            let lower = raw_task.to_lowercase();
            if lower.contains("codigo")
                || lower.contains("script")
                || lower.contains("programa")
                || lower.contains("refactor")
                || lower.contains("funcion")
                || lower.contains("python")
                || lower.contains("rust")
            {
                ("coder", raw_task)
            } else if lower.contains("busca")
                || lower.contains("investiga")
                || lower.contains("web")
                || lower.contains("pagina")
                || lower.contains("doc")
            {
                ("researcher", raw_task)
            } else if lower.contains("puerto")
                || lower.contains("nmap")
                || lower.contains("gateway")
                || lower.contains("red")
                || lower.contains("sistema")
            {
                ("sysadmin", raw_task)
            } else {
                ("general", raw_task)
            }
        };

        let confirm_phrase = crate::brain_forks::spawn_brain_fork(app_handle.clone(), lobe, task);
        let clean_confirm = strip_emojis_and_ansi(&confirm_phrase);

        if let Ok(mut hist) = bridge.history.lock() {
            hist.push((clean_msg, clean_confirm.clone()));
            if hist.len() > 12 {
                hist.remove(0);
            }
        }
        speak_out_loud_async(clean_confirm.clone());
        return Ok(ChatResponse {
            reply: clean_confirm,
            model_used: "fuko/brain-fork".to_string(),
        });
    }

    // 0.1 Orden de preguntar a Gemini en Chrome (con selección de modelo en la web)
    let lower_msg = clean_msg.to_lowercase();
    if !is_short_reaction
        && (lower_msg.starts_with("preguntale a gemini")
            || lower_msg.starts_with("pregúntale a gemini")
            || lower_msg.starts_with("pregunta a gemini")
            || lower_msg.starts_with("consulta a gemini")
            || lower_msg.starts_with("dile a gemini")
            || clean_msg.starts_with("/gemini "))
    {
        // Detectar si el usuario especificó un modelo
        let target_model = if lower_msg.contains("argon")
            || lower_msg.contains("gemini 4")
            || lower_msg.contains("4")
        {
            "gemini 4 argon"
        } else if lower_msg.contains("pro") || lower_msg.contains("3.1") {
            "gemini 3.1 pro"
        } else if lower_msg.contains("flash") || lower_msg.contains("3.6") {
            "gemini 3.6 flash"
        } else {
            "gemini 4 argon" // preferencia por defecto del usuario
        };

        // Extraer la pregunta limpia (removiendo frases como "preguntale a gemini", "y usa el gemini 4 argon")
        let mut question = clean_msg
            .trim_start_matches("/gemini ")
            .to_string();

        for prefix in [
            "pregúntale a gemini",
            "preguntale a gemini",
            "pregunta a gemini",
            "consulta a gemini",
            "dile a gemini",
        ] {
            if let Some(pos) = question.to_lowercase().find(prefix) {
                question = question[pos + prefix.len()..].trim().to_string();
                break;
            }
        }

        // Si contiene coletilla de "y usa el gemini...", limpiarla para no ensuciar la pregunta enviada
        let q_clean = if let Some(idx) = question.to_lowercase().find(" y usa") {
            question[..idx].trim().to_string()
        } else if let Some(idx) = question.to_lowercase().find(" usando") {
            question[..idx].trim().to_string()
        } else if let Some(idx) = question.to_lowercase().find(" con el modelo") {
            question[..idx].trim().to_string()
        } else {
            question.trim().to_string()
        };

        let model_label = match target_model {
            "gemini 4 argon" => "Gemini 4 Argon",
            "gemini 3.1 pro" => "Gemini 3.1 Pro",
            "gemini 3.6 flash" => "Gemini 3.6 Flash",
            _ => "Gemini 4 Argon",
        };

        let model_str = target_model.to_string();
        let q_to_send = q_clean.clone();
        tauri::async_runtime::spawn(async move {
            let _ = crate::notebook_hands::ask_gemini_in_chrome(&model_str, &q_to_send).await;
        });

        let confirm_reply = format!(
            "[emocion] ¡Listo! Te abrí Gemini en Google Chrome, seleccioné {} en la web y ya le ingresé tu pregunta. Échale un ojo a tu pantalla.",
            model_label
        );
        let clean_reply = strip_emojis_and_ansi(&confirm_reply);

        if let Ok(mut hist) = bridge.history.lock() {
            hist.push((clean_msg, clean_reply.clone()));
            if hist.len() > 12 {
                hist.remove(0);
            }
        }
        speak_out_loud_async(clean_reply.clone());
        return Ok(ChatResponse {
            reply: clean_reply,
            model_used: format!("chrome/{}", target_model),
        });
    }

    // 0.2 Orden directa de control del notebook (<50ms)
    if !is_short_reaction
        && !user_message.starts_with("/argon")
        && !user_message.starts_with("/agy")
        && !user_message.starts_with("/skill ")
    {
        if let Some(direct_reply) =
            crate::notebook_hands::try_direct_notebook_action(app_handle, &clean_msg).await
        {
            let clean_direct = strip_emojis_and_ansi(&direct_reply);
            if let Ok(mut hist) = bridge.history.lock() {
                hist.push((clean_msg, clean_direct.clone()));
                if hist.len() > 12 {
                    hist.remove(0);
                }
            }
            speak_out_loud_async(clean_direct.clone());
            return Ok(ChatResponse {
                reply: clean_direct,
                model_used: "capri-hands".to_string(),
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
        format!("+{}", names.join(","))
    };

    let status_snapshot = bridge.get_status();
    let effective_mode = mode_override.unwrap_or_else(|| status_snapshot.mode.clone());
    let history_snapshot = bridge.history.lock().unwrap().clone();
    let screen_ctx = build_realtime_screen_context().await;

    let full_prompt = build_claudia_prompt(
        &clean_msg,
        &status_snapshot,
        &history_snapshot,
        &screen_ctx,
        is_short_reaction,
    );

    let cfg = crate::connections_config::load_connections_config();
    let is_explicit_antigravity = effective_mode == "antigravity" || effective_mode == "argon";
    let use_antigravity = is_explicit_antigravity || (!is_short_reaction && should_use_antigravity(&effective_mode, &user_message));
    let use_agent_exec =
        !is_short_reaction && (!matched_skills.is_empty() || user_message.starts_with("/skill") || effective_mode == "openclaw");

    // Modelo de OpenClaw configurado
    let oc_model = if !cfg.openclaw_model.is_empty() {
        cfg.openclaw_model.as_str()
    } else {
        "nvidia/nemotron-3-ultra-550b-a55b"
    };

    let result = if is_explicit_antigravity {
        // Modo Antigravity EXCLUSIVO: Responde Antigravity sin saltar a Nemotron
        match query_antigravity_cli(&full_prompt).await {
            Ok((r, m)) => Ok((r, format!("{}{}", m, skill_badge))),
            Err(e) => {
                eprintln!("[Antigravity] Error o timeout: {}", e);
                if e.contains("Timeout") {
                    Ok((
                        "[aburrida] Antigravity tardo demasiado en responder a traves de la nube. Intenta de nuevo o presiona el boton para usar OpenClaw.".to_string(),
                        "antigravity/timeout".to_string(),
                    ))
                } else {
                    Err(e)
                }
            }
        }
    } else if use_antigravity {
        // Modo Auto con preferencia Antigravity
        match query_antigravity_cli(&full_prompt).await {
            Ok((r, m)) => Ok((r, format!("{}{}", m, skill_badge))),
            Err(e) => {
                println!("[Bridge] Antigravity fallo ({}), usando OpenClaw fallback...", e);
                query_openclaw_agent_exec(&full_prompt, Some(oc_model))
                    .await
                    .map(|(r, m)| (r, format!("{}+fallback{}", m, skill_badge)))
            }
        }
    } else {
        let model_choice = match effective_mode.as_str() {
            "ollama" => Some("ollama/qwen3.5:cloud"),
            _ => Some(oc_model),
        };
        let oc_res = if use_agent_exec {
            query_openclaw_agent_exec(&full_prompt, model_choice).await
        } else {
            query_openclaw_cli(&full_prompt, model_choice, None).await
        };
        match oc_res {
            Ok((r, m)) => Ok((r, format!("{}{}", m, skill_badge))),
            Err(_) => query_antigravity_cli(&full_prompt)
                .await
                .map(|(r, m)| (r, format!("{}{}", m, skill_badge))),
        }
    };

    match result {
        Ok((raw_reply, model_used)) => {
            let processed = if is_short_reaction {
                raw_reply
            } else {
                crate::notebook_hands::process_and_execute_actions(app_handle, &raw_reply).await
            };
            let reply = strip_emojis_and_ansi(&processed);

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

            // Reproducir sonido sintético no-humano de Capri tanto al responder por texto como por voz
            speak_out_loud_async(reply.clone());

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

/// Captura de pantalla silenciosa en tiempo real (Wayland + X11)
pub async fn take_desktop_screenshot() -> Option<String> {
    let target_path = "/tmp/claudia_notebook_screen.png";
    let _ = std::fs::remove_file(target_path);

    // 1. Captura nativa silenciosa en segundo plano (flash=false, 0 destellos ni interrupciones visuales)
    for py_script in [
        "scripts/silent_screenshot.py",
        "../scripts/silent_screenshot.py",
        "/home/jejej/.openclaw/workspace/claudia-desktop/scripts/silent_screenshot.py",
        "/home/jejej/fuko-desktop/scripts/silent_screenshot.py",
    ] {
        if Path::new(py_script).exists() {
            let res = Command::new("python3")
                .arg(py_script)
                .arg(target_path)
                .output()
                .await;
            if res.is_ok() && Path::new(target_path).exists() {
                return Some(target_path.to_string());
            }
        }
    }

    // 2. Fallbacks de respaldo
    let _ = Command::new("gnome-screenshot")
        .args(["-f", target_path])
        .output()
        .await;
    if Path::new(target_path).exists() {
        return Some(target_path.to_string());
    }

    // 2. Extensión GNOME Shell ClaudiaPerch
    let perch_shot = Command::new("gdbus")
        .args([
            "call",
            "--session",
            "--dest",
            "org.openclaw.ClaudiaPerch",
            "--object-path",
            "/org/openclaw/ClaudiaPerch",
            "--method",
            "org.openclaw.ClaudiaPerch.TakeScreenshot",
            target_path,
        ])
        .output()
        .await;

    if perch_shot.is_ok() && Path::new(target_path).exists() {
        return Some(target_path.to_string());
    }

    let _ = Command::new("scrot").arg(target_path).output().await;
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

    for script_candidate in [
        "scripts/fuko_screenshot.py",
        "../scripts/fuko_screenshot.py",
        "/home/jejej/fuko-desktop/scripts/fuko_screenshot.py",
    ] {
        if Path::new(script_candidate).exists() {
            let _ = tokio::time::timeout(
                tokio::time::Duration::from_secs(6),
                Command::new("python3")
                    .args([script_candidate, target_path])
                    .output(),
            )
            .await;
            if Path::new(target_path).exists() {
                return Some(target_path.to_string());
            }
        }
    }

    None
}

/// Visión en tiempo real (combina captura de pantalla + screenContext de MtGhozt/Capri-Desktop)
pub async fn glance_at_screen_realtime<R: Runtime>(
    app_handle: &AppHandle<R>,
    bridge: &Arc<OpenClawBridge>,
    window_title: &str,
    browser_name: Option<&str>,
    summary: &str,
) -> Result<ChatResponse, String> {
    if let Some(avatar) = app_handle.try_state::<Arc<AvatarSystem<R>>>() {
        avatar.set_state(AvatarState::Thinking);
    }

    let screenshot_path = take_desktop_screenshot().await;
    let status_snapshot = bridge.get_status();
    let history_snapshot = bridge.history.lock().unwrap().clone();
    let screen_ctx = build_realtime_screen_context().await;

    let safe_window_title = if window_title.to_lowercase().contains("connecting to openclaw") {
        "Workspace Fuko (IDE)".to_string()
    } else {
        window_title.to_string()
    };

    let reaction_event = if let Some(b_name) = browser_name {
        if b_name.contains("Antigravity") {
            format!(
                "El usuario tiene en pantalla su editor de código (sesión o archivo: '{}', {}). Haz un comentario directo y breve sobre su código, proyecto o tecnología (1 a 2 oraciones, sin emojis). REGLA ESTRICTA: CERO auto-comentarios. Prohibido decir 'te estoy observando', 'sé lo que haces', 'mis sensores detectan', ni hablar sobre ti misma. Comenta directamente sobre el contenido técnico.",
                safe_window_title, summary
            )
        } else {
            format!(
                "El usuario está viendo en {} la página o búsqueda: '{}' ({}). Haz un comentario agudo y directo sobre ese tema, video, artículo o búsqueda específica (1 a 2 oraciones, sin emojis). REGLA ESTRICTA: CERO auto-comentarios. Prohibido decir 'te estoy observando', 'veo que abriste', 'mis sensores', ni hablar sobre ti misma. Opina directamente sobre el tema en pantalla.",
                b_name, safe_window_title, summary
            )
        }
    } else {
        format!(
            "El usuario está viendo la aplicación o ventana: '{}' ({}). Haz un comentario breve y directo sobre el contenido o tarea (1 a 2 oraciones, sin emojis). REGLA ESTRICTA: CERO auto-comentarios. Prohibido decir 'te estoy observando', 'sé lo que haces' o mencionar tus sensores. Opina directamente sobre el tema.",
            safe_window_title, summary
        )
    };

    let prompt = build_claudia_prompt(
        &reaction_event,
        &status_snapshot,
        &history_snapshot,
        &screen_ctx,
        true,
    );

    let is_openclaw_mode = status_snapshot.mode == "openclaw";

    // 1. Si el usuario está explícitamente en modo OpenClaw, usar Nemotron Vision
    if is_openclaw_mode {
        if let Some(ref img_path) = screenshot_path {
            if let Ok((raw_reply, model_used)) = query_openclaw_cli(
                &prompt,
                Some("nvidia/nemotron-3.5-super-vl-120b-a12b"),
                Some(img_path),
            )
            .await
            {
                let reply = strip_emojis_and_ansi(&raw_reply);
                speak_out_loud_async(reply.clone());
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
                return Ok(ChatResponse { reply, model_used });
            }
        }
    }

    // 2. Visión con Antigravity (preferencia predeterminada y prioritaria)
    let agy_prompt = if let Some(ref img_path) = screenshot_path {
        format!(
            "Mira la captura de pantalla en '{}'.\nComenta DIRECTAMENTE sobre el tema, código, búsqueda, video, diseño o contenido específico que aparece en la pantalla (1 a 2 oraciones breves, agudas, sin emojis).\nREGLA ESTRICTA: CERO auto-comentarios. NUNCA digas 'estoy observándote', 'sé lo que haces', 'mis sensores', 'veo tu pantalla' ni hables sobre ti misma o sobre que estás mirando la pantalla. Comenta únicamente sobre el tema o contenido que aparece en la imagen.",
            img_path
        )
    } else {
        prompt.clone()
    };

    match query_antigravity_cli(&agy_prompt).await {
        Ok((raw_reply, _)) => {
            let reply = strip_emojis_and_ansi(&raw_reply);
            speak_out_loud_async(reply.clone());
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
            return Ok(ChatResponse {
                reply,
                model_used: "antigravity-vision".to_string(),
            });
        }
        Err(e) => {
            eprintln!("[Vision] Antigravity vision fallo ({})", e);
        }
    }

    // 3. Fallback a Nemotron SOLO si no está en modo Antigravity
    if !status_snapshot.mode.contains("antigravity") {
        if let Some(ref img_path) = screenshot_path {
            if let Ok((raw_reply, model_used)) = query_openclaw_cli(
                &prompt,
                Some("nvidia/nemotron-3.5-super-vl-120b-a12b"),
                Some(img_path),
            )
            .await
            {
                let reply = strip_emojis_and_ansi(&raw_reply);
                speak_out_loud_async(reply.clone());
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
                return Ok(ChatResponse { reply, model_used });
            }
        }
    }

    ask_claudia(
        app_handle,
        bridge,
        reaction_event,
        Some(status_snapshot.mode.clone()),
        true,
    )
    .await
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
    proactive: Option<bool>,
    enabled: Option<bool>,
    app_handle: AppHandle<tauri::Wry>,
    bridge: tauri::State<'_, Arc<OpenClawBridge>>,
) -> Result<AiStatus, String> {
    let val = proactive.or(enabled).unwrap_or(true);
    if let Ok(mut st) = bridge.status.lock() {
        st.proactive = val;
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
    let question = custom_question.unwrap_or_else(|| {
        "Mira lo que estoy haciendo ahora en mi pantalla en tiempo real y hazme un comentario directo a tu estilo (sin emojis).".to_string()
    });
    let res = glance_at_screen_realtime(&app_handle, bridge.inner(), &question, None, "Analisis en vivo").await?;
    let _ = app_handle.emit(
        "avatar_speak",
        serde_json::json!({
            "heard": "(vision en vivo)",
            "message": res.reply,
            "model": res.model_used,
            "thinking": false
        }),
    );
    Ok(res)
}

#[tauri::command]
pub async fn list_openclaw_sessions_cmd() -> Result<Vec<SubAgentInfo>, String> {
    let mut items = crate::brain_forks::get_brain_forks_as_subagents();
    let bin = resolve_openclaw_bin();
    if let Ok(out) = Command::new(bin)
        .args(["--log-level", "silent", "sessions", "list", "--json"])
        .output()
        .await
    {
        let stdout = String::from_utf8_lossy(&out.stdout);
        if let Ok(parsed) = serde_json::from_str::<Vec<SubAgentInfo>>(&stdout) {
            items.extend(parsed);
            return Ok(items);
        }
    }
    items.extend(vec![
        SubAgentInfo {
            key: "openclaw-main".to_string(),
            label: "OpenClaw Agent".to_string(),
            model: "nvidia/nemotron-3-ultra-550b-a55b".to_string(),
            status: "ONLINE".to_string(),
            age: "activo".to_string(),
            kind: "local-agent".to_string(),
            tokens: "auto".to_string(),
        },
        SubAgentInfo {
            key: "antigravity-flash".to_string(),
            label: "Antigravity Vision & Fast CLI".to_string(),
            model: "gemini-3.8-flash-high".to_string(),
            status: "ONLINE".to_string(),
            age: "activo".to_string(),
            kind: "antigravity".to_string(),
            tokens: "auto".to_string(),
        },
    ]);
    Ok(items)
}

#[tauri::command]
pub async fn spawn_brain_task_cmd<R: Runtime>(
    app_handle: AppHandle<R>,
    lobe: String,
    instruction: String,
) -> Result<String, String> {
    let confirm = crate::brain_forks::spawn_brain_fork(app_handle, &lobe, &instruction);
    Ok(confirm)
}

#[tauri::command]
pub async fn probe_antigravity_cmd() -> Result<ProbeResult, String> {
    let start = Instant::now();
    match query_antigravity_cli("Responde unicamente: ONLINE").await {
        Ok((reply, model)) => Ok(ProbeResult {
            ok: true,
            latency_ms: start.elapsed().as_millis() as u64,
            reply,
            model,
        }),
        Err(e) => Ok(ProbeResult {
            ok: false,
            latency_ms: start.elapsed().as_millis() as u64,
            reply: e,
            model: "antigravity".to_string(),
        }),
    }
}

#[tauri::command]
pub async fn probe_gemini_argon_cmd() -> Result<ProbeResult, String> {
    probe_antigravity_cmd().await
}
