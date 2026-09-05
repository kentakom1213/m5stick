#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_futures::join::join;
use embassy_time::{Duration, Timer};

use esp_hal::{
    clock::CpuClock, interrupt::software::SoftwareInterruptControl, timer::timg::TimerGroup,
};
use esp_println::println;
use esp_radio::ble::controller::BleConnector;

use trouble_host::prelude::*;

esp_bootloader_esp_idf::esp_app_desc!();

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("panic: {info}");

    loop {}
}

#[esp_rtos::main]
async fn main(_spawner: Spawner) {
    println!("starting BLE example");

    let peripherals = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));

    esp_alloc::heap_allocator!(size: 72 * 1024);

    let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);

    let timg0 = TimerGroup::new(peripherals.TIMG0);

    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);

    println!("RTOS started");

    let connector = BleConnector::new(peripherals.BT, Default::default()).unwrap();

    let controller: ExternalController<_, 20> = ExternalController::new(connector);

    run_ble(controller).await;
}

async fn run_ble<C>(controller: C)
where
    C: Controller,
{
    const CONNECTIONS_MAX: usize = 1;
    const L2CAP_CHANNELS_MAX: usize = 1;

    // 今はテスト用固定アドレス
    let address = Address::random([0x42, 0x11, 0x22, 0x33, 0x44, 0xc0]);

    println!("BLE address: {:?}", address);

    let mut resources: HostResources<DefaultPacketPool, CONNECTIONS_MAX, L2CAP_CHANNELS_MAX> =
        HostResources::new();

    let builder = trouble_host::new(controller, &mut resources).set_random_address(address);

    let stack = builder.build();

    let mut runner = stack.runner;
    let mut peripheral = stack.peripheral;

    let runner_task = async {
        loop {
            match runner.run().await {
                Ok(()) => {}
                Err(err) => {
                    panic!("BLE runner error: {:?}", err);
                }
            }
        }
    };

    let advertising_task = async {
        let mut adv_data = [0u8; 31];

        let len = AdStructure::encode_slice(
            &[
                AdStructure::Flags(LE_GENERAL_DISCOVERABLE | BR_EDR_NOT_SUPPORTED),
                AdStructure::CompleteLocalName(b"M5Stick Mouse"),
            ],
            &mut adv_data,
        )
        .unwrap();

        let _advertiser = peripheral
            .advertise(
                &Default::default(),
                Advertisement::NonconnectableScannableUndirected {
                    adv_data: &adv_data[..len],
                    scan_data: &[],
                },
            )
            .await
            .unwrap();

        println!("advertising as \"M5Stick Mouse\"");

        // _advertiser をdropしないよう保持し続ける
        loop {
            Timer::after(Duration::from_secs(60)).await;
        }
    };

    join(runner_task, advertising_task).await;
}
