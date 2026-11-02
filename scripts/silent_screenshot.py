#!/usr/bin/env python3
import sys
import os

def take_silent_screenshot(target_path):
    try:
        import dbus
        bus = dbus.SessionBus()
        # Reclamar el nombre permitido por GNOME Shell en la lista blanca
        bus.request_name("org.gnome.Screenshot", dbus.bus.NAME_FLAG_REPLACE_EXISTING)
        
        obj = bus.get_object("org.gnome.Shell.Screenshot", "/org/gnome/Shell/Screenshot")
        iface = dbus.Interface(obj, "org.gnome.Shell.Screenshot")
        
        # Screenshot(include_cursor=False, flash=False, filename=target_path)
        # flash=False asegura que sea 100% en segundo plano sin destello visual ni sonido
        res = iface.Screenshot(False, False, target_path)
        if res and os.path.exists(target_path):
            return True
    except Exception as e:
        sys.stderr.write(f"[SilentScreenshot] DBus error: {e}\n")
    return False

if __name__ == "__main__":
    out = sys.argv[1] if len(sys.argv) > 1 else "/tmp/claudia_notebook_screen.png"
    ok = take_silent_screenshot(out)
    sys.exit(0 if ok else 1)
