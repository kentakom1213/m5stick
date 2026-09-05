#![no_std]
#![no_main]

#[path = "../src/presenter.rs"]
mod presenter;

use presenter::PresenterAction;

use core::sync::atomic::{AtomicBool, Ordering};

static KEYBOARD_NOTIFY_ENABLED: AtomicBool = AtomicBool::new(false);

use embassy_executor::Spawner;
use embassy_futures::{join::join, select::select};
use embassy_time::{Duration, Timer};
use esp_hal::{
    clock::CpuClock,
    gpio::{Input, InputConfig},
    interrupt::software::SoftwareInterruptControl,
    rng::{Trng, TrngSource},
    timer::timg::TimerGroup,
};
use esp_println::println;
use esp_radio::ble::controller::BleConnector;

use trouble_host::prelude::*;

esp_bootloader_esp_idf::esp_app_desc!();

const CONNECTIONS_MAX: usize = 1;

// Securityを使うので少し余裕を持たせる
const L2CAP_CHANNELS_MAX: usize = 4;

#[rustfmt::skip]
static REPORT_MAP: [u8; 99] = [
    // =========================================================
    // Report ID 1: Mouse
    // =========================================================

    0x05, 0x01, // Usage Page (Generic Desktop)
    0x09, 0x02, // Usage (Mouse)
    0xA1, 0x01, // Collection (Application)

    0x85, 0x01, // Report ID (1)

    0x09, 0x01, // Usage (Pointer)
    0xA1, 0x00, // Collection (Physical)

    0x05, 0x09, // Usage Page (Button)
    0x19, 0x01, // Usage Minimum (1)
    0x29, 0x03, // Usage Maximum (3)
    0x15, 0x00, // Logical Minimum (0)
    0x25, 0x01, // Logical Maximum (1)
    0x95, 0x03, // Report Count (3)
    0x75, 0x01, // Report Size (1)
    0x81, 0x02, // Input

    0x95, 0x01, // Padding
    0x75, 0x05,
    0x81, 0x03,

    0x05, 0x01, // Generic Desktop
    0x09, 0x30, // X
    0x09, 0x31, // Y
    0x15, 0x81, // -127
    0x25, 0x7f, // 127
    0x75, 0x08, // 8 bit
    0x95, 0x02, // X + Y
    0x81, 0x06, // Relative

    0xC0,
    0xC0,

    // =========================================================
    // Report ID 2: Keyboard
    // =========================================================

    0x05, 0x01, // Usage Page (Generic Desktop)
    0x09, 0x06, // Usage (Keyboard)
    0xA1, 0x01, // Collection (Application)

    0x85, 0x02, // Report ID (2)

    // modifier keys
    0x05, 0x07, // Usage Page (Keyboard)
    0x19, 0xe0,
    0x29, 0xe7,
    0x15, 0x00,
    0x25, 0x01,
    0x75, 0x01,
    0x95, 0x08,
    0x81, 0x02,

    // reserved byte
    0x95, 0x01,
    0x75, 0x08,
    0x81, 0x03,

    // 6 regular keys
    0x95, 0x06,
    0x75, 0x08,
    0x15, 0x00,
    0x25, 0x65,
    0x05, 0x07,
    0x19, 0x00,
    0x29, 0x65,
    0x81, 0x00,

    0xC0,
];

#[gatt_server]
struct Server {
    hid_service: HidService,
    battery_service: BatteryService,
}

#[gatt_service(uuid = service::HUMAN_INTERFACE_DEVICE)]
struct HidService {
    #[characteristic(
        uuid = "2a4a",
        read,
        value = [0x01, 0x01, 0x00, 0x03],
        permissions(encrypted)
    )]
    hid_information: [u8; 4],

    #[characteristic(
        uuid = "2a4b",
        read,
        value = REPORT_MAP,
        permissions(encrypted)
    )]
    report_map: [u8; 99],

    #[characteristic(uuid = "2a4c", write_without_response, permissions(encrypted))]
    control_point: u8,

    #[characteristic(
        uuid = "2a4e",
        read,
        write_without_response,
        value = 1,
        permissions(encrypted)
    )]
    protocol_mode: u8,

    // Report ID 1: Mouse Input
    #[descriptor(
        uuid = "2908",
        read = encrypted,
        value = [1u8, 1u8]
    )]
    #[characteristic(uuid = "2a4d", read, notify, permissions(encrypted))]
    mouse_report: [u8; 3],

    // Report ID 2: Keyboard Input
    #[descriptor(
        uuid = "2908",
        read = encrypted,
        value = [2u8, 1u8]
    )]
    #[characteristic(uuid = "2a4d", read, notify, permissions(encrypted))]
    keyboard_report: [u8; 8],
}

#[gatt_service(uuid = service::BATTERY)]
struct BatteryService {
    #[characteristic(
        uuid = characteristic::BATTERY_LEVEL,
        read,
        notify,
        value = 100,
        permissions(encrypted)
    )]
    level: u8,
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("panic: {info}");

    loop {}
}

#[esp_rtos::main]
async fn main(_spawner: Spawner) {
    println!("starting BLE HID mouse");

    let peripherals = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));

    esp_alloc::heap_allocator!(size: 72 * 1024);

    let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);

    let timg0 = TimerGroup::new(peripherals.TIMG0);

    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);

    println!("RTOS started");

    // Pairing用の暗号学的乱数源
    let _trng_source = TrngSource::new(peripherals.RNG, peripherals.ADC1);

    let mut trng = Trng::try_new().unwrap();

    println!("TRNG initialized");

    let connector = BleConnector::new(peripherals.BT, Default::default()).unwrap();

    let controller: ExternalController<_, 20> = ExternalController::new(connector);

    // Button の初期化
    let button_a = Input::new(peripherals.GPIO37, InputConfig::default());
    let button_b = Input::new(peripherals.GPIO39, InputConfig::default());

    run_ble(controller, &mut trng, button_a, button_b).await;
}

async fn run_ble<'d, C>(
    controller: C,
    trng: &mut Trng,
    mut button_a: Input<'d>,
    mut button_b: Input<'d>,
) where
    C: Controller,
{
    let address = Address::random([0x42, 0x11, 0x22, 0x33, 0x44, 0xc0]);

    println!("BLE address: {:?}", address);

    let mut resources: HostResources<DefaultPacketPool, CONNECTIONS_MAX, L2CAP_CHANNELS_MAX> =
        HostResources::new();

    let builder = trouble_host::new(controller, &mut resources)
        .set_random_address(address)
        .set_random_generator_seed(trng);

    let stack = builder.build();

    let mut runner = stack.runner;
    let mut peripheral = stack.peripheral;

    let server = Server::new_with_config(GapConfig::Peripheral(PeripheralConfig {
        name: "M5Stick Mouse",
        appearance: &appearance::human_interface_device::GENERIC_HUMAN_INTERFACE_DEVICE,
    }))
    .unwrap();

    println!("GATT server initialized");

    let runner_task = async {
        loop {
            if let Err(err) = runner.run().await {
                panic!("BLE runner error: {:?}", err);
            }
        }
    };

    let peripheral_task = async {
        loop {
            println!("advertising...");

            let conn = match advertise("M5Stick Mouse", &mut peripheral, &server).await {
                Ok(conn) => conn,

                Err(err) => {
                    println!("advertising error: {:?}", err);

                    continue;
                }
            };

            println!("connection established");

            // HID over GATTではpairingさせる
            conn.raw().set_bondable(true).unwrap();

            let gatt = gatt_events_task(&server, &conn);

            let presenter = presenter_task(&server, &conn, &mut button_a, &mut button_b);

            select(gatt, presenter).await;

            println!("connection ended");
        }
    };

    join(runner_task, peripheral_task).await;
}

async fn advertise<'values, 'server, C: Controller>(
    name: &'values str,
    peripheral: &mut Peripheral<'values, C, DefaultPacketPool>,
    server: &'server Server<'values>,
) -> Result<GattConnection<'values, 'server, DefaultPacketPool>, BleHostError<C::Error>> {
    let mut adv_data = [0u8; 31];

    let len = AdStructure::encode_slice(
        &[
            AdStructure::Flags(LE_GENERAL_DISCOVERABLE | BR_EDR_NOT_SUPPORTED),
            AdStructure::ServiceUuids16(&[
                service::HUMAN_INTERFACE_DEVICE.to_le_bytes(),
                service::BATTERY.to_le_bytes(),
            ]),
            AdStructure::CompleteLocalName(name.as_bytes()),
        ],
        &mut adv_data,
    )?;

    let advertiser = peripheral
        .advertise(
            &Default::default(),
            Advertisement::ConnectableScannableUndirected {
                adv_data: &adv_data[..len],

                scan_data: &[],
            },
        )
        .await?;

    println!("waiting for connection");

    let conn = advertiser.accept().await?.with_attribute_server(server)?;

    Ok(conn)
}

async fn gatt_events_task<P: PacketPool>(server: &Server<'_>, conn: &GattConnection<'_, '_, P>) {
    let keyboard = server.hid_service.keyboard_report;
    let keyboard_cccd = keyboard.cccd_handle;

    loop {
        match conn.next().await {
            GattConnectionEvent::Disconnected { reason } => {
                println!("disconnected: {:?}", reason);

                KEYBOARD_NOTIFY_ENABLED.store(false, Ordering::Release);

                break;
            }

            GattConnectionEvent::PairingComplete { security_level, .. } => {
                println!("pairing complete: {:?}", security_level);
            }

            GattConnectionEvent::PairingFailed(err) => {
                println!("pairing failed: {:?}", err);
            }

            GattConnectionEvent::Gatt { event } => {
                let reply = match event {
                    GattEvent::Write(event) => {
                        let data = event.data();

                        if Some(event.handle()) == keyboard_cccd && data.len() >= 2 {
                            let enabled = data[0] & 0x01 != 0;

                            KEYBOARD_NOTIFY_ENABLED.store(enabled, Ordering::Release);

                            println!("keyboard notify={}", enabled);
                        }

                        event.accept()
                    }

                    other => other.accept(),
                };

                match reply {
                    Ok(reply) => {
                        reply.send().await;
                    }

                    Err(err) => {
                        println!("GATT error: {:?}", err);
                    }
                }
            }

            _ => {}
        }
    }
}

async fn send_key<P: PacketPool>(
    keyboard: &Characteristic<[u8; 8]>,
    conn: &GattConnection<'_, '_, P>,
    action: PresenterAction,
) {
    let key = action.key_code();

    // key down
    let down = [
        0x00, // modifiers
        0x00, // reserved
        key, 0, 0, 0, 0, 0,
    ];

    if let Err(err) = keyboard.notify(conn, &down).await {
        println!("keyboard down error: {:?}", err);

        return;
    }

    Timer::after(Duration::from_millis(20)).await;

    // key up
    let up = [0u8; 8];

    if let Err(err) = keyboard.notify(conn, &up).await {
        println!("keyboard up error: {:?}", err);
    }
}

async fn presenter_task<P: PacketPool>(
    server: &Server<'_>,
    conn: &GattConnection<'_, '_, P>,
    button_a: &mut Input<'_>,
    button_b: &mut Input<'_>,
) {
    println!("waiting for keyboard notification subscription");

    while !KEYBOARD_NOTIFY_ENABLED.load(Ordering::Acquire) {
        Timer::after(Duration::from_millis(50)).await;
    }

    println!("keyboard ready");

    let keyboard = server.hid_service.keyboard_report;

    let mut a_was_pressed = button_a.is_low();
    let mut b_was_pressed = button_b.is_low();

    loop {
        let a_pressed = button_a.is_low();
        let b_pressed = button_b.is_low();

        if a_pressed && !a_was_pressed {
            println!("next slide");

            send_key(&keyboard, conn, PresenterAction::NextSlide).await;
        }

        if b_pressed && !b_was_pressed {
            println!("previous slide");

            send_key(&keyboard, conn, PresenterAction::PreviousSlide).await;
        }

        a_was_pressed = a_pressed;
        b_was_pressed = b_pressed;

        Timer::after(Duration::from_millis(15)).await;
    }
}
