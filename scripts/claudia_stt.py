#!/usr/bin/env python3
"""
claudia_stt.py - Captura de micrófono en tiempo real con detección automática de fin de frase (VAD)
y transcripción local ultrarrápida en español usando faster-whisper.
"""

import json
import math
import os
import signal
import struct
import subprocess
import sys
import time
import wave

STOP_REQUESTED = False
STOP_FLAG_PATH = "/tmp/claudia-mic-stop.flag"
WAV_OUT_PATH = "/tmp/claudia-mic-input.wav"


def handle_sig(signum, frame):
    global STOP_REQUESTED
    STOP_REQUESTED = True


signal.signal(signal.SIGINT, handle_sig)
signal.signal(signal.SIGTERM, handle_sig)


def ensure_mic_ready(env):
    try:
        subprocess.run(
            ["pactl", "set-source-mute", "@DEFAULT_SOURCE@", "0"],
            env=env,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            timeout=2,
        )
        subprocess.run(
            [
                "pactl",
                "set-source-volume",
                "alsa_input.pci-0000_00_1f.3-platform-skl_hda_dsp_generic.HiFi__Mic1__source",
                "100%",
            ],
            env=env,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            timeout=2,
        )
    except Exception:
        pass


def compute_rms(pcm_bytes: bytes) -> float:
    count = len(pcm_bytes) // 2
    if count == 0:
        return 0.0
    shorts = struct.unpack(f"<{count}h", pcm_bytes[: count * 2])
    sum_sq = sum(s * s for s in shorts)
    return math.sqrt(sum_sq / count)


def record_utterance(max_seconds: float = 12.0) -> bool:
    global STOP_REQUESTED
    if os.path.exists(STOP_FLAG_PATH):
        try:
            os.remove(STOP_FLAG_PATH)
        except Exception:
            pass

    env = os.environ.copy()
    env["XDG_RUNTIME_DIR"] = "/run/user/1000"
    env["PULSE_SERVER"] = "unix:/run/user/1000/pulse/native"
    for k in list(env.keys()):
        if k.startswith("SNAP"):
            env.pop(k, None)

    ensure_mic_ready(env)

    proc = subprocess.Popen(
        ["parec", "--raw", "--format=s16le", "--rate=16000", "--channels=1"],
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        env=env,
    )

    sample_rate = 16000
    chunk_samples = 1600  # 100ms por bloque
    chunk_bytes = chunk_samples * 2

    frames = []
    speech_started = False
    silence_chunks = 0
    total_chunks = 0
    max_chunks = int(max_seconds * 10)
    wait_start_chunks = 60  # hasta 6s esperando que empiece a hablar

    # Calibración rápida del ruido ambiente en los primeros 200ms
    noise_floor = 180.0
    calib_vals = []

    try:
        while not STOP_REQUESTED and total_chunks < max_chunks:
            if os.path.exists(STOP_FLAG_PATH):
                try:
                    os.remove(STOP_FLAG_PATH)
                except Exception:
                    pass
                break

            data = proc.stdout.read(chunk_bytes)
            if not data:
                break

            frames.append(data)
            total_chunks += 1
            rms = compute_rms(data)

            if total_chunks <= 2:
                calib_vals.append(rms)
                if total_chunks == 2:
                    avg_noise = sum(calib_vals) / len(calib_vals)
                    noise_floor = max(160.0, min(avg_noise * 1.8, 900.0))
                continue

            if not speech_started:
                if rms > noise_floor:
                    speech_started = True
                    silence_chunks = 0
                elif total_chunks > wait_start_chunks:
                    # No habló en los primeros 6 segundos
                    break
            else:
                if rms < noise_floor * 0.85:
                    silence_chunks += 1
                    # 1.2 segundos de silencio tras haber hablado -> fin de frase
                    if silence_chunks >= 12 and total_chunks > 10:
                        break
                else:
                    silence_chunks = 0
    finally:
        try:
            proc.terminate()
            proc.wait(timeout=1)
        except Exception:
            pass

    if not frames or len(frames) < 4:
        return False

    with wave.open(WAV_OUT_PATH, "wb") as wf:
        wf.setnchannels(1)
        wf.setsampwidth(2)
        wf.setframerate(sample_rate)
        wf.writeframes(b"".join(frames))

    return True


def transcribe_wav() -> str:
    from faster_whisper import WhisperModel

    try:
        model = WhisperModel("base", device="cpu", compute_type="int8", local_files_only=True)
    except Exception:
        model = WhisperModel("base", device="cpu", compute_type="int8")

    segments, _ = model.transcribe(
        WAV_OUT_PATH,
        language="es",
        beam_size=2,
        vad_filter=True,
        initial_prompt="Claudia, OpenClaw, Gemini, Argon, Duoc, VS Code, Chrome, terminal, volumen, brillo, abre, busca.",
    )
    text = " ".join(s.text.strip() for s in segments).strip()
    return text


def main():
    try:
        ok = record_utterance(max_seconds=13.0)
        if not ok or not os.path.exists(WAV_OUT_PATH):
            print(json.dumps({"ok": False, "error": "No se detectó audio del micrófono."}))
            return

        text = transcribe_wav()
        # Filtrar alucinaciones típicas de Whisper en silencio
        hallucinations = {
            "subtítulos realizados por la comunidad de amara.org",
            "subtitulos realizados por la comunidad de amara.org",
            "gracias por ver el video",
            "gracias.",
            "...",
        }
        if not text or text.lower().strip() in hallucinations:
            print(json.dumps({"ok": False, "error": "No te escuché claro choom, intenta hablar un poquito más fuerte."}))
            return

        print(json.dumps({"ok": True, "text": text}, ensure_ascii=False))
    except Exception as e:
        print(json.dumps({"ok": False, "error": str(e)}, ensure_ascii=False))


if __name__ == "__main__":
    main()
