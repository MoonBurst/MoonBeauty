use std::collections::HashMap;
use std::sync::mpsc::SyncSender;
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
    // Finding 55: bounded channel. Was Sender (unbounded); a hostile
    // notify-send loop outpaces the receiver and OOMs the shell.
    tx: SyncSender<IncomingNotification>,
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
        // Finding 55: cap payload fields at ingress before try_send
        // into the bounded channel (sync_channel 256). Without this,
        // a hostile client can queue 256 x N MB and OOM the shell.
        const MAX_APP_NAME: usize = 128;
        const MAX_SUMMARY: usize = 512;
        const MAX_BODY: usize = 4096;
        const MAX_ACTIONS: usize = 16;
        const MAX_ACTION_LEN: usize = 256;
        let cap = |s: String, n: usize| -> String {
            if s.chars().count() <= n {
                s
            } else {
                s.chars().take(n).collect()
            }
        };
        let app_name = cap(app_name, MAX_APP_NAME);
        let summary = cap(summary, MAX_SUMMARY);
        let body = cap(body, MAX_BODY);
        let actions: Vec<String> = actions
            .into_iter()
            .take(MAX_ACTIONS)
            .map(|a| cap(a, MAX_ACTION_LEN))
            .collect();
        
        let id = next_id();
        eprintln!(
            "[notif-raw] app={:?} summary={:?} body={:?}",
            app_name, summary, body
        );
        // Hints HashMap is deserialized by zbus before this fn runs, and
        // can hold up to the D-Bus message-size cap. We only read two
        // keys; cap at ingress so a hostile sender can't pin a large map
        // in the bounded channel.
        let hints: HashMap<String, OwnedValue> = hints.into_iter().take(32).collect();

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
        // Finding 55: try_send (non-blocking). On full, drop the payload
        // but still return id — matches D-Bus spec (notify() returns
        // immediately) and dunst's queue-full policy (no error to well-
        // behaved apps that would retry and amplify the flood).
        if let Err(e) = self.tx.try_send(IncomingNotification {
            id,
            app_name,
            summary,
            body,
            is_urgent,
            desktop_entry,
            actions,
        }) {
            crate::dbg_log!("[notif] dropped (channel not ready: {:?})", e);
        }
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
        else {
            return;
        };
        rt.block_on(async move {
            let body = (id, key_owned.clone());
            let r = conn
                .emit_signal(
                    None::<&str>,
                    "/org/freedesktop/Notifications",
                    "org.freedesktop.Notifications",
                    "ActionInvoked",
                    &body,
                )
                .await;
            match r {
                Ok(_) => eprintln!("[action-invoked] sent id={} key={:?}", id, key_owned),
                Err(e) => eprintln!("[action-invoked] emit failed: {e}"),
            }
            let _ = conn
                .emit_signal(
                    None::<&str>,
                    "/org/freedesktop/Notifications",
                    "org.freedesktop.Notifications",
                    "NotificationClosed",
                    &(id, 2u32),
                )
                .await;
        });
    });
}

pub fn spawn(tx: SyncSender<IncomingNotification>) {
    std::thread::spawn(move || {
        let rt = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[notif_dbus] rt build: {e}");
                return;
            }
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
