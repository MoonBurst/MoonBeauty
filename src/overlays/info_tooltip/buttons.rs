use super::*;

pub(super) fn make_slanted_button(
    label: String,
    msg: Message,
    bg: Color,
    fg: Color,
    border: Color,
    slant: f32,
    fs: u16,
) -> Element<'static, Message> {
    let btn_bg = ShapeBox::new()
        .shape(ShapeType::Slant {
            left: SlantSide::Left,
            right: SlantSide::Left,
        })
        .background(bg)
        .border(border, 1.0)
        .slant_width(slant)
        .size(Length::Fill, Length::Fill);
    Stack::new()
        .push(btn_bg.view())
        .push(
            button(
                container(text(label).size((fs) as f32).bold().color(fg))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .center_x(Length::Fill)
                    .center_y(Length::Fill),
            )
            .on_press(msg)
            .padding(0)
            .style(|_t, _s| button::Style {
                background: None,
                border: iced::Border::default(),
                ..Default::default()
            }),
        )
        .into()
}


