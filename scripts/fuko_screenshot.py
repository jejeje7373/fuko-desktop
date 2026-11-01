#!/usr/bin/env python3
"""
fuko_screenshot.py - Captura de pantalla para Fuko en GNOME Wayland usando la vía oficial:
el portal org.freedesktop.portal.Screenshot (xdg-desktop-portal).

GNOME bloquea org.gnome.Shell.Screenshot para apps externas y grim/import no funcionan en
GNOME Wayland, así que el portal es el único camino soportado. La primera vez GNOME puede
pedir permiso al usuario; luego lo recuerda.

Uso: fuko_screenshot.py [ruta_destino.png]
Salida: imprime la ruta final y termina con código 0, o termina con código != 0 si falla.
"""
import os
import random
import shutil
import string
import sys
from urllib.parse import unquote, urlparse

import gi

gi.require_version("Gio", "2.0")
from gi.repository import Gio, GLib  # noqa: E402

TARGET = sys.argv[1] if len(sys.argv) > 1 else "/tmp/fuko_screen.png"
TIMEOUT_S = 25


def main() -> int:
    bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
    token = "fuko" + "".join(random.choices(string.ascii_lowercase + string.digits, k=10))
    sender = bus.get_unique_name()[1:].replace(".", "_")
    request_path = f"/org/freedesktop/portal/desktop/request/{sender}/{token}"

    loop = GLib.MainLoop()
    result = {"code": None, "uri": None}

    def on_response(_conn, _sender, _path, _iface, _signal, params):
        code, results = params.unpack()
        result["code"] = code
        result["uri"] = results.get("uri")
        loop.quit()

    # Suscribirse ANTES de llamar para no perder la respuesta
    bus.signal_subscribe(
        "org.freedesktop.portal.Desktop",
        "org.freedesktop.portal.Request",
        "Response",
        request_path,
        None,
        Gio.DBusSignalFlags.NO_MATCH_RULE,
        on_response,
    )

    options = {
        "handle_token": GLib.Variant("s", token),
        "interactive": GLib.Variant("b", False),
    }
    try:
        bus.call_sync(
            "org.freedesktop.portal.Desktop",
            "/org/freedesktop/portal/desktop",
            "org.freedesktop.portal.Screenshot",
            "Screenshot",
            GLib.Variant("(sa{sv})", ("", options)),
            None,
            Gio.DBusCallFlags.NONE,
            -1,
            None,
        )
    except GLib.Error as err:
        print(f"portal error: {err.message}", file=sys.stderr)
        return 3

    def on_timeout():
        loop.quit()
        return False

    GLib.timeout_add_seconds(TIMEOUT_S, on_timeout)
    loop.run()

    if result["code"] != 0 or not result["uri"]:
        print(f"portal response code={result['code']}", file=sys.stderr)
        return 2 if result["code"] is not None else 4

    source = unquote(urlparse(result["uri"]).path)
    if not os.path.exists(source):
        print(f"captura no encontrada: {source}", file=sys.stderr)
        return 5

    # Mover la captura a /tmp para no llenar ~/Imágenes/Capturas de pantalla
    os.makedirs(os.path.dirname(TARGET) or ".", exist_ok=True)
    shutil.move(source, TARGET)
    print(TARGET)
    return 0


if __name__ == "__main__":
    sys.exit(main())
