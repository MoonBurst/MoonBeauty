use chrono::Local;
use iced::widget::{button, column, container, row, scrollable, text, Stack};
use iced::{Alignment, Color, Element, Length, Padding};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use crate::config::settings::ShellSettings;
use crate::config::theme::{hex_to_color, ShellTheme};
use crate::style::shape_box::{ShapeBox, ShapeType, SlantSide};
use crate::style::TextExt;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationToast {
    pub id: u32,
    pub app_name: String,
    pub summary: String,
    pub body: String,
    pub timestamp: String,
    pub is_read: bool,
    pub is_urgent: bool,
    pub border_color: String,
    #[serde(default = "default_toast_text_color")]
    pub text_color: String,
    #[serde(default)]
    pub avatar_path: Option<String>,
    #[serde(skip, default = "Instant::now")]
    pub created_at: Instant,
    #[serde(skip)]
    pub exit_started_at: Option<Instant>,
    #[serde(skip)]
    pub hold_reset_at: Option<Instant>,
    // Set when an older toast was removed and this one needs to slide
    // down into the freed slot. Never set on ADD — new toasts appear
    // above existing ones without moving them.
    #[serde(skip)]
    pub slot_shift_start: Option<Instant>,
    // Deferred TTS payload: filled on push, played only when this toast
    // becomes the front (oldest visible) toast. Otherwise a burst of
    // notifications talks over itself before any of them are readable.
    #[serde(default)]
    pub tts_summary: String,
    #[serde(default)]
    pub tts_body: String,
    #[serde(default)]
    pub tts_readout: Option<String>,
    #[serde(default)]
    pub tts_command: Option<String>,
    #[serde(default)]
    pub tts_pending: bool,
    #[serde(default)]
    pub tts_spoken: bool,
}

fn default_toast_text_color() -> String { "#f7f700".to_string() }

#[derive(Debug, Clone)]
pub enum NotifMessage {
    DismissToast(u32),
    ClearHistory,
    DeleteHistory(u32),
    ToggleHistoryDrawer,
    DismissDrawer,
}

pub struct NotificationOverlay {
    pub active_toasts: Vec<NotificationToast>,
    pub history: Vec<NotificationToast>,
    pub show_history: bool,
    // Count-animation: when active_toasts.len() changes, blend positions
    // over 150ms so existing toasts slide to their new stacked slot
    // instead of teleporting. A bare jump every time a new toast arrives
    // reads as "the animation is doing something weird".
    pub stack_target_count: usize,
    pub stack_from_count: usize,
    pub stack_anim_start: Instant,
}

impl Default for NotificationOverlay {
    fn default() -> Self {
        Self::new()
    }
}

impl NotificationOverlay {
    pub fn new() -> Self {
        Self {
            active_toasts: Vec::new(),
            history: Self::load_history(),
            show_history: false,
            stack_target_count: 0,
            stack_from_count: 0,
            stack_anim_start: Instant::now(),
        }
    }

    fn history_path() -> PathBuf {
        let base = dirs::cache_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
        base.join("moon_notification_history.json")
    }

    pub fn load_history() -> Vec<NotificationToast> {
        let path = Self::history_path();
        if path.exists() {
            if let Ok(raw) = fs::read_to_string(&path) {
                if let Ok(items) = serde_json::from_str::<Vec<NotificationToast>>(&raw) {
                    return items;
                }
            }
        }
        Vec::new()
    }

    pub fn save_history(&self) {
        let path = Self::history_path();
        if let Ok(data) = serde_json::to_string_pretty(&self.history) {
            let tmp = path.with_extension("tmp");
            if fs::write(&tmp, data).is_ok() {
                let _ = fs::rename(tmp, path);
            }
        }
    }

    pub fn push_notification(
        &mut self,
        app_name: &str,
        summary: &str,
        body: &str,
        is_urgent: bool,
        is_test: bool,
        settings: &ShellSettings,
    ) {
        let id = rand_id();

        // Match friend profile for custom border colors
        let matched_profile = settings.friend_profiles.iter().find(|p| {
            summary.to_lowercase().contains(&p.summary.to_lowercase())
                || app_name.to_lowercase().contains(&p.name.to_lowercase())
        });

        let border_color = if is_urgent {
            "#ff0000".to_string()
        } else if let Some(p) = matched_profile {
            p.color.clone()
        } else {
            settings.palette.base05.clone()
        };

        let text_color = if is_urgent {
            settings.palette.base05.clone()
        } else if let Some(p) = matched_profile {
            p.color.clone()
        } else {
            settings.palette.base05.clone()
        };
        let avatar_path: Option<String> = matched_profile.and_then(|p| {
            let specific = format!("resources/{0}/{0}.png", p.name);
            if std::path::Path::new(&specific).exists() {
                Some(specific)
            } else if std::path::Path::new("resources/fallback.png").exists() {
                Some("resources/fallback.png".to_string())
            } else {
                None
            }
        });
        let item = NotificationToast {
            id,
            app_name: app_name.to_string(),
            summary: summary.to_string(),
            body: body.to_string(),
            timestamp: Local::now().format("%I:%M %p").to_string(),
            is_read: false,
            is_urgent,
            border_color,
            text_color,
            avatar_path,
            created_at: Instant::now(),
            exit_started_at: None,
            hold_reset_at: None,
            slot_shift_start: None,
            tts_summary: String::new(),
            tts_body: String::new(),
            tts_readout: None,
            tts_command: None,
            tts_pending: false,
            tts_spoken: false,
        };

        // Do-not-disturb: save to history and held queue; do not show or speak yet.
        if dnd_active() {
            eprintln!("[dnd] holding notification back while paused: {:?}", summary);
            self.history.insert(0, item.clone());
            if self.history.len() > 80 {
                self.history.truncate(80);
            }
            self.save_history();
            let mut held = load_held_notifs();
            held.push(item);
            save_held_notifs(&held);
            eprintln!("[dnd] total notifications held: {}", held.len());
            return;
        }
        if self.active_toasts.len() >= 4 {
            self.active_toasts.remove(0);
        }
        self.active_toasts.push(item.clone());

        self.history.insert(0, item);
        if self.history.len() > 80 {
            self.history.truncate(80);
        }
        self.save_history();

        // Test notifications are completely silent; real notifications speak TTS if profile.tts is true
        // Store TTS on the toast; tick_toasts fires it when this toast
        // becomes the front (oldest non-exiting) one. Firing immediately
        // makes a burst of notifications all speak at once before any
        // are readable, which sounds like overlapping voices from nowhere.
        let tts_pending = !is_test
            && matched_profile.map(|p| p.tts).unwrap_or(false);
        let tts_readout = matched_profile.and_then(|p| p.default_readout.clone());
        let tts_command = settings.tts_command.clone();
        eprintln!(
            "[tts-defer] app={:?} pending={} summary={:?}",
            app_name, tts_pending, summary,
        );
        if let Some(t) = self.active_toasts.last_mut() {
            t.tts_summary = summary.to_string();
            t.tts_body = body.to_string();
            t.tts_readout = tts_readout;
            t.tts_command = tts_command;
            t.tts_pending = tts_pending;
        }
    }

    fn speak_tts(
        summary: &str,
        body: &str,
        custom_readout: Option<&str>,
        tts_command: Option<String>,
    ) {
        let s = summary.to_string();
        let b = body.to_string();
        let readout_template = custom_readout.unwrap_or("Message from {name}").to_string();

        std::thread::spawn(move || {
            let spoken = readout_template
                .replace("{name}", &s)
                .replace("{summary}", &s)
                .replace("{body}", &b);
            eprintln!("[tts] speak text={:?}", spoken);
            eprintln!(
                "[tts] env: XDG_RUNTIME_DIR={:?} DBUS_SESSION_BUS_ADDRESS={:?}",
                std::env::var("XDG_RUNTIME_DIR").ok(),
                std::env::var("DBUS_SESSION_BUS_ADDRESS").ok(),
            );

            // User override in settings.json ("tts_command"). The command runs
            // via sh -c so flags like --voice work. If {text} is present it
            // is substituted; otherwise the spoken text is appended as "$1".
            if let Some(cmd) = tts_command {
                let script = if cmd.contains("{text}") {
                    cmd.replace("{text}", "\"$1\"")
                } else {
                    format!("{} \"$1\"", cmd)
                };
                eprintln!("[tts] custom command: {}", script);
                match std::process::Command::new("sh")
                    .args(["-c", &script, "--", &spoken])
                    .output()
                {
                    Ok(o) => eprintln!(
                        "[tts] custom exit={:?} stderr={}",
                        o.status.code(),
                        String::from_utf8_lossy(&o.stderr)
                            .trim()
                            .chars()
                            .take(200)
                            .collect::<String>(),
                    ),
                    Err(e) => eprintln!("[tts] custom spawn failed: {e}"),
                }
                return;
            }

            let candidates: Vec<String> = vec![
                "/run/current-system/sw/bin/sage-tts".into(),
                "sage-tts".into(),
                "spd-say".into(),
                "espeak-ng".into(),
                "espeak".into(),
            ];
            let mut tried: Vec<String> = Vec::new();
            for bin in &candidates {
                tried.push(bin.clone());
                match std::process::Command::new(bin).arg(&spoken).output() {
                    Ok(o) => {
                        let stderr = String::from_utf8_lossy(&o.stderr).trim().to_string();
                        eprintln!(
                            "[tts] {} exit={:?} stderr={}",
                            bin,
                            o.status.code(),
                            stderr.chars().take(200).collect::<String>(),
                        );
                        let really_ok = o.status.success()
                            && !stderr.contains("daemon not running");
                        if really_ok {
                            return;
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(e) => {
                        eprintln!("[tts] spawn {} failed: {e}", bin);
                        return;
                    }
                }
            }
            eprintln!("[tts] NO TTS BINARY FOUND; tried {:?}", tried);
        });
    }




    pub fn tick_toasts(&mut self, hold_duration_sec: u64) {
        let hold_limit = std::time::Duration::from_secs(hold_duration_sec.max(1));
        let exit_duration = std::time::Duration::from_millis(250);
        let now = Instant::now();

        // One toast exits at a time. After it's fully removed, remaining
        // toasts get fresh hold timers so the user has time to read each
        // one before it goes.
        let any_exiting = self.active_toasts.iter().any(|t| t.exit_started_at.is_some());
        if !any_exiting {
            // Hold timer uses hold_reset_at (fallback created_at), NOT
            // created_at directly. Resetting created_at would restart
            // the grow/drop entrance animation of remaining toasts.
            let oldest_expired = self.active_toasts
                .iter_mut()
                .filter(|t| {
                    let hold_ref = t.hold_reset_at.unwrap_or(t.created_at);
                    !t.is_urgent
                        && t.exit_started_at.is_none()
                        && hold_ref.elapsed() >= hold_limit
                })
                .min_by_key(|t| t.hold_reset_at.unwrap_or(t.created_at));
            if let Some(t) = oldest_expired {
                t.exit_started_at = Some(now);
            }
        }

        let mut removed_any = false;
        self.active_toasts.retain(|t| {
            if t.is_urgent { return true; }
            match t.exit_started_at {
                Some(start) => {
                    if start.elapsed() < exit_duration { true }
                    else { removed_any = true; false }
                }
                None => true,
            }
        });

        if removed_any {
            let now = Instant::now();
            for t in self.active_toasts.iter_mut() {
                t.hold_reset_at = Some(now);
                t.exit_started_at = None;
                // Re-indexed toast needs to slide down into the slot
                // vacated by the just-removed one. Animated per-frame
                // in view_toasts. Never set on ADD, so new arrivals
                // don't nudge existing toasts.
                t.slot_shift_start = Some(now);
            }
        }

        // Fire deferred TTS for the front toast. Front = oldest toast
        // that isn't currently exiting. So when one expires and gets
        // removed, the next toast speaks on the next tick.
        let front_id = self.active_toasts
            .iter()
            .filter(|t| t.exit_started_at.is_none())
            .min_by_key(|t| t.created_at)
            .map(|t| t.id);
        if let Some(fid) = front_id {
            let payload = if let Some(t) =
                self.active_toasts.iter_mut().find(|t| t.id == fid)
            {
                if t.tts_pending && !t.tts_spoken {
                    t.tts_spoken = true;
                    Some((
                        t.tts_summary.clone(),
                        t.tts_body.clone(),
                        t.tts_readout.clone(),
                        t.tts_command.clone(),
                    ))
                } else {
                    None
                }
            } else {
                None
            };
            if let Some((s, b, r, c)) = payload {
                eprintln!("[tts-fire] id={} summary={:?}", fid, s);
                Self::speak_tts(&s, &b, r.as_deref(), c);
            }
        }

        // Detect count changes and kick off the position-blend animation.
        let new_count = self.active_toasts.len();
        if new_count != self.stack_target_count {
            eprintln!("[stack-anim] {} -> {}", self.stack_target_count, new_count);
            self.stack_from_count = self.stack_target_count;
            self.stack_target_count = new_count;
            self.stack_anim_start = Instant::now();
        }
    }

    pub fn update(&mut self, message: NotifMessage) {
        match message {
            NotifMessage::DismissToast(id) => {
                let now = Instant::now();
                for t in self.active_toasts.iter_mut() {
                    if t.id == id && t.exit_started_at.is_none() {
                        t.exit_started_at = Some(now);
                    }
                }
            }
            NotifMessage::ClearHistory => {
                self.history.clear();
                self.save_history();
            }
            NotifMessage::DeleteHistory(id) => {
                self.history.retain(|t| t.id != id);
                self.save_history();
            }
            NotifMessage::ToggleHistoryDrawer => {
                self.show_history = !self.show_history;
            }
            NotifMessage::DismissDrawer => {
                self.show_history = false;
            }
        }
    }

    pub fn view_toasts<'a>(&'a self, theme: &'a ShellTheme, settings: &'a ShellSettings, _window_w: f32) -> Element<'a, NotifMessage> {
        let card_w = settings.notif_card_width as f32;
        let card_h = settings.notif_card_height as f32;

        let shape = match settings.notif_shape.to_lowercase().as_str() {
            "hex" | "hexagon" => ShapeType::Hexagon { cut: 14 },
            "slant" => ShapeType::Slant { left: SlantSide::Right, right: SlantSide::Right },
            "flat" => ShapeType::Flat,
            _ => ShapeType::Rounded(10),
        };

        let from_right = settings.notif_corner.contains("right");

        // Render oldest-first for positional stacking, but push in reverse so
        // the oldest card is painted last and therefore sits on top of newer
        // cards when the stack overlap is negative.
        let mut deck = Stack::new();
        let ordered: Vec<(usize, &NotificationToast)> =
            self.active_toasts.iter().enumerate().collect();

        for (i, toast) in ordered.into_iter().rev() {
            let id = toast.id;
            let border_c = hex_to_color(&toast.border_color);

            // Multi-phase entrance animation:
            //   1. slide horizontally in from the requested side
            //   2. pause briefly
            //   3. drop vertically into the resting slot
            let elapsed_ms = toast.created_at.elapsed().as_millis() as f32;
            let slide_ms: f32 = if settings.notif_slide_in_ms == 0 {
                1.0
            } else {
                settings.notif_slide_in_ms.max(50) as f32
            };
            let pause_ms = settings.notif_drop_pause_ms as f32;
            let drop_ms = 250.0_f32;
            // Use configured drop distance. The earlier max(card_h +
            // overlap, configured) made the drop 3x larger than requested,
            // which visually overshot the other toasts and then came back
            // up to rest. The "includes overlap" spacing is in slot_y.
            let drop_dist = settings.notif_drop_distance_px as f32;

            let slide_progress = (elapsed_ms / slide_ms).min(1.0);
            let slide_ease = 1.0 - (1.0 - slide_progress).powi(3);
            let exit_progress = toast.exit_started_at
                .map(|s| (s.elapsed().as_millis() as f32 / 250.0).min(1.0))
                .unwrap_or(0.0);
            let exit_ease = exit_progress * exit_progress;

            // Horizontal position via Row+Spacer(Fixed(x)). For right corner,
            // x starts off-screen (window_w + 60) and interpolates to the
            // resting x (window_w - card_w - margin_x). This is the only way
            // to actually slide FROM outside using iced's layout: padding
            // can't push a widget past its container's edge.
            // For left corner, x is fixed at margin_x (no slide) because
            // iced can't place a widget at negative x.
            // Use the strip width (from settings), not window_w.
            // window_w is the surface's actual size (a corner strip, ~540px),
            // while a full-monitor 1920 would push every x past the surface
            // edge and clip the toast for the entire animation. Compute
            // inline so this block is self-contained regardless of what
            // earlier code defines.
            let strip_w = settings.notif_card_width as f32
                + 2.0 * settings.notif_margin_x as f32
                + 100.0;
            // Expand-from-side animation: iced 0.13 cannot place a widget at a
            // negative x or beyond its parent, so a true slide is impossible.
            // Instead: animate WIDTH from a 4px sliver to full card width,
            // with the leading edge anchored to the side we're "sliding from".
            // Reads as the toast unfolding from off-screen.
            let animated_w = (card_w * slide_ease * (1.0 - exit_ease)).max(4.0);
            let strip_margin = settings.notif_margin_x as f32;
            let x_pos = if from_right {
                (strip_w - strip_margin) - animated_w
            } else {
                strip_margin
            };

            // Phase 3: vertical settle-drop (begins after slide + pause)
            let drop_start = slide_ms + pause_ms;
            let drop_progress = ((elapsed_ms - drop_start) / drop_ms).clamp(0.0, 1.0);
            let drop_ease = 1.0 - (1.0 - drop_progress).powi(3);
            let drop_extra = (1.0 - drop_ease) * (-drop_dist);

            // Stack: oldest at BOTTOM (fully visible, z-front), newer above
            // (behind, only top `peek` px visible). Add drop_dist to all
            // slot_y so the initial drop animation has room to animate
            // from y_pos=0 upward (i.e., the toast falls from top of strip).
            let peek = (settings.notif_stack_overlap as f32).abs().max(4.0);

            // Fixed baseline. Existing toasts occupy fixed slots: i=0
            // is the bottom/front (oldest), higher i sits above/behind.
            // A new toast is placed at i = n-1 (highest), so it drops
            // in ABOVE the existing ones without moving them. Only on
            // removal does the active_toasts array re-index (remaining
            // toasts get i-1), and slot_shift_start animates that
            // downward slide over 200ms.
            let shift_progress = toast.slot_shift_start
                .map(|s| (s.elapsed().as_millis() as f32 / 200.0).min(1.0))
                .unwrap_or(1.0);
            let shift_ease = 1.0 - (1.0 - shift_progress).powi(3);
            let shift_offset = (1.0 - shift_ease) * (-peek);
            // 4*peek gives headroom for up to 5 stacked toasts above the
            // oldest one. Beyond that, clamping at y=0 overlaps.
            let fixed_base = drop_dist + 4.0 * peek;
            let slot_y = fixed_base - (i as f32) * peek + shift_offset;
            let y_pos = (slot_y + drop_extra).max(0.0);

            let card_bg = ShapeBox::new()
                .shape(shape)
                .background(theme.background)
                .border(border_c, theme.border_width)
                .size(Length::Fixed(card_w), Length::Fixed(card_h));

            let badge_bg = if toast.is_urgent { theme.danger } else { border_c };
            let icon_badge = container(
                text(if toast.is_urgent { "!" } else { "i" })
                    .size(13)
                    .bold()
                    .color(Color::BLACK)
            )
            .padding([3, 7])
            .style(move |_t| container::Style {
                background: Some(iced::Background::Color(badge_bg)),
                border: iced::Border { radius: 10.0.into(), ..Default::default() },
                ..Default::default()
            });

            let toast_text_color = hex_to_color(&toast.text_color);
            let avatar: Element<'_, NotifMessage> = if let Some(ref path) = toast.avatar_path {
                iced::widget::image(iced::widget::image::Handle::from_path(path.clone()))
                    .width(Length::Fixed(48.0))
                    .height(Length::Fixed(48.0))
                    .into()
            } else {
                container(text("")).width(Length::Fixed(0.0)).into()
            };
            let content = row![
                icon_badge,
                avatar,
                column![
                    text(&toast.summary).size(settings.notif_summary_size).bold().color(toast_text_color).width(Length::Fill),
                    text(&toast.body).size(settings.notif_body_size).color(toast_text_color).width(Length::Fill),
                ]
                .width(Length::Fill)
                .spacing(2),
                button(text("✕").size(12).color(theme.muted))
                    .on_press(NotifMessage::DismissToast(id))
                    .padding(4)
                    .style(|_t, _s| button::Style { background: None, ..Default::default() }),
            ]
            .spacing(12)
            .align_y(Alignment::Center)
            .padding(14);

            let toast_card = container(Stack::new().push(card_bg.view()).push(content))
                .width(Length::Fixed(card_w))
                .height(Length::Fixed(card_h));

            // Column with right/left align reveals the correct side as width grows.
            let aligned_toast: Element<'_, NotifMessage> = if from_right {
                iced::widget::column![toast_card]
                    .align_x(iced::alignment::Horizontal::Right)
                    .width(Length::Fill)
                    .into()
            } else {
                iced::widget::column![toast_card]
                    .align_x(iced::alignment::Horizontal::Left)
                    .width(Length::Fill)
                    .into()
            };
            let clipped = container(aligned_toast)
                .width(Length::Fixed(animated_w))
                .height(Length::Fixed(card_h))
                .clip(true);
            let toast_layer = container(
                iced::widget::row![
                    container(text("")).width(Length::Fixed(x_pos.max(0.0))),
                    clipped,
                ]
            )
            .width(Length::Fill)
            .padding(Padding {
                top: y_pos,
                right: 0.0,
                bottom: 0.0,
                left: 0.0,
            });

            deck = deck.push(toast_layer);
        }

        deck.into()
    }

    pub fn view_history<'a>(&'a self, theme: &'a ShellTheme) -> Element<'a, NotifMessage> {
        let mut list_col = column![].spacing(8);

        if self.history.is_empty() {
            list_col = list_col.push(
                container(
                    text("No notifications in history.")
                        .size(theme.font_size_overlay)
                        .color(theme.muted)
                )
                .padding(30)
                .center_x(Length::Fill)
            );
        } else {
            for item in &self.history {
                let id = item.id;
                let row_item = row![
                    text("🔔").size(18),
                    column![
                        row![
                            text(&item.summary).size(theme.font_size_overlay).bold().color(theme.text).width(Length::Fill),
                            text(&item.timestamp).size(11).color(theme.muted),
                        ]
                        .align_y(Alignment::Center),
                        text(&item.body).size(theme.font_size_overlay.saturating_sub(3)).color(theme.muted),
                    ]
                    .width(Length::Fill)
                    .spacing(3),
                    button(text("✕").size(13).bold().color(theme.danger))
                        .on_press(NotifMessage::DeleteHistory(id))
                        .padding(6)
                        .style(|_t, _s| button::Style { background: None, ..Default::default() }),
                ]
                .spacing(14)
                .align_y(Alignment::Center)
                .padding(12);

                let card_row = container(row_item)
                    .width(Length::Fill)
                    .style(move |_t| container::Style {
                        background: Some(iced::Background::Color(Color::from_rgb8(18, 20, 26))),
                        border: iced::Border { color: theme.border, width: 1.0, radius: 6.0.into() },
                        ..Default::default()
                    });

                list_col = list_col.push(card_row);
            }
        }

        let card_bg = ShapeBox::new()
            .shape(ShapeType::Hexagon { cut: 18 })
            .background(theme.background)
            .border(theme.border, theme.border_width)
            .size(Length::Fixed(960.0), Length::Fixed(680.0));

        let card_content = container(
            column![
                row![
                    text("📜 NOTIFICATION HISTORY")
                        .size(theme.font_size_overlay + 2)
                        .bold()
                        .color(theme.text),
                    row![].width(Length::Fill),
                    button(text("Clear All").size(11).bold().color(theme.danger))
                        .on_press(NotifMessage::ClearHistory)
                        .padding([4, 12])
                        .style(move |_t, _s| button::Style {
                            background: Some(iced::Background::Color(Color::from_rgb8(30, 15, 15))),
                            border: iced::Border { color: theme.danger, width: 1.0, radius: 4.0.into() },
                            ..Default::default()
                        }),
                ]
                .align_y(Alignment::Center),
                scrollable(list_col).height(Length::Fill),
                text("Click ✕ to delete individual items  •  ESC to dismiss")
                    .size(11)
                    .color(theme.muted)
                    .align_x(Alignment::Center),
            ]
            .spacing(14)
            .padding(24),
        )
        .width(Length::Fixed(960.0))
        .height(Length::Fixed(680.0));

        let card = Stack::new().push(card_bg.view()).push(card_content);

        let backdrop = container(column![]).width(Length::Fill).height(Length::Fill)
            .style(|_t| container::Style {
                background: Some(iced::Background::Color(Color::from_rgba8(0, 0, 0, 0.65))),
                ..Default::default()
            });

        Stack::new()
            .push(backdrop)
            .push(container(card).center_x(Length::Fill).center_y(Length::Fill))
            .into()
    }
}

fn rand_id() -> u32 {
    let mut b = [0u8; 4];
    if let Ok(mut f) = fs::File::open("/dev/urandom") {
        use std::io::Read;
        let _ = f.read_exact(&mut b);
        u32::from_ne_bytes(b)
    } else {
        1000
    }
}

// ── do-not-disturb flag ────────────────────────────────────────────────────
pub fn dnd_flag_path() -> PathBuf {
    let dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(dir).join("moon-beauty-shell-dnd")
}

pub fn dnd_active() -> bool {
    dnd_flag_path().exists()
}

/// Flips the flag; returns the new state (true = notifications paused).
pub fn toggle_dnd() -> bool {
    let p = dnd_flag_path();
    let state = if p.exists() {
        let _ = fs::remove_file(&p);
        eprintln!("[dnd] flag file removed: {:?}", p);
        false
    } else {
        let _ = fs::write(&p, b"1");
        eprintln!("[dnd] flag file created: {:?}", p);
        true
    };
    state
}

pub fn held_notifs_path() -> PathBuf {
    let dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(dir).join("moon-beauty-shell-held-notifs.json")
}

pub fn load_held_notifs() -> Vec<NotificationToast> {
    let p = held_notifs_path();
    if p.exists() {
        if let Ok(content) = fs::read_to_string(&p) {
            if let Ok(list) = serde_json::from_str::<Vec<NotificationToast>>(&content) {
                return list;
            }
        }
    }
    Vec::new()
}

pub fn save_held_notifs(toasts: &[NotificationToast]) {
    let p = held_notifs_path();
    if toasts.is_empty() {
        let _ = fs::remove_file(&p);
    } else if let Ok(json) = serde_json::to_string(toasts) {
        let tmp = p.with_extension("tmp");
        if fs::write(&tmp, json).is_ok() {
            let _ = fs::rename(tmp, p);
        }
    }
}

pub fn held_notifs_count() -> usize {
    load_held_notifs().len()
}

pub fn take_held_notifs() -> Vec<NotificationToast> {
    let list = load_held_notifs();
    let _ = fs::remove_file(held_notifs_path());
    list
}
