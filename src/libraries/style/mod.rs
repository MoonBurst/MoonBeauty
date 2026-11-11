pub mod shape_box;

pub use shape_box::{ShapeBox, ShapeType, SlantSide};

/// Shell font family. Matches `fontFamily` in the QML build (Shapes.qml / Theme.qml).
pub const SHELL_FAMILY: &str = "Fira Code";

/// Build a `Font` using the shell family at the given weight.
pub fn shell_font(weight: iced::font::Weight) -> iced::Font {
    iced::Font {
        family: iced::font::Family::Name(SHELL_FAMILY),
        weight,
        ..Default::default()
    }
}

pub trait TextExt {
    fn bold(self) -> Self;
    fn shell(self) -> Self;
}

impl<'a, Theme, Renderer> TextExt for iced::widget::Text<'a, Theme, Renderer>
where
    Theme: iced::widget::text::Catalog + 'a,
    Renderer: iced::advanced::text::Renderer,
    Renderer::Font: From<iced::Font>,
{
    fn bold(self) -> Self {
        self.font(shell_font(iced::font::Weight::Bold))
    }
    fn shell(self) -> Self {
        self.font(shell_font(iced::font::Weight::Normal))
    }
}
