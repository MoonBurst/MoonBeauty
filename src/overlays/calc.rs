use iced::widget::{column, container, text, text_input, Stack};
use iced::{Alignment, Element, Length, Padding};
use tokio::process::Command;

use crate::libraries::config::settings::ShellSettings;
use crate::libraries::config::theme::ShellTheme;
use crate::libraries::style::shape_box::{ShapeBox, ShapeType};

#[derive(Debug, Clone)]
pub enum CalcMessage {
    InputChanged(String),
    CopyResult(String),
    Dismiss,
    Refocus,
}

pub const CALC_INPUT_ID: &str = "calc_input";

pub struct CalculatorOverlay {
    pub input_text: String,
    pub result_text: String,
}

impl Default for CalculatorOverlay {
    fn default() -> Self {
        Self::new()
    }
}

impl CalculatorOverlay {
    pub fn new() -> Self {
        Self {
            input_text: String::new(),
            result_text: String::new(),
        }
    }

    pub fn new_with_input(expr: &str) -> Self {
        let expr = expr.to_string();
        let result = Self::evaluate(&expr);
        Self {
            input_text: expr,
            result_text: result,
        }
    }

    pub fn update(&mut self, message: CalcMessage) {
        if let CalcMessage::InputChanged(val) = message {
            self.input_text = val.clone();
            self.result_text = Self::evaluate(&val);
        }
    }

    pub fn evaluate(expr: &str) -> String {
        let clean = expr.trim().trim_start_matches('=').trim();
        if clean.is_empty() {
            return String::new();
        }

        if let Some(res) = Self::eval_units(clean) {
            return res;
        }

        match Self::eval_math(clean) {
            Ok(v) => format!("{:.6}", v)
                .trim_end_matches('0')
                .trim_end_matches('.')
                .to_string(),
            Err(_) => "Syntax Error".to_string(),
        }
    }

    fn eval_math(expr: &str) -> Result<f64, ()> {
        let mut tokens = Vec::new();
        let mut num_buf = String::new();

        for c in expr.chars() {
            if c.is_ascii_digit() || c == '.' {
                num_buf.push(c);
            } else {
                if !num_buf.is_empty() {
                    tokens.push(num_buf.clone());
                    num_buf.clear();
                }
                if !c.is_whitespace() {
                    tokens.push(c.to_string());
                }
            }
        }
        if !num_buf.is_empty() {
            tokens.push(num_buf);
        }

        let mut it = tokens.iter().peekable();
        Self::parse_expr(&mut it)
    }

    fn parse_expr<'a>(
        it: &mut std::iter::Peekable<std::slice::Iter<'a, String>>,
    ) -> Result<f64, ()> {
        let mut left = Self::parse_term(it)?;
        while let Some(&op) = it.peek() {
            if op == "+" {
                it.next();
                left += Self::parse_term(it)?;
            } else if op == "-" {
                it.next();
                left -= Self::parse_term(it)?;
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn parse_term<'a>(
        it: &mut std::iter::Peekable<std::slice::Iter<'a, String>>,
    ) -> Result<f64, ()> {
        let mut left = Self::parse_factor(it)?;
        while let Some(&op) = it.peek() {
            if op == "*" {
                it.next();
                left *= Self::parse_factor(it)?;
            } else if op == "/" {
                it.next();
                let denom = Self::parse_factor(it)?;
                if denom == 0.0 {
                    return Err(());
                }
                left /= denom;
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn parse_factor<'a>(
        it: &mut std::iter::Peekable<std::slice::Iter<'a, String>>,
    ) -> Result<f64, ()> {
        let token = it.next().ok_or(())?;
        if token == "(" {
            let val = Self::parse_expr(it)?;
            if it.next().map(|s| s.as_str()) != Some(")") {
                return Err(());
            }
            Ok(val)
        } else if token == "-" {
            Ok(-Self::parse_factor(it)?)
        } else {
            token.parse::<f64>().map_err(|_| ())
        }
    }

    fn eval_units(expr: &str) -> Option<String> {
        let parts: Vec<&str> = expr.split_whitespace().collect();
        if parts.len() == 4
            && (parts[2].eq_ignore_ascii_case("in") || parts[2].eq_ignore_ascii_case("to"))
        {
            let val = parts[0].parse::<f64>().ok()?;
            let from = parts[1].to_lowercase();
            let to = parts[3].to_lowercase();

            if from == "c" && to == "f" {
                return Some(format!("{:.2} °F", val * 9.0 / 5.0 + 32.0));
            } else if from == "f" && to == "c" {
                return Some(format!("{:.2} °C", (val - 32.0) * 5.0 / 9.0));
            } else if from == "kg" && to == "lbs" {
                return Some(format!("{:.2} lbs", val * 2.20462));
            } else if from == "lbs" && to == "kg" {
                return Some(format!("{:.2} kg", val / 2.20462));
            } else if from == "km" && to == "mi" {
                return Some(format!("{:.2} mi", val * 0.621371));
            } else if from == "mi" && to == "km" {
                return Some(format!("{:.2} km", val / 0.621371));
            }
        }
        None
    }

    pub async fn copy_to_clipboard(val: &str) {
        if val.is_empty() {
            return;
        }
        let _ = Command::new("wl-copy").arg(val).status().await;
        let _ = Command::new("notify-send")
            .args(["-a", "Calculator", "Copied to Clipboard", val])
            .spawn();
    }

    pub fn view<'a>(
        &'a self,
        theme: &ShellTheme,
        settings: &ShellSettings,
    ) -> Element<'a, CalcMessage> {
        let (win_w, win_h) = settings.get_window_size("calc", 820, 580);

        let cut_f = theme.overlay_cut;
        let angle_deg = theme.overlay_angle;
        let rad = angle_deg.to_radians();
        let sqrt2 = std::f32::consts::SQRT_2;
        let dx = (cut_f * rad.cos() * sqrt2).min(win_w * 0.45).max(0.0);
        let dy = (cut_f * rad.sin() * sqrt2).min(win_h * 0.45).max(0.0);

        let header_inset = if dy > 22.0 {
            (dx * (1.0 - (22.0 / dy))).max(0.0) + 16.0
        } else {
            16.0
        };
        let footer_inset = if dy > 20.0 {
            (dx * (1.0 - (20.0 / dy))).max(0.0) + 16.0
        } else {
            16.0
        };

        let input = text_input(
            "Type math or unit conversion (e.g. 50 * 12, 100 kg in lbs)...",
            &self.input_text,
        )
        .id(CALC_INPUT_ID)
        .on_input(CalcMessage::InputChanged)
        .on_submit(CalcMessage::CopyResult(self.result_text.clone()))
        .padding(14)
        .size((theme.font_size_overlay) as f32);

        let input_row = container(input).width(Length::Fill).padding(Padding {
            top: 0.0,
            right: header_inset,
            bottom: 0.0,
            left: header_inset,
        });

        let result_display = text(&self.result_text)
            .size((theme.font_size_overlay + 16) as f32)
            .color(theme.success);

        let card_bg = ShapeBox::new()
            .shape(ShapeType::AngledHexagon {
                cut: cut_f,
                angle_deg,
            })
            .background(theme.background)
            .border(theme.border, theme.border_width)
            .size(Length::Fixed(win_w), Length::Fixed(win_h));

        let header_row = container(
            text("🧮 CALCULATOR & UNIT CONVERSION")
                .size((theme.font_size_overlay) as f32)
                .color(theme.text),
        )
        .width(Length::Fill)
        .padding(Padding {
            top: 0.0,
            right: header_inset,
            bottom: 0.0,
            left: header_inset,
        });

        let footer_row = container(
            text("Press [Enter] to copy result  •  [ESC] to dismiss")
                .size(11)
                .color(theme.muted)
                .align_x(Alignment::Center),
        )
        .width(Length::Fill)
        .padding(Padding {
            top: 0.0,
            right: footer_inset,
            bottom: 0.0,
            left: footer_inset,
        });

        let card_content = container(
            column![
                header_row,
                input_row,
                container(result_display).padding(20).center_x(Length::Fill),
                footer_row,
            ]
            .spacing(16)
            .padding(20),
        )
        .width(Length::Fixed(win_w))
        .height(Length::Fixed(win_h));

        let card = Stack::new().push(card_bg.view()).push(card_content);

        container(card)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into()
    }
}
