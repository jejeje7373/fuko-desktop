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
        console.log('[ClaudiaPerch] Extensión activada y exportada en DBus: org.openclaw.ClaudiaPerch');
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

            let spots = [];
            for (let w of windows) {
                if (!w || w.is_hidden() || w.is_override_redirect()) continue;
                
                // Ignorar la propia ventana de Claudia
                const title = w.get_title() || '';
                const wmClass = (w.get_wm_class() || '').toLowerCase();
                if (title.includes('Claudia') || wmClass.includes('claudia')) continue;

                // Solo ventanas normales y de diálogo
                const type = w.get_window_type();
                if (type !== Meta.WindowType.NORMAL && type !== Meta.WindowType.DIALOG) continue;

                const rect = w.get_frame_rect();
                if (rect.width < 100 || rect.height < 50) continue;

                // Comprobar si está maximizada completa
                const max = w.get_maximized();
                const isMax = (max === Meta.MaximizeFlags.BOTH);

                spots.push({
                    title: title,
                    wm_class: wmClass,
                    x: rect.x,
                    y: rect.y,
                    width: rect.width,
                    height: rect.height,
                    maximized: isMax
                });
            }

            return JSON.stringify(spots);
        } catch (e) {
            console.error('[ClaudiaPerch] Error obteniendo ventanas:', e);
            return '[]';
        }
    }
}
