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
        _actions: Vec<String>,
        hints: HashMap<String, OwnedValue>,
        _expire_timeout: i32,
    ) -> u32 {
        let id = next_id();
        let is_urgent = hints
            .get("urgency")
            .and_then(|v| u8::try_from(v).ok())
            .map(|u| u == 2)
            .unwrap_or(false);
        let _ = self.tx.send(IncomingNotification {
            id, app_name, summary, body, is_urgent,
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
                Ok(_c) => {
                    eprintln!("[notif_dbus] org.freedesktop.Notifications owned");
                    std::future::pending::<()>().await;
                    let _ = _c;
                }
                Err(e) => eprintln!("[notif_dbus] serve failed: {e}"),
            }
        });
    });
}
