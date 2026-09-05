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
    profile_label: &str,
    battery: u8,
    peer_address: Option<&str>,
    joyc_ok: bool,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    display.clear(Rgb565::BLACK)?;

    let large_white_style = MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE);

    let small_style = MonoTextStyle::new(&FONT_6X10, Rgb565::WHITE);

    let status_style = MonoTextStyle::new(&FONT_10X20, status_color(status));

    let battery_style = MonoTextStyle::new(&FONT_10X20, battery_color(battery));

    let ok_style = MonoTextStyle::new(&FONT_6X10, Rgb565::GREEN);

    let ng_style = MonoTextStyle::new(&FONT_6X10, Rgb565::RED);

    let status_text = match status {
        Status::Waiting => "WAITING",
        Status::Connected => "CONNECTED",
    };

    Text::new(status_text, Point::new(10, 35), status_style).draw(display)?;

    let address_text = peer_address.unwrap_or("--:--:--:--:--:--");

    Text::new(address_text, Point::new(10, 60), small_style).draw(display)?;

    Text::new(profile_label, Point::new(10, 85), large_white_style).draw(display)?;

    let mut battery_text = heapless::String::<16>::new();

    write!(battery_text, "[{}] {}%", battery_icon(battery), battery).unwrap();

    Text::new(&battery_text, Point::new(10, 120), battery_style).draw(display)?;

    Text::new("BLE", Point::new(10, 155), large_white_style).draw(display)?;
    Text::new("OK", Point::new(70, 155), ok_style).draw(display)?;

    Text::new("JoyC", Point::new(10, 180), large_white_style).draw(display)?;

    let (joyc_text, joyc_style) = if joyc_ok {
        ("OK", ok_style)
    } else {
        ("NG", ng_style)
    };

    Text::new(joyc_text, Point::new(70, 180), joyc_style).draw(display)?;

    Ok(())
}

fn status_color(status: Status) -> Rgb565 {
    match status {
        Status::Waiting => Rgb565::YELLOW,
        Status::Connected => Rgb565::GREEN,
    }
}

fn battery_color(percent: u8) -> Rgb565 {
    match percent {
        50..=100 => Rgb565::GREEN,
        20..=49 => Rgb565::YELLOW,
        _ => Rgb565::RED,
    }
}

fn battery_icon(percent: u8) -> &'static str {
    match percent {
        75..=100 => "###",
        40..=74 => "## ",
        10..=39 => "#  ",
        _ => "   ",
    }
}
