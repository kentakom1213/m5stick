#![no_std]
#![no_main]

mod mini_joyc;
mod mouse;

use esp_hal::{
    delay::Delay,
    i2c::master::{Config as I2cConfig, I2c},
    time::Rate,
};
use esp_println::println;

use mini_joyc::MiniJoyC;
use mouse::from_joystick;

esp_bootloader_esp_idf::esp_app_desc!();

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("panic: {info}");

    loop {}
}

#[esp_hal::main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());

    let mut delay = Delay::new();

    let i2c = I2c::new(
        peripherals.I2C0,
        I2cConfig::default().with_frequency(Rate::from_khz(200)),
    )
    .unwrap()
    .with_sda(peripherals.GPIO0)
    .with_scl(peripherals.GPIO26);

    let mut joyc = MiniJoyC::new(i2c);

    println!("Mini JoyC mouse test");

    loop {
        match joyc.read() {
            Ok(joy) => {
                let mouse = from_joystick(joy);

                println!(
                    "joy=({:4}, {:4}, {}) mouse=({:4}, {:4}, {})",
                    joy.x, joy.y, joy.pressed, mouse.dx, mouse.dy, mouse.left_pressed,
                );
            }

            Err(err) => {
                println!("Mini JoyC error: {:?}", err);
            }
        }

        delay.delay_millis(100);
    }
}
