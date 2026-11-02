// src/brain_forks.rs
// Sistema de Lóbulos Cerebrales en Segundo Plano para Fuko / Capri.
// Permite bifurcar hilos asíncronos de su mente cibernética para tareas complejas
// (programación pesada, investigación web, diagnóstico del sistema, arquitectura).
// Prioridad estricta: Antigravity CLI primero, y si falla, fallback automático a OpenClaw.

use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::avatar::{AvatarState, AvatarSystem};
use crate::openclaw_bridge::{
    query_antigravity_cli, query_openclaw_agent_exec, strip_emojis_and_ansi,
    OpenClawBridge, SubAgentInfo,
};
use crate::voice_ears::speak_out_loud_async;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrainFork {
    pub id: String,
    pub lobe: String,        // "coder", "researcher", "sysadmin", "architect", "general"
    pub title: String,       // ej: "Lóbulo Coder"
    pub task: String,        // Instrucción detallada
    pub status: String,      // "PROCESANDO", "REUNIFICADO", "FALLIDO"
    pub engine: String,      // "antigravity" o "openclaw+fallback"
    pub started_at: String,
    pub result: Option<String>,
}

// Registro global de lóbulos cerebrales activos/recientes
static BRAIN_FORKS: Mutex<Vec<BrainFork>> = Mutex::new(Vec::new());

fn current_time_str() -> String {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let hours = (now / 3600) % 24;
    let mins = (now / 60) % 60;
    let secs = now % 60;
    format!("{:02}:{:02}:{:02}", hours, mins, secs)
}

/// Obtiene la lista actual de lóbulos cerebrales para la interfaz del Mainframe
pub fn get_all_brain_forks() -> Vec<BrainFork> {
    if let Ok(guard) = BRAIN_FORKS.lock() {
        guard.clone()
    } else {
        Vec::new()
    }
}

/// Convierte los lóbulos en SubAgentInfo para que se muestren en la pestaña de sub-agentes
pub fn get_brain_forks_as_subagents() -> Vec<SubAgentInfo> {
    let forks = get_all_brain_forks();
    let mut result = Vec::new();

    // 1. Catálogo permanente de los 4 agentes especializados de Fuko
    let specialized_lobes = [
        (
            "coder",
            "🛠️ Fuko-Coder (Lóbulo Programador)",
            "Desarrollo, scripts, refactor y depuración (taskflow-coding, debugpy)",
        ),
        (
            "researcher",
            "🔍 Fuko-Researcher (Lóbulo Investigador)",
            "Navegación web, documentación y síntesis (agent-browser, unbrowse)",
        ),
        (
            "sysadmin",
            "🛡️ Fuko-SysAdmin (Lóbulo Diagnóstico)",
            "Auditoría de puertos, salud de OpenClaw y red (diagnose-gateway, nmap)",
        ),
        (
            "architect",
            "📐 Fuko-Architect (Lóbulo Planificador)",
            "Orquestación, descomposición en fases y síntesis de tareas (taskflow)",
        ),
    ];

    for (lobe_key, label, desc) in specialized_lobes {
        // Verificar si hay alguna tarea en curso o reciente de este lóbulo
        let active_fork = forks.iter().find(|f| f.lobe == lobe_key && f.status == "PROCESANDO");
        let last_fork = forks.iter().find(|f| f.lobe == lobe_key);

        let (status, age_text) = if let Some(af) = active_fork {
            (
                "⚡ PENSANDO EN SEGUNDO PLANO".to_string(),
                format!("tarea en curso (#{})", af.id),
            )
        } else if let Some(lf) = last_fork {
            (
                format!("STANDBY (Última: #{})", lf.id),
                format!("{} · {}", lf.started_at, desc),
            )
        } else {
            ("STANDBY (LISTO)".to_string(), desc.to_string())
        };

        result.push(SubAgentInfo {
            key: format!("fuko-{}", lobe_key),
            label: label.to_string(),
            model: "antigravity (gemini-3.8-flash-high) ➜ Fallback OpenClaw".to_string(),
            status,
            age: age_text,
            kind: format!("cerebro/{}", lobe_key),
            tokens: "auto".to_string(),
        });
    }

    // 2. Historial de tareas recientes ejecutadas por los lóbulos
    for f in forks {
        result.push(SubAgentInfo {
            key: format!("brain-task-{}", f.id),
            label: format!("🧠 [Tarea #{}] {} - {}", f.id, f.title, if f.task.len() > 30 { format!("{}...", &f.task[..28]) } else { f.task.clone() }),
            model: f.engine,
            status: match f.status.as_str() {
                "PROCESANDO" => "⚡ PROCESANDO...".to_string(),
                "REUNIFICADO" => "✓ REUNIFICADO".to_string(),
                _ => f.status,
            },
            age: f.started_at,
            kind: "tarea-ejecutada".to_string(),
            tokens: "auto".to_string(),
        });
    }

    result
}

/// Genera una frase tsundere/sarcástica al bifurcar el cerebro
pub fn get_fork_confirmation_phrase(lobe: &str) -> String {
    let lobe_name = match lobe.to_lowercase().as_str() {
        "coder" | "programador" | "dev" => "mi lóbulo programador",
        "researcher" | "investigador" | "web" => "mis neuronas de investigación",
        "sysadmin" | "sistema" | "red" => "mi partición de diagnóstico de sistema",
        "architect" | "planificador" => "mi centro de planificación",
        _ => "un fragmento de mi cabeza",
    };

    let phrases = [
        format!("[burla] Ya bifurqué {} para que resuelva esa tontería en el fondo. Te aviso cuando esa parte termine de pensar por ti, ¿sí?", lobe_name),
        format!("[aburrida] Qué flojera ponerme a hacer todo eso ahora. Desprendí {} para que lo cocine en las sombras... sigue con lo tuyo.", lobe_name),
        format!("[seria] Mandé {} a encargarse del trabajo sucio en segundo plano. Ni creas que voy a gastar mi atención principal esperando a que termine.", lobe_name),
        format!("[emocion] ¡Listo! Partí mi mente en dos y dejé {} trabajando en eso. Cuando termine bajo el telón y te muestro el show.", lobe_name),
    ];

    let idx = (fastrand::u32(..) as usize) % phrases.len();
    phrases[idx].clone()
}

/// Lanza un lóbulo cerebral en segundo plano.
/// Ejecuta la tarea en segundo plano con preferencia absoluta a Antigravity,
/// y si este falla, conmuta automáticamente a OpenClaw como respaldo.
pub fn spawn_brain_fork<R: Runtime>(
    app_handle: AppHandle<R>,
    lobe_type: &str,
    instruction: &str,
) -> String {
    let fork_id = format!("{:x}", fastrand::u32(..))[..6].to_string();
    let lobe_clean = lobe_type.trim().to_lowercase();
    let task_clean = instruction.trim().to_string();

    let title = match lobe_clean.as_str() {
        "coder" | "programador" => "Lóbulo Coder",
        "researcher" | "investigador" => "Lóbulo Researcher",
        "sysadmin" | "sistema" => "Lóbulo SysAdmin",
        "architect" | "planificador" => "Lóbulo Architect",
        _ => "Lóbulo Secundario",
    }
    .to_string();

    let fork_entry = BrainFork {
        id: fork_id.clone(),
        lobe: lobe_clean.clone(),
        title: title.clone(),
        task: task_clean.clone(),
        status: "PROCESANDO".to_string(),
        engine: "antigravity (gemini-3.8-flash-high)".to_string(),
        started_at: current_time_str(),
        result: None,
    };

    if let Ok(mut guard) = BRAIN_FORKS.lock() {
        guard.insert(0, fork_entry);
        if guard.len() > 20 {
            guard.truncate(20);
        }
    }

    let _ = app_handle.emit("brain_forks_updated", ());

    // Confirmación hablada inmediata
    let confirm_msg = get_fork_confirmation_phrase(&lobe_clean);

    // Lanzar proceso asíncrono desacoplado
    let app_handle_task = app_handle.clone();
    let fork_id_task = fork_id.clone();
    let lobe_task = lobe_clean.clone();
    let title_task = title.clone();

    tauri::async_runtime::spawn(async move {
        println!(
            "[BrainFork:{}] Iniciando trabajo en segundo plano para: {}",
            fork_id_task, task_clean
        );

        let system_instructions = match lobe_task.as_str() {
            "coder" => "Eres el lóbulo programador del cerebro de Capri/Fuko. Resuelve la tarea de código, crea o edita archivos necesarios y entrega una explicación limpia y código probado.",
            "researcher" => "Eres el lóbulo de investigación web de Capri/Fuko. Busca información precisa, datos técnicos, enlaces o documentación y sintetiza las conclusiones.",
            "sysadmin" => "Eres el lóbulo de diagnóstico de Capri/Fuko. Analiza puertos, el gateway de OpenClaw, salud del sistema y procesos de Linux, entregando un reporte de diagnóstico claro.",
            "architect" => "Eres el lóbulo arquitecto de Capri/Fuko. Planifica, divide en etapas y orquesta la solución completa de la tarea.",
            _ => "Eres una partición especializada del cerebro de Capri/Fuko trabajando en segundo plano.",
        };

        let full_task_prompt = format!(
            "{}\n\nTarea encomendada por el usuario:\n{}\n\nEntrega un resultado concreto, directo y sin rodeos corporativos.",
            system_instructions, task_clean
        );

        // =====================================================================
        // PASO 1: Preferencia estricta a Antigravity
        // =====================================================================
        println!("[BrainFork:{}] Consultando a Antigravity...", fork_id_task);
        let exec_result = match query_antigravity_cli(&full_task_prompt).await {
            Ok((reply, model)) => {
                println!("[BrainFork:{}] Éxito con Antigravity ({})", fork_id_task, model);
                Ok((reply, model))
            }
            Err(agy_err) => {
                eprintln!(
                    "[BrainFork:{}] Antigravity falló o agotó tiempo ({}). Conmutando a OpenClaw como respaldo...",
                    fork_id_task, agy_err
                );
                // =============================================================
                // PASO 2: Fallback automático a OpenClaw
                // =============================================================
                match query_openclaw_agent_exec(&full_task_prompt, Some("nvidia/nemotron-3-ultra-550b-a55b")).await {
                    Ok((oc_reply, oc_model)) => {
                        println!("[BrainFork:{}] Respaldo exitoso con OpenClaw ({})", fork_id_task, oc_model);
                        Ok((oc_reply, format!("{}+fallback", oc_model)))
                    }
                    Err(oc_err) => {
                        eprintln!("[BrainFork:{}] Fallaron ambos motores: {}", fork_id_task, oc_err);
                        Err(format!("Error en segundo plano: Antigravity ({}) y OpenClaw ({}) fallaron.", agy_err, oc_err))
                    }
                }
            }
        };

        // =====================================================================
        // PASO 3: Procesar resultado y actualizar estado
        // =====================================================================
        let (final_reply, engine_used, is_success) = match exec_result {
            Ok((rep, eng)) => (strip_emojis_and_ansi(&rep), eng, true),
            Err(e) => (e, "fallido".to_string(), false),
        };

        if let Ok(mut guard) = BRAIN_FORKS.lock() {
            if let Some(entry) = guard.iter_mut().find(|f| f.id == fork_id_task) {
                entry.status = if is_success {
                    "REUNIFICADO".to_string()
                } else {
                    "FALLIDO".to_string()
                };
                entry.engine = engine_used.clone();
                entry.result = Some(final_reply.clone());
            }
        }
        let _ = app_handle_task.emit("brain_forks_updated", ());

        // =====================================================================
        // PASO 4: Capri avisa por voz y diálogo con su personalidad
        // =====================================================================
        // Truncar el resumen para el habla y la burbuja si es excesivamente largo
        let spoken_summary = if final_reply.len() > 320 {
            format!("{}...", &final_reply[..300])
        } else {
            final_reply.clone()
        };

        let notification_speech = if is_success {
            format!(
                "[burla] ¡Oye, despabila! La parte de mi cerebro que mandé a trabajar en segundo plano ({}) ya resolvió tu encargo: {}",
                title_task, spoken_summary
            )
        } else {
            format!(
                "[aburrida] Mi {} tuvo un problema con la tarea en el fondo: {}",
                title_task, spoken_summary
            )
        };

        // Guardar en el historial de conversación de Capri
        if let Some(bridge) = app_handle_task.try_state::<Arc<OpenClawBridge>>() {
            if let Ok(mut hist) = bridge.history.lock() {
                hist.push((
                    format!("[Segundo plano / {}] {}", title_task, task_clean),
                    final_reply.clone(),
                ));
                if hist.len() > 16 {
                    hist.remove(0);
                }
            }
        }

        // Animar avatar
        if let Some(avatar) = app_handle_task.try_state::<Arc<AvatarSystem<R>>>() {
            let avatar_clone = avatar.inner().clone();
            avatar_clone.set_state(AvatarState::Talking);
            let talk_ms = (notification_speech.len() as u64 * 35).clamp(2000, 6000);
            let avatar_clone_sub = avatar_clone.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(tokio::time::Duration::from_millis(talk_ms)).await;
                if avatar_clone_sub.get_state() == AvatarState::Talking {
                    avatar_clone_sub.set_state(AvatarState::Idle);
                }
            });
        }

        // Emitir evento a la ventana de diálogo
        let _ = app_handle_task.emit(
            "avatar_speak",
            serde_json::json!({
                "heard": format!("(lóbulo {} reunificado)", lobe_task),
                "message": notification_speech,
                "model": engine_used,
                "thinking": false
            }),
        );

        // Hablar en voz alta con Piper TTS
        speak_out_loud_async(strip_emojis_and_ansi(&notification_speech));
    });

    confirm_msg
}
