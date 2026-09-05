#![no_std]
#![no_main]

mod ble;
mod mini_joyc;
mod mouse;
mod presenter;

use esp_hal::{
    clock::CpuClock,
    gpio::{Input, InputConfig, Level, Output, OutputConfig},
    i2c::master::{Config as I2cConfig, I2c},
    interrupt::software::SoftwareInterruptControl,
    rng::{Trng, TrngSource},
    time::Rate,
    timer::timg::TimerGroup,
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

    // M5StickC Plus2 の電源保持．
    // この値をLOWにすれば，バッテリー駆動時は電源OFFできる．
    let _power_hold = Output::new(peripherals.GPIO4, Level::High, OutputConfig::default());

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

    ble::run(controller, &mut trng, button_a, button_b, joyc).await;
}
