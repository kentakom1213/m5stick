#![no_std]
#![no_main]

use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::*,
};
use embedded_hal_bus::spi::ExclusiveDevice;
use esp_hal::{
    delay::Delay,
    gpio::{Level, Output, OutputConfig},
    spi::{
        master::{Config as SpiConfig, Spi},
        Mode,
    },
    time::Rate,
};
use esp_println::println;
use mipidsi::{
    interface::SpiInterface,
    models::ST7789,
    options::{ColorInversion, ColorOrder},
    Builder,
};

esp_bootloader_esp_idf::esp_app_desc!();

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("panic: {info}");

    loop {}
}

#[esp_hal::main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());

    println!("starting LCD test");

    // M5StickC Plus2 の電源保持
    let _hold = Output::new(
        peripherals.GPIO4,
        Level::High,
        OutputConfig::default(),
    );

    // LCD バックライト
    let _backlight = Output::new(
        peripherals.GPIO27,
        Level::High,
        OutputConfig::default(),
    );

    // LCD 制御ピン
    let dc = Output::new(
        peripherals.GPIO14,
        Level::Low,
        OutputConfig::default(),
    );

    let rst = Output::new(
        peripherals.GPIO12,
        Level::High,
        OutputConfig::default(),
    );

    let cs = Output::new(
        peripherals.GPIO5,
        Level::High,
        OutputConfig::default(),
    );

    // SPI
    let spi = Spi::new(
        peripherals.SPI2,
        SpiConfig::default()
            .with_frequency(Rate::from_mhz(20))
            .with_mode(Mode::_0),
    )
    .unwrap()
    .with_sck(peripherals.GPIO13)
    .with_mosi(peripherals.GPIO15);

    // mipidsi は SpiDevice を要求するので，
    // SPI bus + CS を ExclusiveDevice にまとめる
    let spi_device =
        ExclusiveDevice::new(spi, cs, Delay::new()).unwrap();

    let mut buffer = [0u8; 512];

    let interface =
        SpiInterface::new(spi_device, dc, &mut buffer);

    let mut delay = Delay::new();

    println!("initializing LCD");

    let mut display = Builder::new(ST7789, interface)
        .reset_pin(rst)
        .display_size(135, 240)
        .display_offset(52, 40)
        .invert_colors(ColorInversion::Inverted)
        .color_order(ColorOrder::Rgb)
        .init(&mut delay)
        .unwrap();

    println!("LCD initialized");

    display.clear(Rgb565::RED).unwrap();

    println!("screen should now be red");

    loop {}
}
