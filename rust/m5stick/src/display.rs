use embedded_graphics::{
    mono_font::{
        MonoTextStyle,
        ascii::{FONT_6X10, FONT_10X20},
    },
    pixelcolor::Rgb565,
    prelude::*,
    text::Text,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Waiting,
    Connected,
}

pub fn render<D>(display: &mut D, status: Status) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    display.clear(Rgb565::BLACK)?;

    let title_style = MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE);

    let text_style = MonoTextStyle::new(&FONT_6X10, Rgb565::WHITE);

    let status_style = MonoTextStyle::new(
        &FONT_10X20,
        match status {
            Status::Waiting => Rgb565::YELLOW,
            Status::Connected => Rgb565::GREEN,
        },
    );

    Text::new("PRESENTER", Point::new(10, 25), title_style).draw(display)?;

    let status_text = match status {
        Status::Waiting => "WAITING",
        Status::Connected => "CONNECTED",
    };

    Text::new(status_text, Point::new(10, 60), status_style).draw(display)?;

    Text::new("A  > Next", Point::new(10, 100), text_style).draw(display)?;

    Text::new("B  < Back", Point::new(10, 120), text_style).draw(display)?;

    Text::new("Joy: Pointer", Point::new(10, 150), text_style).draw(display)?;

    Text::new("Press: Click", Point::new(10, 170), text_style).draw(display)?;

    Ok(())
}
