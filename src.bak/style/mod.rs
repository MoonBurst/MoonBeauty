pub mod shape_box;

pub use shape_box::{ShapeBox, ShapeType, SlantSide};

pub trait TextExt {
    fn bold(self) -> Self;
}

impl<'a, Theme, Renderer> TextExt for iced::widget::Text<'a, Theme, Renderer>
where
    Theme: iced::widget::text::Catalog + 'a,
    Renderer: iced::advanced::text::Renderer,
    Renderer::Font: From<iced::Font>,
{
    fn bold(self) -> Self {
        self.font(iced::Font {
            weight: iced::font::Weight::Bold,
            ..Default::default()
        })
    }
}
