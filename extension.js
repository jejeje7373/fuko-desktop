import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Meta from 'gi://Meta';
import Shell from 'gi://Shell';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';

const IFACE_XML = `
<node>
  <interface name="org.openclaw.ClaudiaPerch">
    <method name="GetLedges">
      <arg type="s" direction="out" name="json"/>
    </method>
    <method name="TakeScreenshot">
      <arg type="s" direction="in" name="targetPath"/>
      <arg type="b" direction="out" name="ok"/>
    </method>
  </interface>
</node>`;

export default class ClaudiaPerchExtension extends Extension {
    enable() {
        this._dbus = Gio.DBusExportedObject.wrapJSObject(IFACE_XML, this);
        this._dbus.export(Gio.DBus.session, '/org/openclaw/ClaudiaPerch');
        this._ownerId = Gio.DBus.session.own_name(
            'org.openclaw.ClaudiaPerch',
            Gio.BusNameOwnerFlags.REPLACE,
            null,
            null
        );
        console.log('[ClaudiaPerch] Extensión activada en DBus: org.openclaw.ClaudiaPerch');
    }

    disable() {
        if (this._ownerId) {
            Gio.DBus.session.unown_name(this._ownerId);
            this._ownerId = null;
        }
        if (this._dbus) {
            this._dbus.unexport();
            this._dbus = null;
        }
        console.log('[ClaudiaPerch] Extensión desactivada');
    }

    GetLedges() {
        try {
            const display = global.display;
            const workspace = display.get_workspace_manager().get_active_workspace();
            const windows = display.get_tab_list(Meta.TabList.NORMAL_ALL, workspace);
            const focusWin = display.get_focus_window();

            let spots = [];
            for (let w of windows) {
                if (!w || w.is_hidden() || w.is_override_redirect() || w.minimized) continue;

                // Ignorar la propia ventana de Fuko / Claudia y el Mainframe
                const title = w.get_title() || '';
                const wmClass = (w.get_wm_class() || '').toLowerCase();
                const titleLower = title.toLowerCase();
                if (
                    titleLower === 'fuko' ||
                    titleLower.includes('fuko mainframe') ||
                    titleLower.includes('claudia') ||
                    wmClass.includes('fuko') ||
                    wmClass.includes('claudia')
                ) {
                    continue;
                }

                // Solo ventanas normales y de diálogo
                const type = w.get_window_type();
                if (type !== Meta.WindowType.NORMAL && type !== Meta.WindowType.DIALOG) continue;

                const rect = w.get_frame_rect();
                if (rect.width < 120 || rect.height < 60) continue;

                const max = w.get_maximized();
                const isMax = (max === Meta.MaximizeFlags.BOTH);
                const isFocused = (focusWin && w === focusWin) || w.has_focus();

                spots.push({
                    id: String(w.get_stable_sequence()),
                    title: title,
                    wm_class: wmClass,
                    x: rect.x,
                    y: rect.y,
                    width: rect.width,
                    height: rect.height,
                    maximized: isMax,
                    focused: Boolean(isFocused)
                });
            }

            // Ordenar para que la ventana enfocada quede primero
            spots.sort((a, b) => (b.focused ? 1 : 0) - (a.focused ? 1 : 0));

            return JSON.stringify(spots);
        } catch (e) {
            console.error('[ClaudiaPerch] Error obteniendo ventanas:', e);
            return '[]';
        }
    }

    TakeScreenshotAsync(params, invocation) {
        try {
            const [targetPath] = params;
            const outPath = targetPath || '/tmp/claudia_notebook_screen.png';
            const file = Gio.File.new_for_path(outPath);
            const stream = file.replace(null, false, Gio.FileCreateFlags.NONE, null);
            const screenshot = new Shell.Screenshot();

            screenshot.screenshot(false, stream, (obj, res) => {
                try {
                    screenshot.screenshot_finish(res);
                    stream.close(null);
                    invocation.return_value(new GLib.Variant('(b)', [true]));
                } catch (err) {
                    try { stream.close(null); } catch (_) {}
                    console.error('[ClaudiaPerch] Error en screenshot_finish:', err);
                    invocation.return_value(new GLib.Variant('(b)', [false]));
                }
            });
        } catch (e) {
            console.error('[ClaudiaPerch] Error en TakeScreenshot:', e);
            invocation.return_value(new GLib.Variant('(b)', [false]));
        }
    }
}
