#!/usr/bin/env python3
"""
claudia_ears_daemon.py - Demonio en tiempo real de Oídos Locales para Claudia Desktop.
Implementa fielmente Ears.cs + LocalEars + Gather() de capri-desktop:
- Mantiene faster-whisper ('base', int8) pre-cargado en RAM para transcripción instantánea (<0.35s).
- Lee continuamente bloques de 100ms (1600 muestras a 16kHz) desde PipeWire/PulseAudio (parec).
- Aplica el filtro paso-bajo de voz de Capri (VoiceLevel bajo ~900Hz) y seguimiento adaptativo
  del ruido de sala (room) con preroll de 400ms, calibrado para el DMIC Intel Skylake del notebook.
- Emite eventos JSON en tiempo real por stdout:
  * {"type": "ready", "mic": "..."}
  * {"type": "level", "level": 0..6}  -> Mueve las 5 barras de onda en TalkPill en tiempo real
  * {"type": "speech_start"}          -> Cuando detecta que empezaste a hablar
  * {"type": "transcribing"}          -> Cuando haces una pausa de 0.8s y transcribe en RAM
  * {"type": "heard", "text": "..."}  -> Texto transcrito listo para Claudia
"""

import json
import os
import subprocess
import sys
import time
import wave
import numpy as np

RATE = 16000
CHUNK_SAMPLES = 1600  # 100 ms por bloque (igual que Ears.cs: Rate / 10)
CHUNK_BYTES = CHUNK_SAMPLES * 2

TTS_SPEAKING_FLAG = "/tmp/claudia-tts-speaking.flag"
MIC_MUTED_FLAG = "/tmp/claudia-mic-muted.flag"
FORCE_FLUSH_FLAG = "/tmp/claudia-mic-stop.flag"
DEBUG_WAV_PATH = "/tmp/claudia-mic-input.wav"

HALLUCINATION_PHRASES = {
    "subtítulos realizados por la comunidad de amara.org",
    "subtitulos realizados por la comunidad de amara.org",
    "subtítulos por la comunidad de amara.org",
    "gracias por ver el video",
    "gracias por ver",
    "suscríbete al canal",
    "¡suscríbete!",
    "subtítulos en español",
    "amara.org",
    "...",
    ".",
}


def emit(payload: dict):
    try:
        sys.stdout.write(json.dumps(payload, ensure_ascii=False) + "\n")
        sys.stdout.flush()
    except Exception:
        pass


def get_clean_env():
    env = os.environ.copy()
    env["XDG_RUNTIME_DIR"] = "/run/user/1000"
    env["PULSE_SERVER"] = "unix:/run/user/1000/pulse/native"
    for k in list(env.keys()):
        if k.startswith("SNAP"):
            env.pop(k, None)
    return env


def ensure_mic_ready() -> str:
    """Detecta el micrófono digital integrado del notebook (HiFi__Mic1__source) y calibra su ganancia."""
    env = get_clean_env()
    chosen = "@DEFAULT_SOURCE@"
    try:
        out = subprocess.check_output(
            ["pactl", "list", "sources", "short"], env=env, text=True, timeout=3
        )
        sources = []
        for line in out.splitlines():
            parts = line.split()
            if len(parts) >= 2:
                name = parts[1]
                if "monitor" not in name.lower():
                    sources.append(name)
        for s in sources:
            if "Mic1" in s:
                chosen = s
                break
        if chosen == "@DEFAULT_SOURCE@":
            for s in sources:
                if "Mic" in s:
                    chosen = s
                    break
        if chosen == "@DEFAULT_SOURCE@" and sources:
            chosen = sources[0]

        if chosen != "@DEFAULT_SOURCE@":
            subprocess.run(
                ["pactl", "set-default-source", chosen],
                env=env,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                timeout=2,
            )
            subprocess.run(
                ["pactl", "set-source-mute", chosen, "0"],
                env=env,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                timeout=2,
            )
            subprocess.run(
                ["pactl", "set-source-volume", chosen, "75%"],
                env=env,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                timeout=2,
            )

        subprocess.run(
            ["amixer", "-c", "0", "sset", "Dmic0", "54", "unmute"],
            env=env,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            timeout=2,
        )
    except Exception:
        pass
    return chosen


def voice_level(samples_i16: np.ndarray) -> float:
    """
    Filtro paso-bajo de Capri (Capri.cs VoiceLevel líneas 8637-8648):
    Atenúa clics de teclado y siseo de ventilador (~7 dB) manteniendo la fundamental de la voz (<900 Hz).
    """
    if samples_i16.size == 0:
        return 0.0
    x = samples_i16.astype(np.float32) / 32768.0
    kernel = np.array([0.40, 0.30, 0.20, 0.10], dtype=np.float32)
    filtered = np.convolve(x, kernel, mode="valid")
    return float(np.sqrt(np.mean(filtered * filtered)))


def compute_meter_level(level: float, wake: float) -> int:
    """
    Calcula el nivel 0..6 para las 5 barras de onda de TalkPill (Ears.cs + TalkPill.DrawMeter).
    En silencio devuelve 0; al hablar escala dinámicamente de 1 a 6.
    """
    if level < wake * 0.85:
        return 0
    norm = (level - wake * 0.80) / max(0.025, wake * 3.2)
    return max(1, min(6, int(round(norm * 5.0)) + 1))


def load_whisper_model():
    from faster_whisper import WhisperModel

    try:
        return WhisperModel(
            "base", device="cpu", compute_type="int8", local_files_only=True
        )
    except Exception:
        return WhisperModel("base", device="cpu", compute_type="int8")


def transcribe_utterance(model, chunks: list) -> str:
    if not chunks:
        return ""
    pcm_i16 = np.concatenate(chunks)
    try:
        with wave.open(DEBUG_WAV_PATH, "wb") as wf:
            wf.setnchannels(1)
            wf.setsampwidth(2)
            wf.setframerate(RATE)
            wf.writeframes(pcm_i16.tobytes())
    except Exception:
        pass

    audio_f32 = pcm_i16.astype(np.float32) / 32768.0
    peak = float(np.max(np.abs(audio_f32))) if audio_f32.size > 0 else 0.0
    if 0.018 < peak < 0.38:
        audio_f32 = np.clip(audio_f32 * (0.48 / peak), -1.0, 1.0)

    segments, _ = model.transcribe(
        audio_f32,
        language="es",
        beam_size=2,
        best_of=2,
        temperature=0.0,
        vad_filter=True,
        vad_parameters=dict(min_silence_duration_ms=350, speech_pad_ms=200),
        initial_prompt="Claudia, OpenClaw, Gemini, Argon, Duoc, VS Code, Chrome, terminal, volumen, brillo, abre, busca.",
        condition_on_previous_text=False,
    )
    text = " ".join(seg.text.strip() for seg in segments).strip()
    clean_lower = text.lower().strip(" .,¡!¿?-")
    if not clean_lower or clean_lower in HALLUCINATION_PHRASES:
        return ""
    return text


def run_daemon():
    for flag in (FORCE_FLUSH_FLAG, TTS_SPEAKING_FLAG):
        try:
            if os.path.exists(flag):
                os.remove(flag)
        except Exception:
            pass

    mic_source = ensure_mic_ready()
    model = load_whisper_model()
    emit({"type": "ready", "mic": mic_source})

    env = get_clean_env()
    cmd = [
        "parec",
        f"--device={mic_source}",
        "--raw",
        "--format=s16le",
        f"--rate={RATE}",
        "--channels=1",
        "--latency-msec=40",
    ]

    while True:
        proc = None
        try:
            proc = subprocess.Popen(
                cmd,
                stdout=subprocess.PIPE,
                stderr=subprocess.DEVNULL,
                env=env,
                bufsize=CHUNK_BYTES * 2,
            )

            chunk_idx = 0
            calib_samples = []
            room = 0.010
            preroll = []
            utterance = None
            loud_run = 0
            quiet_run = 0
            last_emitted_level = 0
            last_level_time = 0.0
            emit({"type": "level", "level": 0})

            while True:
                raw = proc.stdout.read(CHUNK_BYTES)
                if not raw or len(raw) < CHUNK_BYTES:
                    break

                chunk_idx += 1
                # Ignorar los primeros 3 bloques (300ms) de encendido del DMIC Intel Skylake
                if chunk_idx <= 3:
                    continue

                samples = np.frombuffer(raw, dtype=np.int16)
                level = voice_level(samples)

                # Calibrar piso base en los bloques 4..6 (300ms estables)
                if chunk_idx <= 6:
                    calib_samples.append(level)
                    if chunk_idx == 6:
                        room = max(0.007, min(float(np.mean(calib_samples)), 0.020))
                    continue

                now = time.monotonic()

                # Si el micrófono está silenciado (MUTED en TalkPill) o Claudia está hablando por TTS:
                if os.path.exists(MIC_MUTED_FLAG) or os.path.exists(TTS_SPEAKING_FLAG):
                    if utterance is not None or loud_run > 0:
                        utterance = None
                        preroll.clear()
                        loud_run = 0
                        quiet_run = 0
                    if last_emitted_level != 0:
                        last_emitted_level = 0
                        emit({"type": "level", "level": 0})
                    continue

                # Seguimiento EMA suave del piso de ruido ambiente cuando no hay frase activa
                if utterance is None:
                    if level < room:
                        room = 0.85 * room + 0.15 * max(level, 0.006)
                    elif level < room * 1.9:
                        room = min(0.022, 0.92 * room + 0.08 * level)

                # Umbral adaptativo de voz: ~2.35x sobre el ruido ambiente promedio (mínimo 0.022)
                wake = max(0.022, room * 2.35)
                loud = level >= wake

                bar_level = compute_meter_level(level, wake) if (loud or utterance is not None) else 0
                if bar_level != last_emitted_level and (
                    bar_level == 0
                    or bar_level > last_emitted_level
                    or (now - last_level_time) >= 0.07
                ):
                    last_emitted_level = bar_level
                    last_level_time = now
                    emit({"type": "level", "level": bar_level})

                force_flush = os.path.exists(FORCE_FLUSH_FLAG)
                if force_flush:
                    try:
                        os.remove(FORCE_FLUSH_FLAG)
                    except Exception:
                        pass

                if utterance is None:
                    preroll.append(samples)
                    if len(preroll) > 4:
                        preroll.pop(0)
                    loud_run = (loud_run + 1) if loud else 0
                    if loud_run >= 2:
                        utterance = list(preroll)
                        preroll.clear()
                        loud_run = 0
                        quiet_run = 0
                        emit({"type": "speech_start"})
                    continue

                # Capturando frase activa
                utterance.append(samples)
                quiet_run = 0 if loud else (quiet_run + 1)

                # Descartar ruido estacionario (ventilador/hum constante de >2s sin modulación vocal)
                if len(utterance) >= 20 and quiet_run == 0:
                    recent_levels = [voice_level(c) for c in utterance[-20:]]
                    low_l = min(recent_levels)
                    high_l = max(recent_levels)
                    if high_l < low_l * 1.22:
                        room = max(room, min(low_l, 0.025))
                        utterance = None
                        quiet_run = 0
                        last_emitted_level = 0
                        emit({"type": "level", "level": 0})
                        continue

                # Fin de frase tras 0.8s (8 bloques) de silencio, o force_flush, o tope de 13s
                if quiet_run >= 8 or force_flush or len(utterance) >= 130:
                    taken = utterance
                    utterance = None
                    quiet_run = 0
                    last_emitted_level = 0
                    emit({"type": "level", "level": 0})

                    voiced_chunks = sum(1 for c in taken if voice_level(c) >= wake * 0.9)
                    if voiced_chunks < 2 and not force_flush:
                        continue

                    emit({"type": "transcribing"})
                    try:
                        text = transcribe_utterance(model, taken)
                    except Exception as e:
                        emit({"type": "error", "error": str(e)})
                        continue

                    if text:
                        emit({"type": "heard", "text": text})
                    else:
                        emit({"type": "missed"})

        except Exception as e:
            emit({"type": "error", "error": str(e)})
        finally:
            if proc is not None:
                try:
                    proc.terminate()
                    proc.wait(timeout=1)
                except Exception:
                    pass
        time.sleep(1.0)


if __name__ == "__main__":
    run_daemon()
