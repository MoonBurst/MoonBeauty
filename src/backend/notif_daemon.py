#!/usr/bin/env python3
import sys, os, socket, json

try:
    import gi
    gi.require_version('Gio', '2.0')
    from gi.repository import Gio, GLib
except Exception:
    sys.exit(0)

SOCKET_PATH = os.path.join(os.environ.get("XDG_RUNTIME_DIR", f"/tmp/user-{os.getuid()}"), "moon-beauty-shell.sock")

def send_to_shell(cmd):
    try:
        s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        s.connect(SOCKET_PATH)
        s.sendall(json.dumps(cmd).encode('utf-8'))
        s.close()
    except Exception:
        pass

class NotificationService:
    def __init__(self):
        self.node_info = Gio.DBusNodeInfo.new_for_xml('''
        <node>
            <interface name="org.freedesktop.Notifications">
                <method name="GetCapabilities">
                    <arg type="as" name="capabilities" direction="out"/>
                </method>
                <method name="Notify">
                    <arg type="s" name="app_name" direction="in"/>
                    <arg type="u" name="replaces_id" direction="in"/>
                    <arg type="s" name="app_icon" direction="in"/>
                    <arg type="s" name="summary" direction="in"/>
                    <arg type="s" name="body" direction="in"/>
                    <arg type="as" name="actions" direction="in"/>
                    <arg type="a{sv}" name="hints" direction="in"/>
                    <arg type="i" name="expire_timeout" direction="in"/>
                    <arg type="u" name="id" direction="out"/>
                </method>
                <method name="CloseNotification">
                    <arg type="u" name="id" direction="in"/>
                </method>
                <method name="GetServerInformation">
                    <arg type="s" name="name" direction="out"/>
                    <arg type="s" name="vendor" direction="out"/>
                    <arg type="s" name="version" direction="out"/>
                    <arg type="s" name="spec_version" direction="out"/>
                </method>
                <signal name="NotificationClosed">
                    <arg type="u" name="id"/>
                    <arg type="u" name="reason"/>
                </signal>
            </interface>
        </node>
        ''')
        self.next_id = 1

    def on_bus_acquired(self, conn, name):
        conn.register_object(
            "/org/freedesktop/Notifications",
            self.node_info.interfaces[0],
            self.handle_method_call,
            None,
            None
        )

    def on_name_lost(self, conn, name):
        pass

    def handle_method_call(self, conn, sender, path, iface, method, params, invocation):
        if method == "GetCapabilities":
            invocation.return_value(GLib.Variant("(as)", (["body", "actions"],)))
        elif method == "GetServerInformation":
            invocation.return_value(GLib.Variant("(ssss)", ("MoonBeautyShell", "MoonBeauty", "1.0", "1.2")))
        elif method == "Notify":
            app_name, replaces_id, app_icon, summary, body, actions, hints, timeout = params
            urgency = hints.get("urgency", 1)
            is_urgent = (urgency == 2)
            nid = self.next_id
            self.next_id += 1
            cmd = {
                "IncomingNotif": {
                    "app_name": str(app_name),
                    "summary": str(summary),
                    "body": str(body),
                    "is_urgent": bool(is_urgent)
                }
            }
            send_to_shell(cmd)
            invocation.return_value(GLib.Variant("(u)", (nid,)))
        elif method == "CloseNotification":
            invocation.return_value(None)

server = NotificationService()

# Claim org.freedesktop.Notifications synchronously. Gio.bus_own_name wants a
# GObject.Closure in strict PyGObject builds; RequestName via call_sync works
# in every version and avoids the closure argument entirely.
def _claim_name():
    bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
    try:
        res = bus.call_sync(
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "RequestName",
            GLib.Variant("(su)", ("org.freedesktop.Notifications", 0x4)),
            None,
            Gio.DBusCallFlags.NONE,
            -1,
            None,
        )
        code = res.unpack()[0]
    except Exception as e:
        print("[notif_daemon] RequestName failed: %s" % e, file=sys.stderr)
        return None
    if code not in (1, 4):
        print("[notif_daemon] RequestName returned %d (not primary owner)" % code, file=sys.stderr)
        return None
    return bus

_bus = _claim_name()
if _bus is None:
    sys.exit(1)

_bus.register_object(
    "/org/freedesktop/Notifications",
    server.node_info.interfaces[0],
    server.handle_method_call,
    None,
    None,
)

loop = GLib.MainLoop()
try:
    loop.run()
except KeyboardInterrupt:
    pass
