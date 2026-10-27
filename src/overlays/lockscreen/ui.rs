use super::coordinator::LockIpc;
use chrono::Local;
use iced::widget::{column, container, text};
use iced::{Alignment, Element, Length, Padding};
use std::sync::mpsc;

use crate::libraries::config::theme::ShellTheme;
use crate::libraries::style::shape_box::{ShapeBox, ShapeType};
use crate::libraries::style::TextExt;

#[derive(Debug, Clone)]
pub enum LockMessage {
    KeyPress(iced::keyboard::Key),
    Tick,
    PollIpc,
    AuthFailed,
    AuthSuccess,
}

pub struct LockscreenOverlay {
    pub buffer_len: usize,
    pub auth_failed: bool,
    pub is_caps_lock: bool,
    /// Snapshot of `ShellSettings::global_padding` at construction time.
    /// Read in `view()` (which runs at 60fps), so we cache it here instead
    /// of calling `ShellSettings::load()` on every frame — that would read
    /// + parse the JSON file 60×/sec per child.
    cached_global_padding: u16,
    ipc_tx: Option<mpsc::Sender<LockIpc>>,
    ipc_rx: Option<mpsc::Receiver<LockIpc>>,
    pending_sends: u32,
    last_tick: std::time::Instant,
}

impl Default for LockscreenOverlay {
    fn default() -> Self {
        Self::new()
    }
}

impl LockscreenOverlay {
    pub fn new() -> Self {
        // Only --lockscreen-instance children have MBS_LOCK_SOCKET. Bar,
        // notif-surface, and tooltip processes do not, and warning about
        // it would be a false alarm.
        let (ipc_tx, ipc_rx) = if std::env::var_os("MBS_LOCK_SOCKET").is_none() {
            (None, None)
        } else {
            match Self::connect_coordinator() {
                Some(pair) => {
                    eprintln!("[lock-child] connected to coordinator");
                    (Some(pair.0), Some(pair.1))
                }
                None => {
                    eprintln!("[lock-child] !!! socket set but connect failed");
                    (None, None)
                }
            }
        };
        let cached_global_padding = crate::libraries::config::ShellSettings::load().global_padding;
        Self {
            buffer_len: 0,
            auth_failed: false,
            is_caps_lock: Self::check_caps_lock(),
            cached_global_padding,
            ipc_tx,
            ipc_rx,
            pending_sends: 0,
            last_tick: std::time::Instant::now(),
        }
    }

    /// Connect to coordinator. Returns (tx_to_send_keys, rx_for_updates).
    /// Uses two threads: one reads coordinator messages, one writes ours.
    fn connect_coordinator() -> Option<(mpsc::Sender<LockIpc>, mpsc::Receiver<LockIpc>)> {
        let path = match std::env::var("MBS_LOCK_SOCKET") {
            Ok(p) => {
                eprintln!("[lock-child-v225] socket env set: {:?}", p);
                p
            }
            Err(_) => {
                eprintln!("[lock-child-v225] MBS_LOCK_SOCKET NOT SET");
                return None;
            }
        };
        eprintln!(
            "[lock-child-v225] attempting UnixStream::connect({:?})",
            path
        );
        let stream = match std::os::unix::net::UnixStream::connect(&path) {
            Ok(s) => {
                eprintln!("[lock-child-v225] socket CONNECTED");
                s
            }
            Err(e) => {
                eprintln!("[lock-child-v225] connect failed: {}", e);
                return None;
            }
        };
        let reader_stream = stream.try_clone().ok()?;
        let writer_stream = stream;

        let (out_tx, out_rx) = mpsc::channel::<LockIpc>();
        let (in_tx, in_rx) = mpsc::channel::<LockIpc>();

        // Reader thread: coordinator → us
        std::thread::spawn(move || {
            use std::io::{BufRead, BufReader};
            let reader = BufReader::new(reader_stream);
            for line in reader.lines() {
                let Ok(line) = line else {
                    break;
                };
                let Ok(msg) = serde_json::from_str::<LockIpc>(&line) else {
                    continue;
                };
                if in_tx.send(msg).is_err() {
                    break;
                }
            }
            eprintln!("[lock-child] coordinator reader closed");
        });

        // Writer thread: us → coordinator
        std::thread::spawn(move || {
            use std::io::Write;
            let mut w = writer_stream;
            while let Ok(msg) = out_rx.recv() {
                let mut line = serde_json::to_string(&msg).unwrap_or_default();
                line.push('\n');
                if w.write_all(line.as_bytes()).is_err() {
                    break;
                }
            }
            eprintln!("[lock-child] coordinator writer closed");
        });

        Some((out_tx, in_rx))
    }

    pub fn check_caps_lock() -> bool {
        for i in 0..6 {
            let p = format!("/sys/class/leds/input{}::capslock/brightness", i);
            if let Ok(c) = std::fs::read_to_string(p) {
                if c.trim() == "1" {
                    return true;
                }
            }
        }
        false
    }

    /// Drain pending IPC messages. Returns one of:
    ///   Some(LockMessage::AuthSuccess)  — coordinator signaled success
    ///   None otherwise (updates applied in-place)
    pub fn drain_ipc(&mut self) -> Option<LockMessage> {
        let mut result = None;
        let Some(rx) = self.ipc_rx.as_ref() else {
            return None;
        };
        while let Ok(msg) = rx.try_recv() {
            match msg {
                LockIpc::BufferUpdate(n) => {
                    self.pending_sends = self.pending_sends.saturating_sub(1);
                    if self.pending_sends == 0 {
                        // No in-flight sends — coordinator's count reflects
                        // every keystroke we've made, adopt it.
                        self.buffer_len = n;
                    }
                    // else: this is a stale ack of an earlier keystroke;
                    // our optimistic count has already advanced past it,
                    // so leave buffer_len alone.
                    self.auth_failed = false;
                }
                LockIpc::Reset => {
                    self.buffer_len = 0;
                    self.pending_sends = 0;
                    self.auth_failed = true;
                }
                LockIpc::Success => {
                    result = Some(LockMessage::AuthSuccess);
                }
                _ => {}
            }
        }
        result
    }

    /// Forward a key event to the coordinator. Applies optimistic local
    /// updates so dots appear immediately without waiting for the round-trip
    /// confirmation. Coordinator's BufferUpdate will overwrite if drift occurs.
    pub fn forward_key(&mut self, key: &iced::keyboard::Key) {
        let (msg, delta) = match key {
            iced::keyboard::Key::Character(c) => (Some(LockIpc::Key(c.to_string())), 1_i32),
            iced::keyboard::Key::Named(iced::keyboard::key::Named::Backspace) => {
                (Some(LockIpc::Backspace), -1_i32)
            }
            iced::keyboard::Key::Named(iced::keyboard::key::Named::Enter) => {
                (Some(LockIpc::Submit), 0_i32)
            }
            _ => (None, 0_i32),
        };

        let Some(msg) = msg else {
            return;
        };
        if let Some(tx) = self.ipc_tx.as_ref() {
            if tx.send(msg).is_err() {
                // Channel dead; don't apply optimistic update that will
                // never be reconciled.
                return;
            }
        }
        self.pending_sends = self.pending_sends.saturating_add(1);

        // Optimistic local update — instant visual feedback
        if delta > 0 {
            self.buffer_len = self.buffer_len.saturating_add(delta as usize);
        } else if delta < 0 {
            self.buffer_len = self.buffer_len.saturating_sub((-delta) as usize);
        }
    }

    pub fn view<'a>(&'a self, theme: &'a ShellTheme) -> Element<'a, LockMessage> {
        let time_str = Local::now().format("%H:%M").to_string();
        let date_str = Local::now().format("%A, %B %d").to_string();

        let card_w = 640.0_f32;
        let card_h = 460.0_f32;
        let cut_f = theme.overlay_cut;
        let angle_deg = theme.overlay_angle;

        // Bordered password field
        let (display_text, display_color, field_border) = if self.auth_failed {
            ("Incorrect password".to_string(), theme.danger, theme.danger)
        } else if self.buffer_len == 0 {
            ("Enter password…".to_string(), theme.muted, theme.border)
        } else {
            let mut s = String::new();
            for i in 0..self.buffer_len {
                if i > 0 && i % 4 == 0 {
                    s.push(' ');
                }
                s.push('•');
            }
            (s, theme.text_strong, theme.border)
        };

        // Octagon password field — same ShapeType::Hexagon family used by
        // the clipboard search bar, dictionary rows, and other overlay fields.
        // 360x56 with cut=16 gives a clearly visible octagon without
        // clipping the "Incorrect password" text or a 20-char dot display.
        let pad = self.cached_global_padding as f32;

        // Field dimensions scale with global_padding. At pad=12 (default)
        // this resolves to 360x56 cut=16 — matching V185. Smaller padding
        // → smaller field; larger → bigger, up to hard caps so the octagon
        // never overflows the card.
        let field_w = (280.0 + pad * 6.67).min(520.0);
        let field_h = (40.0 + pad * 1.33).min(80.0);
        let cut_u16 = (pad * 1.33).max(8.0).min(28.0) as u16;

        {
            use std::sync::OnceLock;
            static ONCE: OnceLock<()> = OnceLock::new();
            ONCE.get_or_init(|| {
                eprintln!(
                    "[lock-child] field: global_padding={} → w={:.0} h={:.0} cut={}",
                    self.cached_global_padding, field_w, field_h, cut_u16
                );
            });
        }

        let field_bg = ShapeBox::new()
            .shape(ShapeType::Hexagon { cut: cut_u16 })
            .background(theme.surface_alt)
            .border(field_border, theme.control_border_width.min(2.0))
            .size(Length::Fixed(field_w), Length::Fixed(field_h));

        let field_text = container(
            text(display_text)
                .size(18)
                .bold()
                .color(display_color)
                .align_x(Alignment::Center)
                .wrapping(iced::widget::text::Wrapping::None),
        )
        .width(Length::Fixed(field_w))
        .height(Length::Fixed(field_h))
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .padding(Padding {
            top: 0.0,
            right: cut_u16 as f32 + 4.0,
            bottom: 0.0,
            left: cut_u16 as f32 + 4.0,
        });

        let field_stack = iced::widget::Stack::new()
            .push(field_bg.view())
            .push(field_text);

        let dots_row = container(field_stack)
            .width(Length::Fill)
            .align_x(Alignment::Center)
            .clip(true)
            .padding(Padding {
                top: 28.0,
                right: 0.0,
                bottom: 0.0,
                left: 0.0,
            });

        let mut inner: iced::widget::Column<'a, LockMessage> = column![
            text(time_str).size(64).bold().color(theme.text_strong),
            text(date_str).size(16).color(theme.muted),
        ]
        .spacing(2)
        .align_x(Alignment::Center);

        inner = inner.push(dots_row);

        if self.is_caps_lock {
            inner = inner.push(
                container(text("⇪ CAPS LOCK ON").size(11).bold().color(theme.warning)).padding(
                    Padding {
                        top: 12.0,
                        right: 0.0,
                        bottom: 0.0,
                        left: 0.0,
                    },
                ),
            );
        }

        inner = inner.push(
            container(
                text("Type to unlock  •  Enter to submit")
                    .size(10)
                    .color(theme.muted),
            )
            .padding(Padding {
                top: 24.0,
                right: 0.0,
                bottom: 0.0,
                left: 0.0,
            }),
        );

        let card_content = container(inner)
            .width(Length::Fixed(card_w - 80.0))
            .height(Length::Fixed(card_h - 80.0))
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .clip(true);

        let card_bg = ShapeBox::new()
            .shape(ShapeType::AngledHexagon {
                cut: cut_f,
                angle_deg,
            })
            .background(theme.background)
            .border(
                if self.auth_failed {
                    theme.danger
                } else {
                    theme.text
                },
                theme.border_width,
            )
            .size(Length::Fixed(card_w), Length::Fixed(card_h));

        let centered = container(card_content)
            .width(Length::Fixed(card_w))
            .height(Length::Fixed(card_h))
            .align_x(Alignment::Center)
            .align_y(Alignment::Center);

        let card = iced::widget::Stack::new()
            .push(card_bg.view())
            .push(centered);

        container(
            container(card)
                .center_x(Length::Fill)
                .center_y(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_t| container::Style {
            background: Some(iced::Background::Color(theme.background)),
            ..Default::default()
        })
        .into()
    }
}
