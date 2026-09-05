#![no_std]
#![no_main]

mod battery;
mod ble;
mod bond_store;
mod config;
mod display;
mod mini_joyc;
mod mouse;
mod presenter;

use embassy_futures::join::join;
use embassy_time::{Duration, Timer};
use embedded_hal_bus::spi::ExclusiveDevice;

use esp_hal::{
    analog::adc::{Adc, AdcConfig, Attenuation},
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
use rand_core::SeedableRng;
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

    let mut peripherals = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));

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

    // esp-rtos / Embassy
    let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);

    let timg0 = TimerGroup::new(peripherals.TIMG0);

    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);

    // BLE pairing用乱数．
    // ADC1はバッテリー測定で使うため，一時的にTRNGへ貸してseedだけ取得する．
    let trng_source = TrngSource::new(peripherals.RNG, peripherals.ADC1.reborrow());

    let trng = Trng::try_new().unwrap();

    let mut security_seed = [0u8; 32];

    trng.read(&mut security_seed);

    drop(trng);

    if trng_source.try_disable().is_err() {
        panic!("failed to release ADC1 from TRNG");
    }

    let mut security_rng = rand_chacha::ChaCha20Rng::from_seed(security_seed);

    let mut adc_config = AdcConfig::new();

    let mut battery_pin = adc_config.enable_pin(peripherals.GPIO38, Attenuation::_11dB);

    let mut battery_adc = Adc::new(peripherals.ADC1, adc_config);

    if let Ok(raw) = nb::block!(battery_adc.read_oneshot(&mut battery_pin)) {
        battery::update(raw);

        println!(
            "battery raw={} level={}% mv~{}",
            raw,
            battery::percent(),
            battery::millivolts(),
        );
    }

    display::render(&mut lcd, display::Status::Waiting, battery::percent(), None).unwrap();

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

    // メモリ
    let mut flash = embassy_embedded_hal::adapter::BlockingAsync::new(
        esp_storage::FlashStorage::new(peripherals.FLASH),
    );

    let battery_task = async {
        loop {
            if let Ok(raw) = nb::block!(battery_adc.read_oneshot(&mut battery_pin)) {
                battery::update(raw);

                println!(
                    "battery raw={} level={}% mv~{}",
                    raw,
                    battery::percent(),
                    battery::millivolts(),
                );
            }

            Timer::after(Duration::from_secs(config::BATTERY_POLL_SECONDS)).await;
        }
    };

    join(
        ble::run(
            controller,
            &mut security_rng,
            button_a,
            button_b,
            joyc,
            &mut lcd,
            &mut flash,
        ),
        battery_task,
    )
    .await;
}
