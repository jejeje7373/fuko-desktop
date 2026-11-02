#!/usr/bin/env python3
"""
scripts/ask_gemini_chrome.py
Automatiza Google Chrome en el escritorio para consultar a Gemini en la web:
1. Abre o enfoca Google Chrome en https://gemini.google.com
2. Si el usuario especificó un modelo (ej: 4 Argon, 3.1 Pro, 3.6 Flash), abre el selector y lo elige.
3. Escribe la pregunta del usuario en el cuadro de texto y la envía con Enter.
"""

import os
import sys
import time
import subprocess

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
CLAUDIA_HID = os.path.join(SCRIPT_DIR, "claudia_hid.py")

def get_clean_env():
    env = os.environ.copy()
    env.setdefault("DISPLAY", ":0")
    env.setdefault("XDG_RUNTIME_DIR", "/run/user/1000")
    env.setdefault("WAYLAND_DISPLAY", "wayland-0")
    env.setdefault("DBUS_SESSION_BUS_ADDRESS", "unix:path=/run/user/1000/bus")
    run_user = "/run/user/1000"
    if os.path.isdir(run_user):
        for fname in os.listdir(run_user):
            if fname.startswith(".mutter-Xwaylandauth.") or "Xauthority" in fname:
                env["XAUTHORITY"] = os.path.join(run_user, fname)
                break
    return env

def run_hid(action, arg=None):
    cmd = ["python3", CLAUDIA_HID, action]
    if arg:
        cmd.append(arg)
    subprocess.run(cmd, env=get_clean_env(), capture_output=True)

def ask_gemini_in_chrome(target_model: str, question: str):
    env = get_clean_env()
    print(f"[Gemini Chrome] Abriendo https://gemini.google.com en Google Chrome...")

    # 1. Abrir Google Chrome en gemini.google.com
    url = "https://gemini.google.com"
    try:
        subprocess.Popen(["google-chrome", url], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    except Exception as e:
        print(f"[Gemini Chrome] Error abriendo google-chrome: {e}, intentando xdg-open...")
        subprocess.Popen(["xdg-open", url], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

    # 2. Esperar a que la página cargue y tome el foco
    time.sleep(2.8)

    # 3. Selección del modelo si el usuario lo especificó
    model_lower = (target_model or "").lower().strip()
    down_arrows = 0

    if "4" in model_lower or "argon" in model_lower:
        down_arrows = 3  # 4 Argon Tareas complejas
        selected_label = "Gemini 4 Argon"
    elif "pro" in model_lower or "3.1" in model_lower:
        down_arrows = 2  # 3.1 Pro Razonamiento avanzado
        selected_label = "Gemini 3.1 Pro"
    elif "flash" in model_lower or "3.6" in model_lower:
        down_arrows = 1  # 3.6 Flash Ayuda completa
        selected_label = "Gemini 3.6 Flash"
    elif "lite" in model_lower or "3.5" in model_lower:
        down_arrows = 0  # Flash-Lite (por defecto)
        selected_label = "Gemini 3.5 Flash-Lite"
    else:
        selected_label = None

    if selected_label and down_arrows > 0:
        print(f"[Gemini Chrome] Configurando modelo en la página: {selected_label}...")
        # Desde el cuadro de texto, Tab va al botón 'Abrir selector de modo'
        run_hid("key", "Tab")
        time.sleep(0.18)
        # Enter abre el menú desplegable de modelos
        run_hid("key", "Return")
        time.sleep(0.35)

        # Flecha abajo N veces para llegar al modelo deseado
        for _ in range(down_arrows):
            run_hid("key", "Down")
            time.sleep(0.12)

        # Enter para confirmar la selección del modelo
        run_hid("key", "Return")
        time.sleep(0.30)

        # Shift+Tab para volver inmediatamente al cuadro de texto de la pregunta
        run_hid("key", "shift+Tab")
        time.sleep(0.20)
    else:
        print("[Gemini Chrome] Usando modelo actual en la página de Gemini.")

    # 4. Escribir y enviar la pregunta
    clean_question = question.strip()
    if clean_question:
        print(f"[Gemini Chrome] Escribiendo pregunta: '{clean_question}'...")
        # Escribir con salto de línea / Enter para enviar
        run_hid("type_enter", clean_question)
        print("[Gemini Chrome] ¡Pregunta enviada exitosamente!")
    else:
        print("[Gemini Chrome] Página lista con el modelo configurado.")

def main():
    if len(sys.argv) < 2:
        print("Uso: ask_gemini_chrome.py [modelo] <pregunta>")
        sys.exit(1)

    if len(sys.argv) == 2:
        model = ""
        question = sys.argv[1]
    else:
        model = sys.argv[1]
        question = sys.argv[2]

    ask_gemini_in_chrome(model, question)

if __name__ == "__main__":
    main()
