#!/usr/bin/env bash
# Lanzador de Claudia Desktop Companion (Capri Desktop Engine + OpenClaw + Gemini 4 Argon)
set -e

# Cerrar FDs heredados > 2 por seguridad si se lanza desde un IDE Snap
for fd in $(ls /proc/$$/fd 2>/dev/null); do
  if [ "$fd" -gt 2 ]; then
    eval "exec ${fd}>&-" 2>/dev/null || true
  fi
done

unset GTK_MODULES SNAP SNAP_NAME SNAP_REVISION SNAP_LIBRARY_PATH SNAP_CONTEXT SNAP_COOKIE
export WEBKIT_DISABLE_DMABUF_RENDERER=1
export GDK_BACKEND=x11
export DISPLAY="${DISPLAY:-:0}"

if [ -z "$XAUTHORITY" ] || [ ! -f "$XAUTHORITY" ]; then
  XAUTH_CANDIDATE=$(ls /run/user/$(id -u)/.mutter-Xwaylandauth.* 2>/dev/null | head -n1 || true)
  if [ -n "$XAUTH_CANDIDATE" ]; then
    export XAUTHORITY="$XAUTH_CANDIDATE"
  fi
fi

exec "/home/jejej/.openclaw/workspace/claudia-desktop/src-tauri/target/debug/claudia-desktop" "$@"
