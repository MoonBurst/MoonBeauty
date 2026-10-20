use iced::widget::{button, column, container, row, scrollable, text, text_input, Stack};
use iced::{Alignment, Color, Element, Length};
use std::fs;
use std::path::PathBuf;
use tokio::process::Command;

use crate::config::settings::ShellSettings;
use crate::config::theme::ShellTheme;
use crate::style::shape_box::{ShapeBox, ShapeType};

#[derive(Debug, Clone)]
pub struct AppEntry {
    pub name: String,
    pub exec: String,
    pub icon: String,
    pub icon_path: Option<String>,
}

#[derive(Debug, Clone)]
pub enum LauncherMessage {
    SearchChanged(String),
    ExecuteApp(String),
    Dismiss,
}

pub const LAUNCHER_SEARCH_ID: &str = "launcher_search";

pub struct AppLauncher {
    pub search_query: String,
    pub all_apps: Vec<AppEntry>,
    pub filtered_apps: Vec<AppEntry>,
    pub focus_requested: bool,
}

impl AppLauncher {
    pub fn new() -> Self {
        let mut all_apps = vec![
            AppEntry { name: "Settings & Options".into(), exec: "internal:settings".into(), icon: "⚙".into(), icon_path: None },
            AppEntry { name: "Calculator & Converter".into(), exec: "internal:calc".into(), icon: "🧮".into(), icon_path: None },
            AppEntry { name: "Clipboard Manager".into(), exec: "internal:clip".into(), icon: "📋".into(), icon_path: None },
            AppEntry { name: "Quick Notes".into(), exec: "internal:notes".into(), icon: "📝".into(), icon_path: None },
            AppEntry { name: "Password Store".into(), exec: "internal:pass".into(), icon: "🔑".into(), icon_path: None },
            AppEntry { name: "Power & Session".into(), exec: "internal:power".into(), icon: "⚡".into(), icon_path: None },
            AppEntry { name: "Tasks & Todos".into(), exec: "internal:todo".into(), icon: "✅".into(), icon_path: None },
            AppEntry { name: "Reminders & Alarms".into(), exec: "internal:reminders".into(), icon: "⏰".into(), icon_path: None },
            AppEntry { name: "Dictionary Lookup".into(), exec: "internal:dict".into(), icon: "📖".into(), icon_path: None },
            AppEntry { name: "Unicode Glyphs".into(), exec: "internal:unicode".into(), icon: "🔣".into(), icon_path: None },
            AppEntry { name: "Web Search".into(), exec: "internal:web".into(), icon: "🌐".into(), icon_path: None },
            AppEntry { name: "Screenshot (Quickshot)".into(), exec: "internal:quickshot".into(), icon: "📷".into(), icon_path: None },
        ];
        all_apps.extend(Self::scan_desktop_files());

        Self {
            search_query: String::new(),
            filtered_apps: all_apps.clone(),
            all_apps,
            focus_requested: true,
        }
    }

    pub fn scan_desktop_files() -> Vec<AppEntry> {
        let mut apps = Vec::new();
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
        let dirs = [
            PathBuf::from("/usr/share/applications"),
            PathBuf::from("/usr/local/share/applications"),
            home.join(".local/share/applications"),
            PathBuf::from("/run/current-system/sw/share/applications"),
            home.join(".nix-profile/share/applications"),
        ];

        let mut seen = std::collections::HashSet::new();
        let mut total_files = 0usize;
        let mut hidden = 0usize;

        for dir in &dirs {
            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|s| s.to_str()) == Some("desktop") {
                        total_files += 1;
                        if let Ok(content) = fs::read_to_string(&path) {
                            let mut name = None;
                            let mut exec = None;
                            let mut icon_name: Option<String> = None;
                            let mut nodisplay = false;

                            for line in content.lines() {
                                if line.starts_with("Name=") && name.is_none() {
                                    name = Some(line["Name=".len()..].trim().to_string());
                                } else if line.starts_with("Exec=") && exec.is_none() {
                                    let raw_exec = line["Exec=".len()..].trim();
                                    let cleaned = raw_exec
                                        .split('%')
                                        .next()
                                        .unwrap_or(raw_exec)
                                        .trim()
                                        .to_string();
                                    exec = Some(cleaned);
                                } else if line.starts_with("Icon=") && icon_name.is_none() {
                                    icon_name = Some(line["Icon=".len()..].trim().to_string());
                                } else if line.eq_ignore_ascii_case("NoDisplay=true") {
                                    nodisplay = true;
                                }
                            }

                            if nodisplay { hidden += 1; continue; }
                            if let (Some(n), Some(e)) = (name, exec) {
                                let key = format!("{}|{}", n.to_lowercase(), e.to_lowercase());
                                if !seen.contains(&key) {
                                    seen.insert(key);
                                    let icon_path = icon_name
                                        .as_deref()
                                        .and_then(Self::resolve_icon);
                                    apps.push(AppEntry {
                                        name: n,
                                        exec: e,
                                        icon: "🚀".to_string(),
                                        icon_path,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        let with_icons = apps.iter().filter(|a| a.icon_path.is_some()).count();
        eprintln!(
            "[launcher] {} .desktop files scanned ({} hidden), {} apps loaded, {} with icons",
            total_files, hidden, apps.len(), with_icons,
        );
        apps
    }

    fn resolve_icon(icon_name: &str) -> Option<String> {
        use std::path::Path;
        if icon_name.is_empty() { return None; }
        if icon_name.starts_with('/') {
            return if Path::new(icon_name).exists() {
                Some(icon_name.to_string())
            } else { None };
        }
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
        let roots = [
            PathBuf::from("/run/current-system/sw/share/icons/hicolor"),
            PathBuf::from("/usr/share/icons/hicolor"),
            PathBuf::from("/usr/local/share/icons/hicolor"),
            home.join(".local/share/icons/hicolor"),
            home.join(".nix-profile/share/icons/hicolor"),
        ];
        let sizes = ["48x48", "64x64", "128x128", "256x256", "512x512", "scalable", "symbolic"];
        let exts = ["png", "svg", "xpm"];
        for root in &roots {
            for size in &sizes {
                for ext in &exts {
                    let p = root.join(size).join("apps")
                        .join(format!("{}.{}", icon_name, ext));
                    if p.exists() {
                        return Some(p.to_string_lossy().into_owned());
                    }
                }
            }
        }
        for pix in ["/run/current-system/sw/share/pixmaps", "/usr/share/pixmaps"] {
            for ext in &exts {
                let p = PathBuf::from(pix).join(format!("{}.{}", icon_name, ext));
                if p.exists() {
                    return Some(p.to_string_lossy().into_owned());
                }
            }
        }
        None
    }

    pub fn update(&mut self, message: LauncherMessage) -> iced::Task<LauncherMessage> {
        if let LauncherMessage::SearchChanged(q) = message {
            eprintln!("[launcher-input] {:?}", q);
            self.search_query = q.clone();
            let lower = q.trim().to_lowercase();
            if lower.is_empty() {
                self.filtered_apps = self.all_apps.clone();
            } else {
                let mut filtered: Vec<AppEntry> = self
                    .all_apps
                    .iter()
                    .filter(|a| {
                        a.name.to_lowercase().contains(&lower)
                            || a.exec.to_lowercase().contains(&lower)
                    })
                    .cloned()
                    .collect();
                // Routing aliases ("cfg", "calc", "clip", ...) are not
                // substrings of any app name; without this the query yields
                // no results and Enter tries to exec the keyword as a shell
                // command. Prepend a synthetic entry whose exec is the
                // internal: route.
                if let Some(alias) = Self::routing_alias(&lower) {
                    if !filtered.iter().any(|a| a.exec == alias.exec) {
                        filtered.insert(0, alias);
                    }
                }
                self.filtered_apps = filtered;
            }
        }
        iced::Task::none()
    }

    fn routing_alias(query: &str) -> Option<AppEntry> {
        let (name, exec) = match query {
            "cfg" | "set" | "settings" => ("Settings & Options", "internal:settings"),
            "calc" | "calculator" => ("Calculator", "internal:calc"),
            "clip" | "clipboard" => ("Clipboard Manager", "internal:clip"),
            "todo" | "td" => ("Tasks & Todos", "internal:todo"),
            "note" | "notes" => ("Quick Notes", "internal:notes"),
            "pass" | "password" => ("Password Store", "internal:pass"),
            "power" | "pwr" => ("Power & Session", "internal:power"),
            "remind" | "reminder" | "reminders" => ("Reminders & Alarms", "internal:reminders"),
            "dict" | "def" | "dictionary" => ("Dictionary", "internal:dict"),
            "unicode" => ("Unicode", "internal:unicode"),
            "email" | "mail" | "em" => ("Email", "internal:email"),
            "rng" | "dice" | "roll" => ("RNG / Dice", "internal:rng"),
            "web" | "search" => ("Web Search", "internal:web"),
            "amogus" | "amongus" | "sus" => ("Among Us", "internal:amogus"),
            _ => return None,
        };
        Some(AppEntry { name: name.into(), exec: exec.into(), icon: "⚙".into(), icon_path: None })
    }

    pub fn focus_search(&mut self) -> iced::Task<LauncherMessage> {
        iced::widget::text_input::focus(LAUNCHER_SEARCH_ID)
    }

    pub async fn launch(exec: &str) {
        if exec.is_empty() {
            return;
        }
        let _ = Command::new("systemd-run")
            .args(["--user", "--collect", "--quiet", "--slice=app.slice", "sh", "-c", exec])
            .spawn();
    }

    pub fn view<'a>(&'a self, theme: &ShellTheme, settings: &ShellSettings) -> Element<'a, LauncherMessage> {
        let (win_w, win_h) = settings.get_window_size("launcher", 840, 650);

        let input = text_input("Search applications or settings...", &self.search_query)
            .id(LAUNCHER_SEARCH_ID)
            .on_input(LauncherMessage::SearchChanged)
            .on_submit(
                if let Some(first) = self.filtered_apps.first() {
                    LauncherMessage::ExecuteApp(first.exec.clone())
                } else {
                    LauncherMessage::ExecuteApp(self.search_query.clone())
                },
            )
            .padding(14)
            .size(theme.font_size_overlay);

        let mut list_col = column![].spacing(6);
        for app in self.filtered_apps.iter().take(40) {
            let exec_cmd = app.exec.clone();
            let icon_str = if app.exec.starts_with("internal:") { "⚙" } else { "🚀" };
            let icon_elem: Element<'_, LauncherMessage> = if let Some(ref path) = app.icon_path {
                iced::widget::image(iced::widget::image::Handle::from_path(path.clone()))
                    .width(Length::Fixed(24.0))
                    .height(Length::Fixed(24.0))
                    .into()
            } else {
                text(icon_str).size(16).into()
            };
            let row_item = button(
                row![
                    icon_elem,
                    column![
                        text(&app.name)
                            .size(theme.font_size_overlay)
                            .color(theme.text),
                        text(&app.exec)
                            .size(theme.font_size_normal.saturating_sub(2))
                            .color(theme.muted),
                    ]
                    .spacing(2),
                ]
                .spacing(12)
                .align_y(Alignment::Center),
            )
            .on_press(LauncherMessage::ExecuteApp(exec_cmd))
            .padding([8, 14])
            .width(Length::Fill)
            .style(|_t, _s| button::Style {
                background: None,
                ..Default::default()
            });

            list_col = list_col.push(row_item);
        }

        let card_bg = ShapeBox::new()
            .shape(ShapeType::Rounded(12))
            .background(theme.background)
            .border(theme.border, theme.border_width)
            .size(Length::Fixed(win_w), Length::Fixed(win_h));

        let card_content = container(
            column![
                input,
                scrollable(list_col).height(Length::Fill),
                text("ESC to dismiss  •  Enter to open")
                    .size(11)
                    .color(theme.muted)
                    .align_x(Alignment::Center),
            ]
            .spacing(12)
            .padding(16),
        )
        .width(Length::Fixed(win_w))
        .height(Length::Fixed(win_h));

        let card = Stack::new().push(card_bg.view()).push(card_content);

        let backdrop = button(container(column![]).width(Length::Fill).height(Length::Fill))
            .on_press(LauncherMessage::Dismiss)
            .style(|_t, _s| button::Style {
                background: Some(iced::Background::Color(Color::from_rgba8(0, 0, 0, 0.65))),
                ..Default::default()
            });

        Stack::new()
            .push(backdrop)
            .push(container(card).center_x(Length::Fill).center_y(Length::Fill))
            .into()
    }
}
