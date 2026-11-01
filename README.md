# 🐱 Fuko Desktop Companion

> **Compañera de escritorio autónoma para Linux (Ubuntu GNOME Wayland & X11) inspirada en [Capri Desktop](https://github.com/lachydotmcg/capri-desktop.git), conectada con [OpenClaw](https://openclaw.ai) y Gemini 4 Argon.**

Fuko no es un simple chatbot de ventana: es un constructo digital que vive en tu escritorio, camina, cae con gravedad, se posa en las esquinas de tus aplicaciones abiertas, te escucha por micrófono en tiempo real y puede interactuar directamente con tu sistema.

---

## ⚡ Características Principales

- **Físicas y Animaciones de Escritorio Estilo Capri:**
  - Sprite sheets PNG animados (240x384 y 384x384): caminar, posarse (perch), bailar con música (`playerctl`), dormir, estirarse y caricias en la cabeza.
  - Gravedad y detección de ventanas en GNOME Wayland a través de su extensión DBus (`claudia-perch@openclaw.ai`).
- **Oídos en Tiempo Real (`TalkPill` + `faster-whisper`):**
  - Pastilla interactiva `TYPE` y `TALK` con barras de onda que reaccionan a tu voz.
  - Modelo `faster-whisper` precargado en memoria RAM para transcripción instantánea (<800ms) sin pausas ni llamadas a APIs lentas.
- **Voz Propia (Piper TTS):**
  - Síntesis de voz neural local (`daniela-high`) en español sin necesidad de conexión a internet.
- **Inteligencia Híbrida Adaptativa:**
  - **OpenClaw (`nemotron` / `qwen`):** Control del sistema, conversación ágil, apertura de programas, búsqueda web y visión.
  - **Gemini 4 Argon:** Reservado automáticamente para programación compleja, arquitectura de software y análisis de ciberseguridad / vulnerabilidades.
  - **44+ Skills integradas:** Herramientas de OpenClaw y Antigravity descubiertas dinámicamente.
- **Control Directo del Notebook (Hands & HID):**
  - Capacidad de escribir texto directamente con simulación de teclado en Wayland/X11 (`Mutter RemoteDesktop`).
  - Control de brillo (`brightnessctl`), volumen (`amixer`/`wpctl`), apertura de programas (`VS Code`, `Chrome`, `Terminal`).
- **Personalidad y Alma Editable (`SOUL.md` & `IDENTITY.md`):**
  - Editor en vivo en el Mainframe Hub con guardado inmediato (`Ctrl+S`).
  - Puedes pedirle por voz o texto: *"Fuko, recuerda que..."* o *"Fuko, agrega a tu soul.md que..."*.

---

## 🛠️ Requisitos del Sistema

- **Sistema Operativo:** Linux (Probado y optimizado en Ubuntu 24.04 LTS con GNOME Wayland / X11).
- **Lenguajes y Herramientas:**
  - Rust (cargo 1.75+)
  - Node.js (v18+)
  - Python 3.10+
  - WebKitGTK 4.1
  - `gstreamer1.0`, `playerctl`, `brightnessctl`, `pulseaudio-utils` (`parec`)

---

## 🚀 Instalación y Compilación

1. **Clonar el repositorio:**
   ```bash
   git clone https://github.com/TU_USUARIO/fuko-desktop.git
   cd fuko-desktop
   ```

2. **Instalar dependencias de frontend y scripts:**
   ```bash
   npm install
   pip install faster-whisper sounddevice numpy
   ```

3. **Compilar el binario de Tauri:**
   ```bash
   cargo build --manifest-path src-tauri/Cargo.toml --release
   ```

4. **Ejecutar a Fuko:**
   ```bash
   ./start-claudia.sh
   ```

---

## 📂 Estructura del Proyecto

```text
fuko-desktop/
├── public/                 # Frontend Web (Canvas 2D, sprites y Mainframe UI)
│   ├── index.html          # Escenario de Fuko y TalkPill interactivo
│   ├── mainframe.html      # Hub de configuración de SOUL.md y OpenClaw
│   └── sprites/            # Sprite sheets de animación
├── src/                    # Backend Rust (Tauri 2.0)
│   ├── avatar.rs           # Máquina de estados de animación
│   ├── context_watcher.rs  # Vigilancia de ventanas activas, música y batería
│   ├── notebook_hands.rs   # Control nativo del notebook (apps, volumen, brillo)
│   ├── openclaw_bridge.rs  # Puente entre OpenClaw, Gemini 4 Argon y Vision
│   ├── overlay_window.rs   # Ventana transparente always-on-top
│   ├── skills_hub.rs       # Gestor de 44+ skills de OpenClaw y Antigravity
│   └── voice_ears.rs       # Integración de audio y TTS
├── scripts/                # Daemons en Python (faster-whisper, HID injection)
└── extension/              # Extensión GNOME Shell para detección de bordes en Wayland
```

---

## 📄 Licencia

Este proyecto está bajo la Licencia MIT.
Sprites y concepto base inspirados en [Capri Desktop](https://github.com/lachydotmcg/capri-desktop.git) por `@lachydotmcg`.
