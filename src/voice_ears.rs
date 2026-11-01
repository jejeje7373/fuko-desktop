// src/voice_ears.rs
// Módulo de Oídos en Tiempo Real (Ears.cs + LocalEars + Gather de capri-desktop) y Voz (Piper TTS)
// para Claudia Desktop. Mantiene faster-whisper pre-cargado en RAM y transmite el nivel del micrófono (0..6)
// en vivo a las 5 barras de onda de TalkPill.

use crate::openclaw_bridge::{ask_claudia, ChatResponse, OpenClawBridge};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

static IS_RECORDING: AtomicBool = AtomicBool::new(false);
static TTS_ENABLED: AtomicBool = AtomicBool::new(true);
static CHATTING: AtomicBool = AtomicBool::new(true);
static MIC_MUTED: AtomicBool = AtomicBool::new(false);
static EARS_READY: AtomicBool = AtomicBool::new(false);
static DAEMON_STARTED: AtomicBool = AtomicBool::new(false);

const TTS_SPEAKING_FLAG: &str = "/tmp/claudia-tts-speaking.flag";
const MIC_MUTED_FLAG: &str = "/tmp/claudia-mic-muted.flag";
const FORCE_FLUSH_FLAG: &str = "/tmp/claudia-mic-stop.flag";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceListenResult {
    pub ok: bool,
    pub heard: String,
    pub reply: String,
    pub model_used: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EarsStatePayload {
    pub chatting: bool,
    pub muted: bool,
    pub ready: bool,
}

fn sync_mute_flag() {
    let should_hold = !CHATTING.load(Ordering::SeqCst) || MIC_MUTED.load(Ordering::SeqCst);
    if should_hold {
        let _ = std::fs::write(MIC_MUTED_FLAG, "1");
    } else {
        let _ = std::fs::remove_file(MIC_MUTED_FLAG);
    }
}

fn emit_ears_state(app_handle: &AppHandle<tauri::Wry>) {
    let payload = EarsStatePayload {
        chatting: CHATTING.load(Ordering::SeqCst),
        muted: MIC_MUTED.load(Ordering::SeqCst),
        ready: EARS_READY.load(Ordering::SeqCst),
    };
    let _ = app_handle.emit("ears_state", payload);
}

/// Sintetiza y reproduce en voz alta la respuesta de Claudia usando Piper TTS local (`es_AR-daniela-high.onnx`).
/// Mientras habla, activa `TTS_SPEAKING_FLAG` (igual que `voiceUntil` / `WakeWord.Held` en capri-desktop)
/// para que el micrófono no transcriba su propia voz.
pub fn speak_out_loud_async(text: String) {
    if !TTS_ENABLED.load(Ordering::SeqCst) {
        return;
    }
    let clean: String = text
        .chars()
        .filter(|c| !matches!(c, '*' | '`' | '#' | '_' | '[' | ']'))
        .take(360)
        .collect();
    if clean.trim().is_empty() {
        return;
    }

    tauri::async_runtime::spawn(async move {
        let piper_bin = "/home/jejej/.local/bin/piper";
        let model_path = "/home/jejej/tts_models/piper/es_AR-daniela-high.onnx";
        let out_wav = "/tmp/claudia-tts-reply.wav";

        if !Path::new(piper_bin).exists() || !Path::new(model_path).exists() {
            return;
        }

        // Silenciar temporalmente los oídos mientras Claudia habla por los parlantes
        let _ = std::fs::write(TTS_SPEAKING_FLAG, "1");

        let mut cmd = Command::new("bash");
        cmd.env("XDG_RUNTIME_DIR", "/run/user/1000")
            .env("PULSE_SERVER", "unix:/run/user/1000/pulse/native")
            .env_remove("SNAP")
            .env_remove("SNAP_NAME")
            .env_remove("SNAP_LIBRARY_PATH")
            .arg("-c")
            .arg(format!(
                "printf %s {} | {} --model {} --output_file {} >/dev/null 2>&1 && paplay {} >/dev/null 2>&1",
                shell_escape(&clean),
                piper_bin,
                model_path,
                out_wav,
                out_wav
            ))
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        let _ = tokio::time::timeout(tokio::time::Duration::from_secs(22), cmd.status()).await;
        // Pequeña cola de 250ms para que la reverberación de la sala se disipe antes de reabrir el mic
        tokio::time::sleep(tokio::time::Duration::from_millis(250)).await;
        let _ = std::fs::remove_file(TTS_SPEAKING_FLAG);
    });
}

fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Inicia el demonio persistente `claudia_ears_daemon.py` que mantiene faster-whisper en RAM,
/// envía niveles 0..6 en tiempo real para las 5 barras de onda de `TalkPill`, y transcribe al instante.
pub fn start_ears_daemon(app_handle: AppHandle<tauri::Wry>, bridge: Arc<OpenClawBridge>) {
    if DAEMON_STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    sync_mute_flag();

    tauri::async_runtime::spawn(async move {
        let script_path =
            "/home/jejej/.openclaw/workspace/claudia-desktop/scripts/claudia_ears_daemon.py";

        loop {
            let mut cmd = Command::new("python3");
            cmd.env("XDG_RUNTIME_DIR", "/run/user/1000")
                .env("PULSE_SERVER", "unix:/run/user/1000/pulse/native")
                .env("PYTHONUNBUFFERED", "1")
                .env_remove("SNAP")
                .env_remove("SNAP_NAME")
                .env_remove("SNAP_LIBRARY_PATH")
                .arg("-u")
                .arg(script_path)
                .stdout(Stdio::piped())
                .stderr(Stdio::null());

            if let Ok(mut child) = cmd.spawn() {
                if let Some(stdout) = child.stdout.take() {
                    let mut lines = BufReader::new(stdout).lines();
                    while let Ok(Some(line)) = lines.next_line().await {
                        let Ok(val) = serde_json::from_str::<serde_json::Value>(&line) else {
                            continue;
                        };
                        let Some(ev_type) = val.get("type").and_then(|t| t.as_str()) else {
                            continue;
                        };

                        match ev_type {
                            "ready" => {
                                EARS_READY.store(true, Ordering::SeqCst);
                                let mic_name = val
                                    .get("mic")
                                    .and_then(|m| m.as_str())
                                    .unwrap_or("default");
                                println!(
                                    "[Claudia Ears] Oídos locales listos en micrófono: {}",
                                    mic_name
                                );
                                emit_ears_state(&app_handle);
                            }
                            "level" => {
                                let lvl = val
                                    .get("level")
                                    .and_then(|l| l.as_i64())
                                    .unwrap_or(0)
                                    .clamp(0, 6) as i32;
                                let active = CHATTING.load(Ordering::SeqCst)
                                    && !MIC_MUTED.load(Ordering::SeqCst);
                                let _ = app_handle.emit("ears_level", if active { lvl } else { 0 });
                            }
                            "speech_start" => {
                                if CHATTING.load(Ordering::SeqCst)
                                    && !MIC_MUTED.load(Ordering::SeqCst)
                                {
                                    let _ = app_handle.emit("ears_speech_start", true);
                                }
                            }
                            "transcribing" => {
                                if CHATTING.load(Ordering::SeqCst)
                                    && !MIC_MUTED.load(Ordering::SeqCst)
                                {
                                    let _ = app_handle.emit("ears_level", 0);
                                    let _ = app_handle.emit(
                                        "avatar_speak",
                                        serde_json::json!({
                                            "heard": "(escuchándote...)",
                                            "message": "...",
                                            "model": "ears",
                                            "thinking": true
                                        }),
                                    );
                                }
                            }
                            "heard" => {
                                if !CHATTING.load(Ordering::SeqCst)
                                    || MIC_MUTED.load(Ordering::SeqCst)
                                {
                                    continue;
                                }
                                let Some(text) = val
                                    .get("text")
                                    .and_then(|t| t.as_str())
                                    .map(|s| s.trim().to_string())
                                else {
                                    continue;
                                };
                                if text.is_empty() {
                                    continue;
                                }

                                println!("[Claudia Ears] Escuchado: \"{}\"", text);
                                let _ = app_handle.emit(
                                    "avatar_speak",
                                    serde_json::json!({
                                        "heard": text,
                                        "message": "...",
                                        "model": "ears",
                                        "thinking": true
                                    }),
                                );

                                // Procesar la orden o conversación en una tarea para no bloquear la lectura de niveles
                                let app_c = app_handle.clone();
                                let bridge_c = bridge.clone();
                                let heard_text = text.clone();
                                tauri::async_runtime::spawn(async move {
                                    let _ = std::fs::write(TTS_SPEAKING_FLAG, "1");
                                    match ask_claudia(
                                        &app_c,
                                        &bridge_c,
                                        heard_text.clone(),
                                        None,
                                        false,
                                    )
                                    .await
                                    {
                                        Ok(ChatResponse { reply, model_used }) => {
                                            let _ = app_c.emit(
                                                "avatar_speak",
                                                serde_json::json!({
                                                    "heard": heard_text,
                                                    "message": reply.clone(),
                                                    "model": model_used,
                                                    "thinking": false
                                                }),
                                            );
                                            if TTS_ENABLED.load(Ordering::SeqCst) {
                                                speak_out_loud_async(reply);
                                            } else {
                                                let _ = std::fs::remove_file(TTS_SPEAKING_FLAG);
                                            }
                                        }
                                        Err(e) => {
                                            let _ = std::fs::remove_file(TTS_SPEAKING_FLAG);
                                            let _ = app_c.emit(
                                                "avatar_speak",
                                                serde_json::json!({
                                                    "heard": heard_text,
                                                    "message": format!("Te escuché, pero hubo un glitch: {}", e),
                                                    "model": "error",
                                                    "thinking": false
                                                }),
                                            );
                                        }
                                    }
                                });
                            }
                            _ => {}
                        }
                    }
                }
                let _ = child.wait().await;
            }
            EARS_READY.store(false, Ordering::SeqCst);
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        }
    });
}

/// Obtiene el estado actual de `TalkPill` / `Ears` (`chatting`, `muted`, `ready`)
#[tauri::command]
pub fn get_ears_state_cmd() -> Result<EarsStatePayload, String> {
    Ok(EarsStatePayload {
        chatting: CHATTING.load(Ordering::SeqCst),
        muted: MIC_MUTED.load(Ordering::SeqCst),
        ready: EARS_READY.load(Ordering::SeqCst),
    })
}

/// Equivalente a `ToggleChat()` en Capri.cs (botón `TALK` en `TalkPill`):
/// Activa o desactiva el chat de voz en vivo con Claudia.
#[tauri::command]
pub fn toggle_voice_chat_cmd(
    enabled: Option<bool>,
    app_handle: AppHandle<tauri::Wry>,
) -> Result<EarsStatePayload, String> {
    let next = enabled.unwrap_or_else(|| !CHATTING.load(Ordering::SeqCst));
    CHATTING.store(next, Ordering::SeqCst);
    if next {
        MIC_MUTED.store(false, Ordering::SeqCst);
    }
    sync_mute_flag();
    let _ = app_handle.emit("ears_level", 0);
    emit_ears_state(&app_handle);

    Ok(EarsStatePayload {
        chatting: next,
        muted: MIC_MUTED.load(Ordering::SeqCst),
        ready: EARS_READY.load(Ordering::SeqCst),
    })
}

/// Equivalente a `ToggleMic()` en Capri.cs (botón cuadrado de micrófono a la derecha de `TalkPill`):
/// Silencia o reactiva el micrófono durante el chat de voz (`MUTED` en rojo vs `TALK` en cian).
#[tauri::command]
pub fn toggle_mic_mute_cmd(
    muted: Option<bool>,
    app_handle: AppHandle<tauri::Wry>,
) -> Result<EarsStatePayload, String> {
    let next = muted.unwrap_or_else(|| !MIC_MUTED.load(Ordering::SeqCst));
    MIC_MUTED.store(next, Ordering::SeqCst);
    sync_mute_flag();
    let _ = app_handle.emit("ears_level", 0);
    emit_ears_state(&app_handle);

    Ok(EarsStatePayload {
        chatting: CHATTING.load(Ordering::SeqCst),
        muted: next,
        ready: EARS_READY.load(Ordering::SeqCst),
    })
}

/// Fuerza el envío inmediato de lo que se esté hablando ahora mismo
#[tauri::command]
pub fn stop_mic_recording_cmd() -> Result<(), String> {
    let _ = std::fs::write(FORCE_FLUSH_FLAG, "stop");
    Ok(())
}

/// Activa o desactiva la síntesis de voz (Piper TTS) de Claudia
#[tauri::command]
pub fn set_tts_enabled_cmd(enabled: bool) -> Result<bool, String> {
    TTS_ENABLED.store(enabled, Ordering::SeqCst);
    Ok(enabled)
}

/// Compatibilidad con los comandos anteriores
#[tauri::command]
pub async fn set_continuous_listen_cmd(
    enabled: bool,
    app_handle: AppHandle<tauri::Wry>,
) -> Result<bool, String> {
    let st = toggle_voice_chat_cmd(Some(enabled), app_handle)?;
    Ok(st.chatting)
}

#[tauri::command]
pub async fn listen_and_ask_cmd(
    _mode: Option<String>,
    app_handle: AppHandle<tauri::Wry>,
) -> Result<VoiceListenResult, String> {
    let st = toggle_voice_chat_cmd(None, app_handle)?;
    Ok(VoiceListenResult {
        ok: true,
        heard: "(voice)".to_string(),
        reply: if st.chatting {
            "Chat de voz libre activado. Háblame cuando quieras, choom.".to_string()
        } else {
            "Chat de voz apagado. Haz click en las barras de TALK para volver a hablar.".to_string()
        },
        model_used: "ears".to_string(),
        error: None,
    })
}
