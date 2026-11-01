#!/usr/bin/env python3
"""
claudia_hid.py - Controlador nativo de Teclado y Mouse en GNOME Wayland + X11 para Claudia Desktop.
Usa org.gnome.Mutter.RemoteDesktop.Session (D-Bus nativo del compositor Mutter en Wayland)
y libX11/libXtst como respaldo, junto con wl-copy / xclip.

Permite a Claudia:
1. Escribir texto o código directamente en cualquier ventana (VS Code, Chrome, Terminal, IntelliJ, formularios web).
2. Cambiar automáticamente el foco a la ventana anterior (Alt+Tab) si la ventana superpuesta de Claudia tenía el foco.
3. Presionar teclas y combinaciones (Tab, Return/Enter, Escape, BackSpace, ctrl+v, ctrl+s, ctrl+l, alt+Tab).
4. Rellenar páginas de inicio de sesión (usuario -> Tab -> contraseña -> Enter).
5. Mover el puntero y hacer clic.
"""

import os
import subprocess
import sys
import time
from gi.repository import Gio, GLib

# Keysyms estándar X11 / Wayland (XKB)
KEYSYMS = {
    "return": 0xFF0D,
    "enter": 0xFF0D,
    "intro": 0xFF0D,
    "tab": 0xFF09,
    "escape": 0xFF1B,
    "esc": 0xFF1B,
    "backspace": 0xFF08,
    "borrar": 0xFF08,
    "delete": 0xFFFF,
    "supr": 0xFFFF,
    "space": 0x0020,
    "espacio": 0x0020,
    "up": 0xFF52,
    "down": 0xFF54,
    "left": 0xFF51,
    "right": 0xFF53,
    "home": 0xFF50,
    "end": 0xFF57,
    "pageup": 0xFF55,
    "pagedown": 0xFF56,
    "ctrl": 0xFFE3,       # Control_L
    "control": 0xFFE3,
    "shift": 0xFFE1,      # Shift_L
    "alt": 0xFFE9,        # Alt_L
    "super": 0xFFEB,      # Super_L
    "win": 0xFFEB,
    "meta": 0xFFEB,
    "f1": 0xFFBE,
    "f2": 0xFFBF,
    "f3": 0xFFC0,
    "f4": 0xFFC1,
    "f5": 0xFFC2,
    "f6": 0xFFC3,
    "f7": 0xFFC4,
    "f8": 0xFFC5,
    "f9": 0xFFC6,
    "f10": 0xFFC7,
    "f11": 0xFFC8,
    "f12": 0xFFC9,
}


def get_clean_env():
    env = os.environ.copy()
    env.setdefault("DISPLAY", ":0")
    env.setdefault("XDG_RUNTIME_DIR", "/run/user/1000")
    env.setdefault("WAYLAND_DISPLAY", "wayland-0")
    env.setdefault("DBUS_SESSION_BUS_ADDRESS", "unix:path=/run/user/1000/bus")
    for k in list(env.keys()):
        if k.startswith("SNAP"):
            env.pop(k, None)
    if "XAUTHORITY" not in env or not os.path.exists(env.get("XAUTHORITY", "")):
        run_user = "/run/user/1000"
        if os.path.isdir(run_user):
            for fname in os.listdir(run_user):
                if fname.startswith(".mutter-Xwaylandauth.") or "Xauthority" in fname:
                    env["XAUTHORITY"] = os.path.join(run_user, fname)
                    break
    return env


def is_claudia_overlay_focused(env) -> bool:
    """
    Detecta si la ventana XWayland de Claudia ('Claudia' overlay) tiene actualmente el foco
    (por ejemplo, cuando el usuario escribió en la barra > tú: en lugar de hablar por micrófono).
    """
    try:
        out = subprocess.check_output(
            ["xprop", "-root", "_NET_ACTIVE_WINDOW"],
            env=env,
            text=True,
            stderr=subprocess.DEVNULL,
            timeout=1.5,
        )
        if "0x0" in out or "#" not in out:
            return False
        win_id = out.split("#")[-1].strip().split(",")[0].strip()
        if not win_id or win_id == "0x0":
            return False
        wname = subprocess.check_output(
            ["xprop", "-id", win_id, "WM_NAME", "WM_CLASS"],
            env=env,
            text=True,
            stderr=subprocess.DEVNULL,
            timeout=1.5,
        )
        return "claudia" in wname.lower() and "mainframe" not in wname.lower()
    except Exception:
        return False


class MutterHID:
    """Sesión de control de teclado y mouse a nivel del compositor GNOME Mutter (Wayland + XWayland)."""

    def __init__(self):
        self.env = get_clean_env()
        for k, v in self.env.items():
            os.environ[k] = v
        self.bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
        res = self.bus.call_sync(
            "org.gnome.Mutter.RemoteDesktop",
            "/org/gnome/Mutter/RemoteDesktop",
            "org.gnome.Mutter.RemoteDesktop",
            "CreateSession",
            None,
            GLib.VariantType.new("(o)"),
            Gio.DBusCallFlags.NONE,
            2500,
            None,
        )
        self.session_path = res.unpack()[0]
        self.bus.call_sync(
            "org.gnome.Mutter.RemoteDesktop",
            self.session_path,
            "org.gnome.Mutter.RemoteDesktop.Session",
            "Start",
            None,
            None,
            Gio.DBusCallFlags.NONE,
            2500,
            None,
        )

    def close(self):
        if self.session_path:
            try:
                self.bus.call_sync(
                    "org.gnome.Mutter.RemoteDesktop",
                    self.session_path,
                    "org.gnome.Mutter.RemoteDesktop.Session",
                    "Stop",
                    None,
                    None,
                    Gio.DBusCallFlags.NONE,
                    1500,
                    None,
                )
            except Exception:
                pass
            self.session_path = None

    def send_keysym(self, keysym: int, pressed: bool):
        self.bus.call_sync(
            "org.gnome.Mutter.RemoteDesktop",
            self.session_path,
            "org.gnome.Mutter.RemoteDesktop.Session",
            "NotifyKeyboardKeysym",
            GLib.Variant("(ub)", (int(keysym), bool(pressed))),
            None,
            Gio.DBusCallFlags.NONE,
            1500,
            None,
        )

    def tap_keysym(self, keysym: int, hold_sec: float = 0.014):
        self.send_keysym(keysym, True)
        time.sleep(hold_sec)
        self.send_keysym(keysym, False)
        time.sleep(hold_sec)

    def char_to_keysym(self, ch: str) -> int:
        if ch == "\n":
            return KEYSYMS["return"]
        if ch == "\t":
            return KEYSYMS["tab"]
        code = ord(ch)
        if 0x20 <= code <= 0x7E:
            return code
        if 0xA0 <= code <= 0xFF:
            return code
        # Convención estándar X11/XKB para cualquier punto Unicode > U+0100
        return 0x01000000 | code

    def press_combo(self, combo_str: str):
        parts = [p.strip() for p in combo_str.replace("-", "+").split("+") if p.strip()]
        if not parts:
            return
        syms = []
        for p in parts:
            lower = p.lower()
            if lower in KEYSYMS:
                syms.append(KEYSYMS[lower])
            elif len(p) == 1:
                syms.append(self.char_to_keysym(p.lower()))
        if not syms:
            return
        for s in syms:
            self.send_keysym(s, True)
            time.sleep(0.015)
        time.sleep(0.03)
        for s in reversed(syms):
            self.send_keysym(s, False)
            time.sleep(0.015)
        time.sleep(0.03)

    def ensure_user_window_focused(self):
        """Si la ventana overlay de Claudia tenía el foco, hace Alt+Tab hacia la ventana del usuario."""
        if is_claudia_overlay_focused(self.env):
            self.press_combo("alt+tab")
            time.sleep(0.28)

    def set_clipboard(self, text: str):
        try:
            p = subprocess.Popen(
                ["wl-copy"],
                stdin=subprocess.PIPE,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                env=self.env,
            )
            p.communicate(input=text.encode("utf-8"), timeout=2)
        except Exception:
            pass
        try:
            p2 = subprocess.Popen(
                ["xclip", "-selection", "clipboard"],
                stdin=subprocess.PIPE,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                env=self.env,
            )
            p2.communicate(input=text.encode("utf-8"), timeout=2)
        except Exception:
            pass

    def paste_text(self, text: str):
        self.set_clipboard(text)
        time.sleep(0.07)
        self.press_combo("ctrl+v")
        time.sleep(0.07)

    def type_text(self, text: str, enter_after: bool = False):
        self.ensure_user_window_focused()
        # Si es texto largo, código multilínea o con símbolos de login/programación (@, _, llaves, tildes),
        # pegar por portapapeles (wl-copy + xclip + Ctrl+V) es instantáneo y evita errores de layout de teclado.
        if (
            len(text) > 18
            or "\n" in text
            or any(ord(c) > 127 or c in "@#$%&{}[]\\|<>~^`_;:/?=" for c in text)
        ):
            self.paste_text(text)
        else:
            for ch in text:
                ks = self.char_to_keysym(ch)
                self.tap_keysym(ks, hold_sec=0.012)
        if enter_after:
            time.sleep(0.06)
            self.tap_keysym(KEYSYMS["return"], hold_sec=0.02)

    def fill_login(self, username: str, password: str, submit: bool = True):
        self.ensure_user_window_focused()
        if username:
            self.paste_text(username)
            time.sleep(0.18)
        if password:
            self.tap_keysym(KEYSYMS["tab"], hold_sec=0.025)
            time.sleep(0.20)
            self.paste_text(password)
            time.sleep(0.18)
        if submit:
            self.tap_keysym(KEYSYMS["return"], hold_sec=0.025)


def main():
    if len(sys.argv) < 2:
        print("Uso: claudia_hid.py <type|type_enter|key|login> [args...]")
        sys.exit(1)

    cmd = sys.argv[1].lower()
    hid = MutterHID()
    try:
        if cmd == "type":
            text = sys.argv[2] if len(sys.argv) > 2 else ""
            time.sleep(0.20)
            hid.type_text(text, enter_after=False)
            print("OK: texto escrito en tu ventana activa")
        elif cmd == "type_enter":
            text = sys.argv[2] if len(sys.argv) > 2 else ""
            time.sleep(0.20)
            hid.type_text(text, enter_after=True)
            print("OK: texto escrito + Enter")
        elif cmd == "key":
            combo = sys.argv[2] if len(sys.argv) > 2 else "Return"
            time.sleep(0.15)
            hid.ensure_user_window_focused()
            for part in combo.split(","):
                hid.press_combo(part.strip())
                time.sleep(0.08)
            print(f"OK: teclas presionadas ({combo})")
        elif cmd == "login":
            user = sys.argv[2] if len(sys.argv) > 2 else ""
            pw = sys.argv[3] if len(sys.argv) > 3 else ""
            submit = (sys.argv[4].lower() != "false") if len(sys.argv) > 4 else True
            time.sleep(0.30)
            hid.fill_login(user, pw, submit)
            print("OK: formulario de inicio de sesión completado")
    finally:
        hid.close()


if __name__ == "__main__":
    main()
