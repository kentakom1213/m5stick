use core::fmt::Write;

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

pub fn render<D>(
    display: &mut D,
    status: Status,
    battery: u8,
    peer_address: Option<&str>,
) -> Result<(), D::Error>
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

    if let Some(peer_address) = peer_address {
        Text::new(peer_address, Point::new(10, 80), text_style).draw(display)?;
    }

    let mut battery_text = heapless::String::<16>::new();

    write!(battery_text, "[{}] {}%", battery_icon(battery), battery).unwrap();

    Text::new(&battery_text, Point::new(10, 105), title_style).draw(display)?;

    Text::new("A  > Next", Point::new(10, 135), text_style).draw(display)?;

    Text::new("B  < Back", Point::new(10, 155), text_style).draw(display)?;

    Text::new("Joy: Pointer", Point::new(10, 180), text_style).draw(display)?;

    Text::new("Press: Click", Point::new(10, 200), text_style).draw(display)?;

    Ok(())
}

fn battery_icon(percent: u8) -> &'static str {
    match percent {
        75..=100 => "###",
        40..=74 => "## ",
        10..=39 => "#  ",
        _ => "   ",
    }
}
