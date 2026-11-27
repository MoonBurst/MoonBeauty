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
    pub is_resident: bool,
    pub desktop_entry: Option<String>,
    pub actions: Vec<String>,
}

pub struct NotifServer {
    // Bounded channel so a flood of notifications cannot OOM the shell.
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
        replaces_id: u32,
        _app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        hints: HashMap<String, OwnedValue>,
        _expire_timeout: i32,
    ) -> u32 {
        // Cap payload fields at ingress before try_send into the bounded channel.
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
        
        let id = if replaces_id != 0 {
            replaces_id
        } else {
            next_id()
        };
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
        let is_resident = hints
            .get("resident")
            .and_then(|v| match &**v {
                zbus::zvariant::Value::Bool(b) => Some(*b),
                zbus::zvariant::Value::U8(n) => Some(*n != 0),
                _ => None,
            })
            .unwrap_or(false);
        let desktop_entry = hints
            .get("desktop-entry")
            .or_else(|| hints.get("desktop_entry"))
            .and_then(|v| match &**v {
                zbus::zvariant::Value::Str(s) => Some(s.as_str().to_string()),
                _ => None,
            });
        // Non-blocking try_send: drop payload on full channel while still returning
        // id immediately per the freedesktop notifications spec.
        if let Err(e) = self.tx.try_send(IncomingNotification {
            id,
            app_name,
            summary,
            body,
            is_urgent,
            is_resident,
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

async fn emit_activation_token_async(conn: &zbus::Connection, id: u32, token: &str) {
    let body = (id, token.to_string());
    match conn
        .emit_signal(
            None::<&str>,
            "/org/freedesktop/Notifications",
            "org.freedesktop.Notifications",
            "ActivationToken",
            &body,
        )
        .await
    {
        Ok(_) => eprintln!("[activation-token] sent id={} len={}", id, token.len()),
        Err(e) => eprintln!("[activation-token] emit failed: {e}"),
    }
}

async fn emit_action_invoked_async(conn: &zbus::Connection, id: u32, key: &str) {
    let body = (id, key.to_string());
    match conn
        .emit_signal(
            None::<&str>,
            "/org/freedesktop/Notifications",
            "org.freedesktop.Notifications",
            "ActionInvoked",
            &body,
        )
        .await
    {
        Ok(_) => eprintln!("[action-invoked] sent id={} key={:?}", id, key),
        Err(e) => eprintln!("[action-invoked] emit failed: {e}"),
    }
}

async fn emit_notification_closed_async(conn: &zbus::Connection, id: u32, reason: u32) {
    match conn
        .emit_signal(
            None::<&str>,
            "/org/freedesktop/Notifications",
            "org.freedesktop.Notifications",
            "NotificationClosed",
            &(id, reason),
        )
        .await
    {
        Ok(_) => eprintln!("[notif-closed] sent id={} reason={}", id, reason),
        Err(e) => eprintln!("[notif-closed] emit failed: {e}"),
    }
}

/// Standard freedesktop notification click handshake:
///   1. (caller generated Wayland xdg-activation token)
///   2. Emit ActivationToken(id, token) BEFORE ActionInvoked
///   3. Emit ActionInvoked(id, action_key) if a matching key was registered
///   4. Emit NotificationClosed(id, 2) unless resident
pub async fn emit_click_handshake(
    id: u32,
    activation_token: Option<String>,
    action_key: Option<String>,
    is_resident: bool,
) {
    let Some(conn) = DBUS_CONN.get().cloned() else {
        eprintln!("[click-handshake] daemon connection not ready");
        return;
    };
    if let Some(token) = activation_token {
        if !token.is_empty() {
            emit_activation_token_async(&conn, id, &token).await;
        }
    }
    if let Some(key) = action_key {
        emit_action_invoked_async(&conn, id, &key).await;
    }
    if !is_resident {
        emit_notification_closed_async(&conn, id, 2).await;
    }
}

pub async fn emit_notification_closed(id: u32, reason: u32) {
    let Some(conn) = DBUS_CONN.get().cloned() else {
        eprintln!("[notif-closed] daemon connection not ready");
        return;
    };
    emit_notification_closed_async(&conn, id, reason).await;
}

pub fn emit_action_invoked(id: u32, key: &str) {
    let key_owned = key.to_string();
    std::thread::spawn(move || {
        let Ok(rt) = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        else {
            return;
        };
        rt.block_on(emit_click_handshake(id, None, Some(key_owned), false));
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
