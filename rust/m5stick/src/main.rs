#![no_std]
#![no_main]

mod ble;
mod display;
mod mini_joyc;
mod mouse;
mod presenter;

use embedded_hal_bus::spi::ExclusiveDevice;

use esp_hal::{
    clock::CpuClock,
    delay::Delay,
    gpio::{Input, InputConfig, Level, Output, OutputConfig},
    i2c::master::{Config as I2cConfig, I2c},
    interrupt::software::SoftwareInterruptControl,
    rng::{Trng, TrngSource},
    spi::{
        Mode,
        master::{Config as SpiConfig, Spi},
    },
    time::Rate,
    timer::timg::TimerGroup,
};

use mipidsi::{
    Builder,
    interface::SpiInterface,
    models::ST7789,
    options::{ColorInversion, ColorOrder},
};

use esp_println::println;
use esp_radio::ble::controller::BleConnector;
use trouble_host::prelude::ExternalController;

use mini_joyc::MiniJoyC;

esp_bootloader_esp_idf::esp_app_desc!();

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("panic: {info}");

    loop {}
}

#[esp_rtos::main]
async fn main(_spawner: embassy_executor::Spawner) {
    println!("starting M5Stick Presenter");

    let peripherals = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));

    esp_alloc::heap_allocator!(size: 72 * 1024);

    // M5StickC Plus2の電源保持．
    // 電源ON/OFF自体はButton Cに任せる．
    let _power_hold = Output::new(peripherals.GPIO4, Level::High, OutputConfig::default());

    // LCDバックライト
    let _backlight = Output::new(peripherals.GPIO27, Level::High, OutputConfig::default());

    let dc = Output::new(peripherals.GPIO14, Level::Low, OutputConfig::default());

    let rst = Output::new(peripherals.GPIO12, Level::High, OutputConfig::default());

    let cs = Output::new(peripherals.GPIO5, Level::High, OutputConfig::default());

    let spi = Spi::new(
        peripherals.SPI2,
        SpiConfig::default()
            .with_frequency(Rate::from_mhz(20))
            .with_mode(Mode::_0),
    )
    .unwrap()
    .with_sck(peripherals.GPIO13)
    .with_mosi(peripherals.GPIO15);

    let spi_device = ExclusiveDevice::new(spi, cs, Delay::new()).unwrap();

    let mut lcd_buffer = [0u8; 512];

    let interface = SpiInterface::new(spi_device, dc, &mut lcd_buffer);

    let mut lcd_delay = Delay::new();

    let mut lcd = Builder::new(ST7789, interface)
        .reset_pin(rst)
        .display_size(135, 240)
        .display_offset(52, 40)
        .invert_colors(ColorInversion::Inverted)
        .color_order(ColorOrder::Rgb)
        .init(&mut lcd_delay)
        .unwrap();

    display::render(&mut lcd, display::Status::Waiting).unwrap();

    // esp-rtos / Embassy
    let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);

    let timg0 = TimerGroup::new(peripherals.TIMG0);

    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);

    // BLE pairing用乱数
    let _trng_source = TrngSource::new(peripherals.RNG, peripherals.ADC1);

    let mut trng = Trng::try_new().unwrap();

    // BLE controller
    let connector = BleConnector::new(peripherals.BT, Default::default()).unwrap();

    let controller: ExternalController<_, 20> = ExternalController::new(connector);

    // Presenter buttons
    let button_a = Input::new(peripherals.GPIO37, InputConfig::default());

    let button_b = Input::new(peripherals.GPIO39, InputConfig::default());

    // Mini JoyC
    //
    // 実機では100 kHzの方が安定した．
    let i2c = I2c::new(
        peripherals.I2C0,
        I2cConfig::default().with_frequency(Rate::from_khz(100)),
    )
    .unwrap()
    .with_sda(peripherals.GPIO0)
    .with_scl(peripherals.GPIO26);

    let joyc = MiniJoyC::new(i2c);

    ble::run(controller, &mut trng, button_a, button_b, joyc, &mut lcd).await;
}
