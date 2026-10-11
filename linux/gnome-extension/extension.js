import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import St from 'gi://St';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';

const NAME = 'dev.october.Lantern';
const XML = `<node><interface name="${NAME}">
<method name="GetSnapshot"><arg type="s" direction="out"/></method>
<method name="Show"><arg type="s" direction="in"/></method>
<method name="PointAsk"/>
</interface></node>`;
const Proxy = Gio.DBusProxy.makeProxyWrapper(XML);

export default class LanternExtension extends Extension {
    enable() {
        this._generation = (this._generation ?? 0) + 1;
        const generation = this._generation;
        this._agents = [];
        this._shellBus = Gio.DBusExportedObject.wrapJSObject('<node><interface name="dev.october.Lantern.Shell"><method name="Raise"/></interface></node>', this);
        this._shellBus.export(Gio.DBus.session, '/dev/october/Lantern/Shell');
        this._shellOwner = Gio.bus_own_name_on_connection(Gio.DBus.session,
            'dev.october.Lantern.Shell', Gio.BusNameOwnerFlags.NONE, null, null);
        this._raiseTimer = null;
        this._label = new St.Label({text: '◉', y_align: Clutter.ActorAlign.CENTER});
        this._button = new St.Button({style_class: 'lantern-edge', reactive: true,
            can_focus: true, track_hover: true, child: this._label,
            accessible_name: 'October Lantern agents'});
        Main.layoutManager.addChrome(this._button, {trackFullscreen: true});
        this._menu = new PopupMenu.PopupMenu(this._button, 0.5, St.Side.RIGHT);
        this._manager = new PopupMenu.PopupMenuManager(this._button);
        this._manager.addMenu(this._menu);
        Main.uiGroup.add_child(this._menu.actor);
        this._menu.actor.hide();
        this._button.connect('clicked', () => {
            this._rebuild();
            this._menu.toggle();
        });
        // Hover reveals the same session list without moving a desktop window on Wayland.
        this._button.connect('notify::hover', () => {
            if (this._button.hover && !this._menu.isOpen) {
                this._rebuild();
                this._menu.open();
            }
        });
        this._position = () => {
            const monitor = Main.layoutManager.primaryMonitor;
            if (monitor)
                this._button.set_position(monitor.x + 8,
                    monitor.y + Math.round(monitor.height * 0.45));
        };
        this._position();
        this._monitors = Main.layoutManager.connect('monitors-changed', this._position);
        this._proxy = new Proxy(Gio.DBus.session, NAME, '/dev/october/Lantern', (proxy, error) => {
            if (generation !== this._generation)
                return;
            if (error) {
                console.error(`Lantern: ${error.message}`);
                return;
            }
            this._proxy = proxy;
            this._poll(generation);
        });
        this._timer = GLib.timeout_add_seconds(GLib.PRIORITY_DEFAULT, 2, () => {
            this._poll(generation);
            return GLib.SOURCE_CONTINUE;
        });
    }

    _poll(generation) {
        if (this._polling || !this._proxy)
            return;
        this._polling = true;
        this._proxy.GetSnapshotRemote((result, error) => {
            this._polling = false;
            if (generation !== this._generation)
                return;
            if (error) {
                this._agents = [];
            } else {
                try {
                    const agents = JSON.parse(result[0]);
                    this._agents = Array.isArray(agents) ? agents.slice(0, 100) : [];
                } catch {
                    this._agents = [];
                }
            }
            const needs = this._agents.filter(a => a.state === 'needs_input').length;
            this._label.text = needs ? `◉ ${needs}` : '◉';
            this._button.set_style(needs ? 'color: #f29950;' : 'color: #c8cdd5;');
            this._button.accessible_name = `October Lantern: ${this._agents.length} agents, ${needs} need input`;
            if (this._menu.isOpen)
                this._rebuild();
        });
    }

    Raise() {
        // The compositor owns focus on Wayland. This only raises Lantern's own window.
        if (this._raiseTimer)
            GLib.source_remove(this._raiseTimer);
        const generation = this._generation;
        this._raiseTimer = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 150, () => {
            this._raiseTimer = null;
            if (generation !== this._generation)
                return GLib.SOURCE_REMOVE;
            const actor = global.get_window_actors().find(actor =>
                actor.meta_window.get_wm_class() === 'dev.october.lantern');
            if (actor)
                Main.activateWindow(actor.meta_window, global.get_current_time());
            return GLib.SOURCE_REMOVE;
        });
    }

    _open(agent = '') {
        this._menu.close();
        this._proxy?.ShowRemote(agent, (_, error) => {
            if (!error)
                this.Raise();
            else
                Gio.DesktopAppInfo.new('dev.october.lantern.desktop')?.launch([], global.create_app_launch_context(0, -1));
        });
    }

    _rebuild() {
        this._menu.removeAll();
        this._menu.addAction('October Lantern', () => this._open());
        this._menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());
        if (!this._agents.length) {
            const empty = new PopupMenu.PopupMenuItem('No active agents', {reactive: false});
            this._menu.addMenuItem(empty);
        }
        for (const agent of this._agents) {
            const state = ({needs_input: 'Needs you', working: 'Working', waiting: 'Ready', idle: 'Idle'})[agent.state] ?? 'Unknown';
            const label = `${String(agent.handle ?? '').slice(0, 40)} · ${String(agent.project ?? '').slice(0, 40)} · ${state}`;
            this._menu.addAction(label, () => this._open(String(agent.id ?? '')));
        }
        this._menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());
        this._menu.addAction('Point & Ask', () => {
            this._menu.close();
            this._proxy?.PointAskRemote((_, error) => {
                if (error)
                    this._open();
            });
        });
    }

    disable() {
        this._generation++;
        if (this._raiseTimer)
            GLib.source_remove(this._raiseTimer);
        if (this._shellOwner)
            Gio.bus_unown_name(this._shellOwner);
        this._shellBus?.unexport();
        this._shellBus = null;
        this._shellOwner = null;
        this._raiseTimer = null;
        if (this._timer)
            GLib.source_remove(this._timer);
        if (this._monitors)
            Main.layoutManager.disconnect(this._monitors);
        this._menu?.destroy();
        this._button?.destroy();
        this._timer = null;
        this._monitors = null;
        this._menu = null;
        this._button = null;
        this._proxy = null;
        this._agents = [];
    }
}
