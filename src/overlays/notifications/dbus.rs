use std::collections::HashMap;
use std::sync::mpsc::Sender;
use zbus::zvariant::OwnedValue;

#[derive(Debug, Clone)]
pub struct IncomingNotification {
    pub id: u32,
    pub app_name: String,
    pub summary: String,
    pub body: String,
    pub is_urgent: bool,
    pub desktop_entry: Option<String>,
    pub actions: Vec<String>,
}

pub struct NotifServer {
    tx: Sender<IncomingNotification>,
}

fn next_id() -> u32 {
    use std::sync::atomic::{AtomicU32, Ordering};
    static NEXT: AtomicU32 = AtomicU32::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

#[zbus::interface(name = "org.freedesktop.Notifications")]
impl NotifServer {
    fn notify(
        &self,
        app_name: String,
        _replaces_id: u32,
        _app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        hints: HashMap<String, OwnedValue>,
        _expire_timeout: i32,
    ) -> u32 {
        let id = next_id();
        eprintln!("[notif-raw] app={:?} summary={:?} body={:?}", app_name, summary, body);
        let is_urgent = hints
            .get("urgency")
            .and_then(|v| u8::try_from(v).ok())
            .map(|u| u == 2)
            .unwrap_or(false);
        let desktop_entry = hints
            .get("desktop-entry")
            .or_else(|| hints.get("desktop_entry"))
            .and_then(|v| match &**v {
                zbus::zvariant::Value::Str(s) => Some(s.as_str().to_string()),
                _ => None,
            });
        let _ = self.tx.send(IncomingNotification {
            id, app_name, summary, body, is_urgent, desktop_entry, actions,
        });
        id
    }

    fn close_notification(&self, _id: u32) {}

    fn get_capabilities(&self) -> Vec<String> {
        vec!["body".into(), "actions".into()]
    }

    fn get_server_information(&self) -> (String, String, String, String) {
        (
            "MoonBeautyShell".into(),
            "MoonBeauty".into(),
            "1.0".into(),
            "1.2".into(),
        )
    }
}

use std::sync::OnceLock;
static DBUS_CONN: OnceLock<zbus::Connection> = OnceLock::new();

pub fn emit_action_invoked(id: u32, key: &str) {
    let Some(conn) = DBUS_CONN.get().cloned() else {
        eprintln!("[action-invoked] daemon connection not ready");
        return;
    };
    let key_owned = key.to_string();
    std::thread::spawn(move || {
        let Ok(rt) = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        else { return; };
        rt.block_on(async move {
            let body = (id, key_owned.clone());
            let r = conn.emit_signal(
                None::<&str>,
                "/org/freedesktop/Notifications",
                "org.freedesktop.Notifications",
                "ActionInvoked",
                &body,
            ).await;
            match r {
                Ok(_) => eprintln!("[action-invoked] sent id={} key={:?}", id, key_owned),
                Err(e) => eprintln!("[action-invoked] emit failed: {e}"),
            }
            let _ = conn.emit_signal(
                None::<&str>,
                "/org/freedesktop/Notifications",
                "org.freedesktop.Notifications",
                "NotificationClosed",
                &(id, 2u32),
            ).await;
        });
    });
}

pub fn spawn(tx: Sender<IncomingNotification>) {
    std::thread::spawn(move || {
        let rt = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(r) => r,
            Err(e) => { eprintln!("[notif_dbus] rt build: {e}"); return; }
        };
        rt.block_on(async move {
            let server = NotifServer { tx };
            let conn = zbus::connection::Builder::session()
                .and_then(|b| b.name("org.freedesktop.Notifications"))
                .and_then(|b| b.serve_at("/org/freedesktop/Notifications", server))
                .expect("zbus builder")
                .build()
                .await;
            match conn {
                Ok(c) => {
                    eprintln!("[notif_dbus] org.freedesktop.Notifications owned");
                    let _ = DBUS_CONN.set(c.clone());
                    std::future::pending::<()>().await;
                }
                Err(e) => eprintln!("[notif_dbus] serve failed: {e}"),
            }
        });
    });
}
