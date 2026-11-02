// src/voice_ears.rs
// Sistema de oídos en tiempo real (faster-whisper daemon) + Voz Sintética No-Humana estilo MtGhozt/Capri-Desktop.
// CERO voz humana: genera ondas cuadradas/triangulares pentatónicas (La menor) y arpegios cyberpunk al responder por texto o por voz.

use crate::openclaw_bridge::{ask_claudia, OpenClawBridge};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command as StdCommand, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

static CHATTING_ACTIVE: AtomicBool = AtomicBool::new(true);
static MIC_MUTED: AtomicBool = AtomicBool::new(false);
static DAEMON_READY: AtomicBool = AtomicBool::new(false);
static SYNTH_SOUND_ENABLED: AtomicBool = AtomicBool::new(true);

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

fn current_ears_state() -> EarsStatePayload {
    EarsStatePayload {
        chatting: CHATTING_ACTIVE.load(Ordering::Relaxed),
        muted: MIC_MUTED.load(Ordering::Relaxed),
        ready: DAEMON_READY.load(Ordering::Relaxed),
    }
}

/// Sintetiza un archivo WAV PCM 16-bit mono (22050 Hz) con los bleeps no-humanos exactos de MtGhozt/Capri-Desktop (voice.js):
fn append_chord_note(
    samples: &mut Vec<i16>,
    sample_rate: u32,
    freqs: &[(f32, f32, bool)],
    dur_sec: f32,
    pause_sec: f32,
) {
    let num_samples = (dur_sec * sample_rate as f32) as usize;
    let attack = (0.004 * sample_rate as f32).max(1.0);
    for i in 0..num_samples {
        let t = i as f32 / sample_rate as f32;
        let env = if (i as f32) < attack {
            (i as f32) / attack
        } else {
            let rem = 1.0 - ((i as f32 - attack) / (num_samples as f32 - attack).max(1.0));
            rem * rem
        };
        let mut val: f32 = 0.0;
        for &(f, amp, is_square) in freqs {
            let phase = (t * f).fract();
            let wave = if is_square {
                if phase < 0.5 { 1.0 } else { -1.0 }
            } else {
                // Onda triangular
                4.0 * (phase - 0.5).abs() - 1.0
            };
            val += wave * amp * env;
        }
        let clamped = (val.clamp(-0.95, 0.95) * 32767.0) as i16;
        samples.push(clamped);
    }
    let pause_samples = (pause_sec * sample_rate as f32) as usize;
    samples.extend(std::iter::repeat(0i16).take(pause_samples));
}

/// - Arpegio inicial de 4 notas (659, 784, 988, 1318 Hz) para avisar al instante que Capri respondió
/// - Secuencia de bleeps pentatónicos a 3 voces (square fundamental + triangle quinta + square sub-octava) según el texto
fn build_capri_synth_wav(text: &str) -> Vec<u8> {
    let sample_rate: u32 = 22050;
    let mut samples: Vec<i16> = Vec::new();

    // 1. Arpegio corto de aviso de respuesta (wakeArp de Capri-Desktop: E5, G5, B5, E6)
    for &arp_f in &[659.25_f32, 783.99, 987.77, 1318.51] {
        append_chord_note(&mut samples, sample_rate, &[(arp_f, 0.16, true)], 0.038, 0.014);
    }

    // Pequeña pausa de 35ms antes de los bleeps de las sílabas/letras
    samples.extend(std::iter::repeat(0i16).take((0.035 * sample_rate as f32) as usize));

    // 2. Escala pentatónica de La menor de MtGhozt/Capri-Desktop: MIDI [62, 65, 67, 69, 72, 74, 77, 79]
    let midi_notes: [i32; 8] = [62, 65, 67, 69, 72, 74, 77, 79];
    let mood_shift: i32 = if text.contains("[emocion]") {
        5
    } else if text.contains("[aburrida]") {
        -5
    } else {
        0
    };

    let clean_chars: Vec<char> = text
        .chars()
        .filter(|c| !c.is_whitespace())
        .take(36) // Limitar a ~36 bleeps para que dure entre 0.8s y 2.2s y sea agradable
        .collect();

    for (idx, ch) in clean_chars.iter().enumerate() {
        if idx % 2 != 0 {
            continue;
        }
        let note_idx = ((*ch as usize).wrapping_add(idx * 3)) % midi_notes.len();
        let midi = midi_notes[note_idx] + mood_shift;
        let f = 440.0_f32 * 2.0_f32.powf((midi as f32 - 69.0) / 12.0);
        let pause = if *ch == '.' || *ch == '!' || *ch == '?' {
            0.075
        } else if *ch == ',' {
            0.045
        } else {
            0.018
        };
        append_chord_note(
            &mut samples,
            sample_rate,
            &[
                (f, 0.15, true),           // Fundamental square
                (f * 1.498, 0.11, false),  // Fifth up triangle
                (f * 0.501, 0.08, true),   // Octave down square
            ],
            0.032,
            pause,
        );
    }

    // Construir cabecera RIFF WAV
    let data_len = (samples.len() * 2) as u32;
    let file_len = 36 + data_len;
    let mut wav = Vec::with_capacity(file_len as usize + 8);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&file_len.to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes()); // Subchunk1Size
    wav.extend_from_slice(&1u16.to_le_bytes());  // PCM = 1
    wav.extend_from_slice(&1u16.to_le_bytes());  // Mono = 1
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&(sample_rate * 2).to_le_bytes()); // ByteRate
    wav.extend_from_slice(&2u16.to_le_bytes());  // BlockAlign
    wav.extend_from_slice(&16u16.to_le_bytes()); // BitsPerSample
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        wav.extend_from_slice(&s.to_le_bytes());
    }
    wav
}

/// Reproduce sonido sintético no-humano estilo Capri-Desktop (nunca usa voz humana de Piper)
pub fn speak_out_loud_async(text: String) {
    if !SYNTH_SOUND_ENABLED.load(Ordering::Relaxed) {
        return;
    }
    let trimmed = text.trim().to_string();
    if trimmed.is_empty() {
        return;
    }

    std::thread::spawn(move || {
        let flag_path = "/tmp/claudia-tts-speaking.flag";
        let wav_path = "/tmp/claudia-synth-blip.wav";
        let _ = std::fs::write(flag_path, "1");

        let wav_bytes = build_capri_synth_wav(&trimmed);
        if let Ok(mut file) = std::fs::File::create(wav_path) {
            let _ = file.write_all(&wav_bytes);
        }

        // Reproducir con paplay, pw-play o aplay
        let played = StdCommand::new("paplay")
            .arg(wav_path)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        if !played {
            let _ = StdCommand::new("aplay")
                .args(["-q", wav_path])
                .status();
        }

        let _ = std::fs::remove_file(flag_path);
    });
}

/// Inicia el daemon de escucha en tiempo real (claudia_ears_daemon.py)
pub fn start_ears_daemon(app_handle: AppHandle<tauri::Wry>, bridge: Arc<OpenClawBridge>) {
    std::thread::spawn(move || {
        let mut script_path = String::new();
        for cand in [
            "scripts/claudia_ears_daemon.py",
            "../scripts/claudia_ears_daemon.py",
            "/home/jejej/fuko-desktop/scripts/claudia_ears_daemon.py",
            "/home/jejej/.openclaw/workspace/claudia-desktop/scripts/claudia_ears_daemon.py",
        ] {
            if Path::new(cand).exists() {
                script_path = cand.to_string();
                break;
            }
        }

        if script_path.is_empty() {
            return;
        }

        let child_res = StdCommand::new("python3")
            .arg("-u")
            .arg(&script_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();

        let Ok(mut child) = child_res else {
            return;
        };

        let Some(stdout) = child.stdout.take() else {
            return;
        };

        let reader = BufReader::new(stdout);
        for line in reader.lines().flatten() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) else {
                continue;
            };
            let ev_type = val.get("event").and_then(|v| v.as_str()).unwrap_or("");

            match ev_type {
                "ready" => {
                    DAEMON_READY.store(true, Ordering::Relaxed);
                    let _ = app_handle.emit("ears_state", current_ears_state());
                }
                "level" => {
                    if CHATTING_ACTIVE.load(Ordering::Relaxed) && !MIC_MUTED.load(Ordering::Relaxed)
                    {
                        let lvl = val.get("level").and_then(|v| v.as_u64()).unwrap_or(0);
                        let _ = app_handle.emit("ears_level", lvl);
                    }
                }
                "speech_start" => {
                    if CHATTING_ACTIVE.load(Ordering::Relaxed) && !MIC_MUTED.load(Ordering::Relaxed)
                    {
                        let _ = app_handle.emit("ears_speech_start", true);
                    }
                }
                "heard" => {
                    if !CHATTING_ACTIVE.load(Ordering::Relaxed) || MIC_MUTED.load(Ordering::Relaxed)
                    {
                        continue;
                    }
                    let text = val
                        .get("text")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    if text.len() < 2 {
                        continue;
                    }

                    let _ = app_handle.emit("ears_speech_recognized", &text);
                    let _ = app_handle.emit("mic_heard_preview", &text);

                    let app_clone = app_handle.clone();
                    let bridge_clone = bridge.clone();
                    tauri::async_runtime::spawn(async move {
                        if let Ok(res) =
                            ask_claudia(&app_clone, &bridge_clone, text.clone(), None, false).await
                        {
                            let _ = app_clone.emit(
                                "avatar_speak",
                                serde_json::json!({
                                    "heard": text,
                                    "message": res.reply,
                                    "model": res.model_used,
                                    "thinking": false
                                }),
                            );
                        }
                    });
                }
                _ => {}
            }
        }
    });
}

#[tauri::command]
pub fn get_ears_state_cmd() -> Result<EarsStatePayload, String> {
    Ok(current_ears_state())
}

#[tauri::command]
pub fn toggle_voice_chat_cmd(
    enabled: Option<bool>,
    app_handle: AppHandle<tauri::Wry>,
) -> Result<EarsStatePayload, String> {
    let next = enabled.unwrap_or_else(|| !CHATTING_ACTIVE.load(Ordering::Relaxed));
    CHATTING_ACTIVE.store(next, Ordering::Relaxed);
    let st = current_ears_state();
    let _ = app_handle.emit("ears_state", &st);
    Ok(st)
}

#[tauri::command]
pub fn toggle_mic_mute_cmd(
    muted: Option<bool>,
    app_handle: AppHandle<tauri::Wry>,
) -> Result<EarsStatePayload, String> {
    let next = muted.unwrap_or_else(|| !MIC_MUTED.load(Ordering::Relaxed));
    MIC_MUTED.store(next, Ordering::Relaxed);
    let st = current_ears_state();
    let _ = app_handle.emit("ears_state", &st);
    Ok(st)
}

#[tauri::command]
pub fn stop_mic_recording_cmd() -> Result<(), String> {
    Ok(())
}

#[tauri::command]
pub fn set_tts_enabled_cmd(enabled: bool) -> Result<bool, String> {
    SYNTH_SOUND_ENABLED.store(enabled, Ordering::Relaxed);
    Ok(enabled)
}

#[tauri::command]
pub async fn set_continuous_listen_cmd(
    enabled: bool,
    app_handle: AppHandle<tauri::Wry>,
) -> Result<bool, String> {
    CHATTING_ACTIVE.store(enabled, Ordering::Relaxed);
    let _ = app_handle.emit("ears_state", current_ears_state());
    Ok(enabled)
}

#[tauri::command]
pub async fn listen_and_ask_cmd(
    _mode: Option<String>,
    app_handle: AppHandle<tauri::Wry>,
) -> Result<VoiceListenResult, String> {
    let st = toggle_voice_chat_cmd(Some(true), app_handle)?;
    Ok(VoiceListenResult {
        ok: st.chatting,
        heard: String::new(),
        reply: "Escucha activa encendida.".to_string(),
        model_used: "ears".to_string(),
        error: None,
    })
}
