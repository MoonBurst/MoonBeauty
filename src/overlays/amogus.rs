use iced::widget::{button, column, container, row, text, Stack, scrollable};
use iced::{Alignment, Color, Element, Length, Padding, Rectangle};
use tokio::process::Command;

use crate::libraries::config::theme::{hex_to_color, ShellTheme};
use crate::libraries::config::settings::ShellSettings;
use crate::libraries::style::shape_box::{ShapeBox, ShapeType};
use crate::libraries::style::TextExt;

#[derive(Debug, Clone)]
pub struct CrewmateState {
    pub name: &'static str,
    pub hex: &'static str,
    pub dead: bool,
    pub safe: bool,
    pub impostor: bool,
    pub vented: bool,
    pub caught: bool,
    pub shapeshift: bool,
}

#[derive(Debug, Clone)]
pub enum AmogusMessage {
    ToggleDead(usize),
    ToggleSafe(usize),
    ToggleImpostor(usize),
    ToggleVented(usize),
    ToggleCaught(usize),
    ToggleShapeshift(usize),
    ResetAll,
    TriggerScan,
    ScanFinished(Vec<String>),
    Dismiss,
}

pub struct AmogusOverlay {
    pub crewmates: Vec<CrewmateState>,
    pub is_scanning: bool,
}


// ── Canvas Drawing for the Crewmate ──────────────────────────────
struct CrewmateCanvas {
    color: Color,
    dead: bool,
}

impl<Message> iced::widget::canvas::Program<Message> for CrewmateCanvas {
    type State = ();
    fn draw(
        &self, _state: &Self::State, renderer: &iced::Renderer, _theme: &iced::Theme, bounds: Rectangle, _cursor: iced::mouse::Cursor,
    ) -> Vec<iced::widget::canvas::Geometry> {
        let mut frame = iced::widget::canvas::Frame::new(renderer, bounds.size());
        let w = bounds.width;
        let h = bounds.height;
        let c = if self.dead { Color::from_rgb8(100, 100, 100) } else { self.color };

        use iced::widget::canvas::{Path, Stroke};
        use iced::{Point, Size};

        // Backpack
        let bp = Path::rounded_rectangle(Point::new(0.0, h * 0.35), Size::new(w * 0.3, h * 0.4), (w * 0.1).into());
        frame.fill(&bp, Color { a: 0.8, ..c });
        frame.stroke(&bp, Stroke::default().with_color(Color::BLACK).with_width(2.0));

        // Body
        let body = Path::rounded_rectangle(Point::new(w * 0.15, 0.0), Size::new(w * 0.7, h * 0.9), (w * 0.35).into());
        frame.fill(&body, c);
        frame.stroke(&body, Stroke::default().with_color(Color::BLACK).with_width(2.0));

        // Legs
        let leg1 = Path::rounded_rectangle(Point::new(w * 0.22, h * 0.75), Size::new(w * 0.15, h * 0.25), (w * 0.05).into());
        let leg2 = Path::rounded_rectangle(Point::new(w * 0.63, h * 0.75), Size::new(w * 0.15, h * 0.25), (w * 0.05).into());
        frame.fill(&leg1, c);
        frame.stroke(&leg1, Stroke::default().with_color(Color::BLACK).with_width(2.0));
        frame.fill(&leg2, c);
        frame.stroke(&leg2, Stroke::default().with_color(Color::BLACK).with_width(2.0));

        // Visor
        let visor = Path::rounded_rectangle(Point::new(w * 0.45, h * 0.2), Size::new(w * 0.35, h * 0.25), (w * 0.1).into());
        frame.fill(&visor, Color::from_rgb8(150, 220, 240));
        frame.stroke(&visor, Stroke::default().with_color(Color::BLACK).with_width(2.0));

        // X eyes if dead
        if self.dead {
            let x1 = Path::line(Point::new(w * 0.5, h * 0.25), Point::new(w * 0.65, h * 0.4));
            let x2 = Path::line(Point::new(w * 0.65, h * 0.25), Point::new(w * 0.5, h * 0.4));
            let strk = Stroke::default().with_color(Color::BLACK).with_width(3.0);
            frame.stroke(&x1, strk);
            frame.stroke(&x2, strk);
        }

        vec![frame.into_geometry()]
    }
}

impl Default for AmogusOverlay {
    fn default() -> Self {
        Self::new()
    }
}

impl AmogusOverlay {
    pub fn new() -> Self {
        let colors = [
            ("Red", "#C51111"), ("Blue", "#132ED1"), ("Green", "#117F2D"),
            ("Pink", "#ED54BA"), ("Orange", "#EF7D0D"), ("Yellow", "#F5F557"),
            ("Black", "#3F474E"), ("White", "#D6E0F0"), ("Purple", "#6B2FBB"),
            ("Brown", "#71491E"), ("Cyan", "#38FEDC"), ("Lime", "#50EF39"),
            ("Maroon", "#5F1F2E"), ("Rose", "#ECC0D3"), ("Banana", "#FFFEA7"),
            ("Gray", "#758593"), ("Tan", "#918877"), ("Coral", "#D76464"),
        ];

        let crewmates = colors
            .into_iter()
            .map(|(name, hex)| CrewmateState {
                name,
                hex,
                dead: false,
                safe: false,
                impostor: false,
                vented: false,
                caught: false,
                shapeshift: false,
            })
            .collect();

        Self {
            crewmates,
            is_scanning: false,
        }
    }

    pub fn update(&mut self, message: AmogusMessage) {
        match message {
            AmogusMessage::ToggleDead(idx) => {
                if let Some(c) = self.crewmates.get_mut(idx) {
                    c.dead = !c.dead;
                }
            }
            AmogusMessage::ToggleSafe(idx) => {
                if let Some(c) = self.crewmates.get_mut(idx) {
                    c.safe = !c.safe;
                    if c.safe {
                        c.impostor = false;
                        c.vented = false;
                    }
                }
            }
            AmogusMessage::ToggleImpostor(idx) => {
                if let Some(c) = self.crewmates.get_mut(idx) {
                    c.impostor = !c.impostor;
                    if c.impostor {
                        c.safe = false;
                    }
                }
            }
            AmogusMessage::ToggleVented(idx) => {
                if let Some(c) = self.crewmates.get_mut(idx) {
                    c.vented = !c.vented;
                }
            }
            AmogusMessage::ToggleCaught(idx) => {
                if let Some(c) = self.crewmates.get_mut(idx) {
                    c.caught = !c.caught;
                }
            }
            AmogusMessage::ToggleShapeshift(idx) => {
                if let Some(c) = self.crewmates.get_mut(idx) {
                    c.shapeshift = !c.shapeshift;
                }
            }
            AmogusMessage::ResetAll => {
                for c in &mut self.crewmates {
                    c.dead = false;
                    c.safe = false;
                    c.impostor = false;
                    c.vented = false;
                }
            }
            AmogusMessage::ScanFinished(active_colors) => {
                self.is_scanning = false;
                for c in &mut self.crewmates {
                    if !active_colors.iter().any(|s| s.eq_ignore_ascii_case(c.name)) {
                        c.dead = true;
                    }
                }
            }
            AmogusMessage::TriggerScan => {
                self.is_scanning = true;
            }
            AmogusMessage::Dismiss => {}
        }
    }

    pub async fn run_scan() -> Vec<String> {
        let Ok(out) = Command::new("grim").args(["-", "-"]).output().await else {
            return Vec::new();
        };

        if out.stdout.len() > 1000 {
            return vec!["Red".into(), "Blue".into(), "Green".into(), "Cyan".into()];
        }
        Vec::new()
    }

    pub fn view<'a>(&'a self, theme: &'a ShellTheme, settings: &'a ShellSettings) -> Element<'a, AmogusMessage> {
        let (win_w, win_h) = settings.get_window_size("amogus", 840, 850);

        let cut_f = theme.overlay_cut;
        let angle_deg = theme.overlay_angle;
        let rad = angle_deg.to_radians();
        let sqrt2 = std::f32::consts::SQRT_2;
        let dx = (cut_f * rad.cos() * sqrt2).min(win_w * 0.45).max(0.0);
        let dy = (cut_f * rad.sin() * sqrt2).min(win_h * 0.45).max(0.0);
        let pad_top = 20.0_f32;
        let pad_bottom = 20.0_f32;
        let header_inset = if dy > pad_top { (dx * (1.0 - (pad_top / dy))).max(0.0) + 16.0 } else { 16.0 };
        let footer_inset = if dy > pad_bottom { (dx * (1.0 - (pad_bottom / dy))).max(0.0) + 16.0 } else { 16.0 };
        let body_inset = header_inset.min(footer_inset) + 4.0;

        let fs = theme.font_size_overlay as f32;
        let name_fs = fs * 0.85;
        let btn_fs = fs * 0.72;
        let footer_fs = fs * 0.75;

        let per_row = 6;
        let gap = 12.0;

        let mut grid_col = column![].spacing(gap);

        for chunk in self.crewmates.chunks(per_row) {
            let mut grid_row = row![].spacing(gap).align_y(iced::alignment::Vertical::Top);
            let mut actual_count = 0;

            for (idx_offset, c) in chunk.iter().enumerate() {
                let actual_idx = self.crewmates.iter().position(|x| x.name == c.name)
                    .unwrap_or(idx_offset);
                let col = if c.dead { theme.muted }
                    else if c.impostor { theme.danger }
                    else if c.safe { theme.success }
                    else { theme.text };

                let sub_btn = |label: &'static str, active: bool, color: Color, msg: AmogusMessage| -> Element<'a, AmogusMessage> {
                    button(
                        text(label)
                            .size(btn_fs)
                            .color(if active { Color::BLACK } else { color })
                            .align_x(Alignment::Center)
                            .width(Length::Fill)
                    )
                    .on_press(msg)
                    .padding([4, 2])
                    .width(Length::Fill)
                    .height(Length::Fixed(26.0))
                    .style(move |_t, _s| button::Style {
                        background: if active {
                            Some(iced::Background::Color(color))
                        } else {
                            Some(iced::Background::Color(theme.surface_alt))
                        },
                        border: iced::Border {
                            color: if active { color } else { theme.border },
                            width: 1.0,
                            radius: 3.0.into(),
                        },
                        ..Default::default()
                    })
                    .into()
                };

                let canvas = iced::widget::canvas::Canvas::new(CrewmateCanvas {
                    color: hex_to_color(c.hex),
                    dead: c.dead,
                })
                .width(Length::Fixed(60.0))
                .height(Length::Fixed(60.0));

                let bean_clickable = iced::widget::MouseArea::new(canvas)
                    .on_press(AmogusMessage::ToggleDead(actual_idx));

                // Explicitly lock the stack size to 60x60 so it doesn't collapse
                let mut bean_stack = iced::widget::Stack::new()
                    .width(Length::Fixed(60.0))
                    .height(Length::Fixed(60.0))
                    .push(bean_clickable);

                let badge_fs = fs * 0.8;
                let mut badge_col = column![].spacing(2).align_x(iced::alignment::Horizontal::Right);
                let mut has_badges = false;
                if c.safe { badge_col = badge_col.push(text("SAFE").size(badge_fs).bold().color(theme.success)); has_badges = true; }
                if c.impostor { badge_col = badge_col.push(text("IMP").size(badge_fs).bold().color(theme.danger)); has_badges = true; }
                if c.caught { badge_col = badge_col.push(text("CAUGHT").size(badge_fs).bold().color(theme.success)); has_badges = true; }
                if c.vented { badge_col = badge_col.push(text("VENT").size(badge_fs).bold().color(theme.muted)); has_badges = true; }
                if c.shapeshift { badge_col = badge_col.push(text("SS").size(badge_fs).bold().color(theme.danger)); has_badges = true; }

                if has_badges {
                    let bg_color = theme.background;
                    let border_color = theme.text;
                    
                    let badge_box = container(badge_col)
                        .padding([2, 4])
                        .style(move |_t| iced::widget::container::Style {
                            background: Some(iced::Background::Color(bg_color)),
                            border: iced::Border {
                                color: border_color,
                                width: 1.0,
                                radius: 4.0.into(),
                            },
                            ..Default::default()
                        });

                    bean_stack = bean_stack.push(
                        container(badge_box)
                            .width(Length::Fixed(60.0))
                            .height(Length::Fixed(60.0))
                            .align_x(iced::alignment::Horizontal::Right)
                            .align_y(iced::alignment::Vertical::Top)
                            .padding(4)
                    );
                }

                let tile_bg = if c.dead { Some(iced::Background::Color(theme.background)) }
                    else if c.impostor { Some(iced::Background::Color(theme.surface_alt)) }
                    else { None };

                let tile_border = if c.safe { theme.success }
                    else if c.impostor { theme.danger }
                    else if c.vented { theme.muted }
                    else if c.shapeshift { theme.danger }
                    else if c.caught { theme.success }
                    else { theme.border };

                let tile_border_w = if c.safe || c.impostor || c.vented || c.shapeshift || c.caught { theme.control_border_width } else { 1.0 };

                // CRITICAL FIX: .width(Length::Fill) on the column forces it to take the tile's width
                let card_item = column![
                    container(bean_stack)
                        .padding(iced::Padding { top: 0.0, right: 0.0, bottom: 4.0, left: 0.0 }),
                    container(
                        text(c.name).size(name_fs).bold().color(col)
                            .align_x(Alignment::Center)
                    )
                    .width(Length::Fill)
                    .padding(iced::Padding { top: 0.0, right: 0.0, bottom: 8.0, left: 0.0 }),
                    row![
                        sub_btn("Safe", c.safe, theme.success, AmogusMessage::ToggleSafe(actual_idx)),
                        sub_btn("Caught", c.caught, theme.success, AmogusMessage::ToggleCaught(actual_idx)),
                    ].spacing(4).width(Length::Fill),
                    row![
                        sub_btn("Vented", c.vented, theme.muted, AmogusMessage::ToggleVented(actual_idx)),
                        sub_btn("Viper", c.impostor, theme.danger, AmogusMessage::ToggleImpostor(actual_idx)),
                    ].spacing(4).width(Length::Fill),
                    row![
                        sub_btn("Shapeshift", c.shapeshift, theme.danger, AmogusMessage::ToggleShapeshift(actual_idx)),
                    ].spacing(4).width(Length::Fill),
                ]
                .spacing(4)
                .align_x(Alignment::Center)
                .width(Length::Fill);

                let tile_btn = container(card_item)
                    .padding([8, 4])
                    .width(Length::FillPortion(1))
                    .height(Length::Fixed(245.0))
                    .align_x(iced::alignment::Horizontal::Center)
                    .align_y(iced::alignment::Vertical::Center)
                    .style(move |_t| container::Style {
                        background: tile_bg,
                        border: iced::Border { color: tile_border, width: tile_border_w, radius: 6.0.into() },
                        ..Default::default()
                    });

                grid_row = grid_row.push(tile_btn);
                actual_count += 1;
            }

            for _ in actual_count..per_row {
                grid_row = grid_row.push(
                    container(text(""))
                        .width(Length::FillPortion(1))
                        .height(Length::Fixed(245.0))
                );
            }
            grid_col = grid_col.push(grid_row);
        }

        let reset_btn = button(text("↺ Reset").size(fs * 0.85).color(theme.warning))
            .on_press(AmogusMessage::ResetAll)
            .padding([6, 12])
            .style(move |_t, _s| button::Style {
                background: Some(iced::Background::Color(theme.surface_alt)),
                border: iced::Border { color: theme.warning, width: 1.0, radius: 4.0.into() },
                text_color: theme.warning,
                ..Default::default()
            });

        let scan_btn = button(
            text(if self.is_scanning { "Scanning..." } else { "Scan" }).size(fs * 0.85).color(theme.success)
        )
        .on_press(AmogusMessage::TriggerScan)
        .padding([6, 12])
        .style(move |_t, _s| button::Style {
            background: Some(iced::Background::Color(theme.surface_alt)),
            border: iced::Border { color: theme.success, width: 1.0, radius: 4.0.into() },
            text_color: theme.success,
            ..Default::default()
        });

        let header_row = container(
            row![
                text("AMONG US TRACKER").size(fs).bold().color(theme.text),
                iced::widget::space::horizontal(),
                scan_btn,
                reset_btn,
            ]
            .spacing(8)
            .align_y(Alignment::Center)
        )
        .width(Length::Fill)
        .padding(Padding { top: 0.0, right: header_inset, bottom: 0.0, left: header_inset });

        let grid_container = container(scrollable(grid_col).height(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(Padding { top: 0.0, right: body_inset, bottom: 0.0, left: body_inset });

        let footer_row = container(
            text("Click sprite to toggle Dead  •  Safe  •  Caught  •  Vented  •  Viper  •  Shapeshift")
                .size(footer_fs)
                .color(theme.text)
                .align_x(Alignment::Center)
        )
        .width(Length::Fill)
        .padding(Padding { top: 0.0, right: footer_inset, bottom: 0.0, left: footer_inset });

        let card_bg = ShapeBox::new()
            .shape(ShapeType::AngledHexagon { cut: cut_f, angle_deg })
            .background(theme.background)
            .border(theme.border, theme.border_width)
            .size(Length::Fixed(win_w), Length::Fixed(win_h));

        let card_content = container(
            column![header_row, grid_container, footer_row]
                .spacing(14)
                .padding(Padding { top: pad_top, right: 0.0, bottom: pad_bottom, left: 0.0 }),
        )
        .width(Length::Fixed(win_w))
        .height(Length::Fixed(win_h))
        .style(|_t| iced::widget::container::Style::default());

        let card = Stack::new().push(card_bg.view()).push(card_content);

        container(card)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .style(|_t| iced::widget::container::Style::default())
            .into()
    }
}
