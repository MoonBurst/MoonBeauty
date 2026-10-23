use iced::widget::{button, column, container, text, Stack};
use iced::{Alignment, Color, Element, Length, Task, Theme};
use iced_layershell::reexport::{Anchor, KeyboardInteractivity, Layer};
use iced_layershell::settings::{LayerShellSettings, Settings};
use iced_layershell::Application;

use crate::backend::tray_watcher::TrayMenuItem;
use crate::config::theme::ShellTheme;
use crate::config::ShellSettings;
use crate::style::shape_box::{ShapeBox, ShapeType, SlantSide};

#[derive(Debug, Clone)]
pub enum Message {
    Dismiss,
    ClickItem(i32),
}

impl TryInto<iced_layershell::actions::LayershellCustomActionsWithInfo<()>> for Message {
    type Error = Message;
    fn try_into(self) -> Result<iced_layershell::actions::LayershellCustomActionsWithInfo<()>, Self::Error> {
        Err(self)
    }
}

pub struct App {
    items: Vec<TrayMenuItem>,
    bus: String,
    menu_path: String,
    width: u32,
    height: u32,
}

impl Application for App {
    type Executor = iced::executor::Default;
    type Message = Message;
    type Theme = Theme;
    type Flags = (Vec<TrayMenuItem>, String, String, u32, u32);

    fn new(flags: Self::Flags) -> (Self, Task<Message>) {
        let (items, bus, menu_path, width, height) = flags;
        (Self { items, bus, menu_path, width, height }, Task::none())
    }

    fn namespace(&self) -> String { "mbs-tray-context".into() }
    fn style(&self, _t: &Self::Theme) -> iced_layershell::Appearance {
        iced_layershell::Appearance { background_color: Color::TRANSPARENT, text_color: Color::WHITE }
    }

    fn update(&mut self, m: Message) -> Task<Message> {
        match m {
            Message::Dismiss => { std::process::exit(0); }
            Message::ClickItem(id) => {
                let bus = self.bus.clone();
                let menu_path = self.menu_path.clone();
                std::thread::spawn(move || {
                    if let Ok(rt) = tokio::runtime::Runtime::new() {
                        rt.block_on(async {
                            let _ = crate::backend::tray_watcher::trigger_menu_item(&bus, &menu_path, id).await;
                        });
                    }
                });
                std::process::exit(0);
            }
        }
    }

    fn subscription(&self) -> iced::Subscription<Message> {
        iced::keyboard::on_key_press(|k, _| {
            if k == iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape) { Some(Message::Dismiss) } else { None }
        })
    }

    fn view(&self) -> Element<'_, Message> {
        let settings = ShellSettings::load();
        let theme = ShellTheme::from_settings(&settings);
        let fs = settings.tooltip_font_size;
        
        let mut col = column![].spacing(4).padding(16);
        
        for item in &self.items {
            if !item.visible { continue; }
            
            if item.label == "-" {
                let sep = container(text(""))
                    .width(Length::Fill)
                    .height(Length::Fixed(1.0))
                    .style(move |_t| container::Style {
                        background: Some(iced::Background::Color(theme.surface_alt)),
                        ..Default::default()
                    });
                col = col.push(container(sep).padding([4, 0]));
                continue;
            }
            
            let fg = if item.enabled { theme.text } else { theme.muted };
            
            let btn = button(
                container(text(&item.label).size(fs).color(fg))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_y(iced::alignment::Vertical::Center)
                    .align_x(iced::alignment::Horizontal::Left)
                    .padding([0, 12])
            )
            .on_press(Message::ClickItem(item.id))
            .padding(0)
            .width(Length::Fill)
            .height(Length::Fixed(32.0))
            .style(move |_t, status| {
                let bg = match status {
                    button::Status::Hovered | button::Status::Pressed => Some(iced::Background::Color(theme.surface_alt)),
                    _ => None,
                };
                button::Style { background: bg, ..Default::default() }
            });
            
            col = col.push(btn);
        }

        let bg_shape = ShapeBox::new()
            .shape(ShapeType::Hexagon { cut: 16 })
            .background(theme.background)
            .border(theme.text, 1.5)
            .size(Length::Fill, Length::Fill);

        let content = Stack::new().push(bg_shape.view()).push(col);
        container(content).width(Length::Fill).height(Length::Fill).into()
    }
}

pub fn run(items: Vec<TrayMenuItem>, bus: String, menu_path: String, width: u32, height: u32) -> Result<(), iced_layershell::Error> {
    let s = ShellSettings::load();
    let bar_h = s.bar_height as i32;
    let layer = LayerShellSettings {
        size: Some((width, height)),
        exclusive_zone: -1,
        anchor: Anchor::Top | Anchor::Right,
        layer: Layer::Overlay,
        keyboard_interactivity: KeyboardInteractivity::Exclusive,
        binded_output_name: std::env::var("MBS_OUTPUT").ok(),
        margin: (bar_h + 8, 16, 0, 0),
        ..Default::default()
    };
    App::run(Settings { flags: (items, bus, menu_path, width, height), layer_settings: layer, ..Default::default() })
}
