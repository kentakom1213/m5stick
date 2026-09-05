#![no_std]
#![no_main]

use core::sync::atomic::{AtomicBool, Ordering};

use embassy_executor::Spawner;
use embassy_futures::{join::join, select::select};
use embassy_time::{Duration, Timer};

use esp_hal::{
    clock::CpuClock,
    interrupt::software::SoftwareInterruptControl,
    rng::{Trng, TrngSource},
    timer::timg::TimerGroup,
};
use esp_println::println;
use esp_radio::ble::controller::BleConnector;

use trouble_host::prelude::*;

static HID_NOTIFY_ENABLED: AtomicBool = AtomicBool::new(false);

esp_bootloader_esp_idf::esp_app_desc!();

const CONNECTIONS_MAX: usize = 1;

// Securityを使うので少し余裕を持たせる
const L2CAP_CHANNELS_MAX: usize = 4;

// 3-byte mouse report:
//
// byte 0:
//   bit 0 = left button
//   bit 1 = right button
//   bit 2 = middle button
//
// byte 1:
//   relative X (-127..127)
//
// byte 2:
//   relative Y (-127..127)
static MOUSE_REPORT_MAP: [u8; 50] = [
    // Usage Page (Generic Desktop)
    0x05, 0x01, // Usage (Mouse)
    0x09, 0x02, // Collection (Application)
    0xA1, 0x01, // Usage (Pointer)
    0x09, 0x01, // Collection (Physical)
    0xA1, 0x00, // Usage Page (Button)
    0x05, 0x09, // Usage Minimum (Button 1)
    0x19, 0x01, // Usage Maximum (Button 3)
    0x29, 0x03, // Logical Minimum (0)
    0x15, 0x00, // Logical Maximum (1)
    0x25, 0x01, // Report Count (3)
    0x95, 0x03, // Report Size (1)
    0x75, 0x01, // Input (Data, Variable, Absolute)
    0x81, 0x02, // Padding: 5 bits
    0x95, 0x01, 0x75, 0x05, 0x81, 0x03, // Usage Page (Generic Desktop)
    0x05, 0x01, // Usage X
    0x09, 0x30, // Usage Y
    0x09, 0x31, // Logical Minimum (-127)
    0x15, 0x81, // Logical Maximum (127)
    0x25, 0x7F, // Report Size (8)
    0x75, 0x08, // Report Count (2)
    0x95, 0x02, // Input (Data, Variable, Relative)
    0x81, 0x06, // End Collection
    0xC0, // End Collection
    0xC0,
];

#[gatt_server]
struct Server {
    hid_service: HidService,
    battery_service: BatteryService,
}

#[gatt_service(uuid = service::HUMAN_INTERFACE_DEVICE)]
struct HidService {
    // HID v1.01
    //
    // [0x01, 0x01] = HID version 1.01
    // country code  = 0
    // flags         = 0x03
    #[characteristic(
        uuid = "2a4a",
        read,
        value = [0x01, 0x01, 0x00, 0x03],
        permissions(encrypted)
    )]
    hid_information: [u8; 4],

    // HID report descriptor
    #[characteristic(
        uuid = "2a4b",
        read,
        value = MOUSE_REPORT_MAP,
        permissions(encrypted)
    )]
    report_map: [u8; 50],

    // HID Control Point
    #[characteristic(uuid = "2a4c", write_without_response, permissions(encrypted))]
    control_point: u8,

    // 0 = Boot Protocol
    // 1 = Report Protocol
    #[characteristic(
        uuid = "2a4e",
        read,
        write_without_response,
        value = 1,
        permissions(encrypted)
    )]
    protocol_mode: u8,

    // Report Reference:
    //
    // report ID   = 0
    // report type = 1 (Input Report)
    #[descriptor(
        uuid = "2908",
        read = encrypted,
        value = [0u8, 1u8]
    )]
    #[characteristic(uuid = "2a4d", read, notify, permissions(encrypted))]
    input_report: [u8; 3],
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

    run_ble(controller, &mut trng).await;
}

async fn run_ble<C>(controller: C, trng: &mut Trng)
where
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

            let mouse = mouse_test_task(&server, &conn);

            // どちらかが終了したら
            // connection loopから抜けて再advertiseする
            select(gatt, mouse).await;

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
    let input = server.hid_service.input_report;
    let input_cccd = input.cccd_handle;

    loop {
        match conn.next().await {
            GattConnectionEvent::Disconnected { reason } => {
                println!("disconnected: {:?}", reason);

                HID_NOTIFY_ENABLED.store(false, Ordering::Release);

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
                        if Some(event.handle()) == input_cccd {
                            let data = event.data();

                            println!("HID CCCD write: data={:?}", data);

                            if data.len() >= 2 {
                                let notify = data[0] & 0x01 != 0;

                                HID_NOTIFY_ENABLED.store(notify, Ordering::Release);

                                println!("HID notify enabled={}", notify);
                            }
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

async fn mouse_test_task<P: PacketPool>(server: &Server<'_>, conn: &GattConnection<'_, '_, P>) {
    let input = server.hid_service.input_report;

    println!("waiting for HID notification subscription");

    while !HID_NOTIFY_ENABLED.load(Ordering::Acquire) {
        Timer::after(Duration::from_millis(50)).await;
    }

    println!("HID notifications really enabled");

    Timer::after(Duration::from_millis(500)).await;

    println!("moving mouse right");

    for _ in 0..10 {
        let report = [0x00, 5u8, 0u8];

        println!("send report: {:?}", report);

        if let Err(err) = input.notify(conn, &report).await {
            println!("mouse notify error: {:?}", err);

            return;
        }

        Timer::after(Duration::from_millis(50)).await;
    }

    let _ = input.notify(conn, &[0, 0, 0]).await;

    println!("test movement sent");

    loop {
        Timer::after(Duration::from_secs(60)).await;
    }
}
