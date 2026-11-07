#![allow(clippy::collapsible_match)]
use super::*;

mod annotator;
mod clipboard;
mod dismiss;
mod email;
mod email_sync;
mod hardware;
mod ipc;
mod launcher;
mod lockscreen;
mod music;
mod notifications;
mod overlays;
mod reminders;
mod settings;
mod top_bar;
mod tray;

impl MoonBeautyShellApp {
    pub fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            // Message::WindowResized removed (problem 07)
            Message::ResizeNotifSurface(w, h) => {
                // Intercepted by `try_into`; never reached. Arm kept only
                // to keep the match exhaustive.
                let _ = (w, h);
            }
            Message::IdleTick => return self.handle_idle_tick(),

            Message::PollNotifs => return self.handle_poll_notifs(),
            Message::TickClock => return self.handle_tick_clock(),

            Message::TickToasts => return self.handle_tick_toasts(),
            Message::ComposeTabPressed(shift) => {
                if self.email.is_composing {
                    if !self.email.get_autocomplete_matches().is_empty() {
                        self.email.update(EmailMessage::ConfirmAutocomplete);
                        return Command::none();
                    }
                    // Iced 0.14 only has focus_next, so Shift+Tab will just go forward.
                    let _ = shift;
                    return iced::widget::operation::focus_next();
                }
            }
            Message::TickEmailSync => return self.handle_tick_email_sync(),

            Message::EmailSyncedWithDiff(old, new) => {
                return self.handle_email_synced_with_diff(old, new)
            }

            Message::EmailSynced => return self.handle_email_synced(),

            Message::TickWeather => return self.handle_tick_weather(),

            Message::WeatherLoaded(w) => return self.handle_weather_loaded(w),

            Message::TickHardware => return self.handle_tick_hardware(),

            Message::MetricsCollected(cpu, ram, gpu, audio, mic, mpris, net) => {
                return self.handle_metrics_collected(cpu, ram, gpu, audio, mic, mpris, net)
            }

            Message::IpcReceived(cmd) => return self.handle_ipc(cmd),
            Message::TopBar(msg) => return self.handle_top_bar(msg),
            Message::Launcher(msg) => return self.handle_launcher(msg),
            Message::Calc(msg) => return self.handle_calc(msg),
            Message::Clipboard(msg) => return self.handle_clipboard(msg),
            Message::Notes(msg) => return self.handle_notes(msg),
            Message::Pass(msg) => return self.handle_pass(msg),
            Message::Power(msg) => return self.handle_power(msg),
            Message::Todo(msg) => return self.handle_todo(msg),
            Message::Reminders(msg) => return self.handle_reminders(msg),
            Message::Dict(msg) => return self.handle_dict(msg),
            Message::Unicode(msg) => return self.handle_unicode(msg),
            Message::Web(msg) => return self.handle_web(msg),
            Message::Annotator(msg) => return self.handle_annotator(msg),

            Message::Amogus(msg) => return self.handle_amogus(msg),
            Message::Rng(msg) => return self.handle_rng(msg),
            Message::Email(msg) => return self.handle_email(msg),
            Message::Notif(msg) => return self.handle_notif(msg),
            Message::Settings(msg) => return self.handle_settings(msg),
            Message::Lock(msg) => return self.handle_lock(msg),
            Message::TrayWatchdogTick => return self.handle_tray_watchdog(),
            Message::TrayAnimTick => return self.handle_tray_anim(),
            Message::FetchTrayItems => return self.handle_fetch_tray_items(),
            Message::TrayItemsLoaded(items) => return self.handle_tray_items_loaded(items),

            Message::DismissActiveOverlay => return self.handle_dismiss_active_overlay(),

            Message::MusicCycleDone(new_state) => return self.handle_music_cycle_done(new_state),
            Message::Noop => {}
            Message::SetToastInputRegion(_) => {
                // Intercepted by `try_into` — never reaches update.
            }
            Message::CloseShell => {
                std::process::exit(0);
            }
        }
        Command::none()
    }
}
