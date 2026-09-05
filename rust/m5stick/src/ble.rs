use core::{
    fmt::Debug,
    sync::atomic::{AtomicBool, Ordering},
};

use embassy_futures::{join::join, select::select3};
use embassy_time::{Duration, Timer};
use embedded_hal::i2c::I2c as I2cTrait;
use esp_hal::{gpio::Input, rng::Trng};
use esp_println::println;
use trouble_host::prelude::*;

use embedded_graphics::{pixelcolor::Rgb565, prelude::DrawTarget};

use crate::display::{self, Status};
use crate::{mini_joyc::MiniJoyC, mouse::from_joystick, presenter::PresenterAction};

const CONNECTIONS_MAX: usize = 1;
const L2CAP_CHANNELS_MAX: usize = 4;

const DEVICE_NAME: &str = "M5Stick Presenter";

const MOUSE_POLL_INTERVAL_MS: u64 = 10; // 100 Hz
const BUTTON_POLL_INTERVAL_MS: u64 = 15;
const KEY_PRESS_DURATION_MS: u64 = 20;

static MOUSE_NOTIFY_ENABLED: AtomicBool = AtomicBool::new(false);

static KEYBOARD_NOTIFY_ENABLED: AtomicBool = AtomicBool::new(false);

#[rustfmt::skip]
static REPORT_MAP: [u8; 99] = [
    // ---------------------------------------------------------
    // Report ID 1: Mouse
    // ---------------------------------------------------------

    0x05, 0x01,
    0x09, 0x02,
    0xA1, 0x01,

    0x85, 0x01,

    0x09, 0x01,
    0xA1, 0x00,

    0x05, 0x09,
    0x19, 0x01,
    0x29, 0x03,
    0x15, 0x00,
    0x25, 0x01,
    0x95, 0x03,
    0x75, 0x01,
    0x81, 0x02,

    0x95, 0x01,
    0x75, 0x05,
    0x81, 0x03,

    0x05, 0x01,
    0x09, 0x30,
    0x09, 0x31,
    0x15, 0x81,
    0x25, 0x7f,
    0x75, 0x08,
    0x95, 0x02,
    0x81, 0x06,

    0xC0,
    0xC0,

    // ---------------------------------------------------------
    // Report ID 2: Keyboard
    // ---------------------------------------------------------

    0x05, 0x01,
    0x09, 0x06,
    0xA1, 0x01,

    0x85, 0x02,

    // modifiers
    0x05, 0x07,
    0x19, 0xe0,
    0x29, 0xe7,
    0x15, 0x00,
    0x25, 0x01,
    0x75, 0x01,
    0x95, 0x08,
    0x81, 0x02,

    // reserved
    0x95, 0x01,
    0x75, 0x08,
    0x81, 0x03,

    // 6 normal keys
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

    // Report ID 1: Mouse
    #[descriptor(
        uuid = "2908",
        read = encrypted,
        value = [1u8, 1u8]
    )]
    #[characteristic(uuid = "2a4d", read, notify, permissions(encrypted))]
    mouse_report: [u8; 3],

    // Report ID 2: Keyboard
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

pub async fn run<'d, C, I2C, D>(
    controller: C,
    trng: &mut Trng,
    mut button_a: Input<'d>,
    mut button_b: Input<'d>,
    mut joyc: MiniJoyC<I2C>,
    display: &mut D,
) where
    C: Controller,
    I2C: I2cTrait,
    I2C::Error: Debug,
    D: DrawTarget<Color = Rgb565>,
    D::Error: Debug,
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
        name: DEVICE_NAME,
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
            display::render(display, Status::Waiting).unwrap();

            let conn = match advertise(&mut peripheral, &server).await {
                Ok(conn) => conn,

                Err(err) => {
                    println!("advertising error: {:?}", err);

                    continue;
                }
            };

            println!("connection established");
            display::render(display, Status::Connected).unwrap();

            conn.raw().set_bondable(true).unwrap();

            let gatt = gatt_events_task(&server, &conn);

            let presenter = presenter_task(&server, &conn, &mut button_a, &mut button_b);

            let mouse = mouse_task(&server, &conn, &mut joyc);

            select3(gatt, presenter, mouse).await;

            println!("connection ended");
            display::render(display, Status::Waiting).unwrap();
        }
    };

    join(runner_task, peripheral_task).await;
}

async fn advertise<'values, 'server, C: Controller>(
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
            AdStructure::CompleteLocalName(DEVICE_NAME.as_bytes()),
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

    Ok(advertiser.accept().await?.with_attribute_server(server)?)
}

async fn gatt_events_task<P: PacketPool>(server: &Server<'_>, conn: &GattConnection<'_, '_, P>) {
    let mouse = server.hid_service.mouse_report;

    let keyboard = server.hid_service.keyboard_report;

    let mouse_cccd = mouse.cccd_handle;

    let keyboard_cccd = keyboard.cccd_handle;

    loop {
        match conn.next().await {
            GattConnectionEvent::Disconnected { reason } => {
                println!("disconnected: {:?}", reason);

                MOUSE_NOTIFY_ENABLED.store(false, Ordering::Release);

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

                        if Some(event.handle()) == mouse_cccd && data.len() >= 2 {
                            let enabled = data[0] & 1 != 0;

                            MOUSE_NOTIFY_ENABLED.store(enabled, Ordering::Release);

                            println!("mouse notify={}", enabled);
                        }

                        if Some(event.handle()) == keyboard_cccd && data.len() >= 2 {
                            let enabled = data[0] & 1 != 0;

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

async fn presenter_task<P: PacketPool>(
    server: &Server<'_>,
    conn: &GattConnection<'_, '_, P>,
    button_a: &mut Input<'_>,
    button_b: &mut Input<'_>,
) {
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

        Timer::after(Duration::from_millis(BUTTON_POLL_INTERVAL_MS)).await;
    }
}

async fn send_key<P: PacketPool>(
    keyboard: &Characteristic<[u8; 8]>,
    conn: &GattConnection<'_, '_, P>,
    action: PresenterAction,
) {
    let down = [0x00, 0x00, action.key_code(), 0, 0, 0, 0, 0];

    if let Err(err) = keyboard.notify(conn, &down).await {
        println!("keyboard down error: {:?}", err);

        return;
    }

    Timer::after(Duration::from_millis(KEY_PRESS_DURATION_MS)).await;

    let up = [0u8; 8];

    if let Err(err) = keyboard.notify(conn, &up).await {
        println!("keyboard up error: {:?}", err);
    }
}

async fn mouse_task<P, I2C>(
    server: &Server<'_>,
    conn: &GattConnection<'_, '_, P>,
    joyc: &mut MiniJoyC<I2C>,
) where
    P: PacketPool,
    I2C: I2cTrait,
    I2C::Error: Debug,
{
    while !MOUSE_NOTIFY_ENABLED.load(Ordering::Acquire) {
        Timer::after(Duration::from_millis(50)).await;
    }

    println!("mouse ready");

    let report_characteristic = server.hid_service.mouse_report;

    let mut previous_pressed = false;

    loop {
        match joyc.read() {
            Ok(joy) => {
                let mouse = from_joystick(joy);

                let should_send =
                    mouse.dx != 0 || mouse.dy != 0 || mouse.left_pressed != previous_pressed;

                if should_send {
                    let report = [
                        if mouse.left_pressed { 0x01 } else { 0x00 },
                        mouse.dx as u8,
                        mouse.dy as u8,
                    ];

                    match report_characteristic.notify(conn, &report).await {
                        Ok(()) => {
                            previous_pressed = mouse.left_pressed;
                        }

                        Err(err) => {
                            println!("mouse notify error: {:?}", err);
                        }
                    }
                }
            }

            Err(err) => {
                println!("Mini JoyC error: {:?}", err);
            }
        }

        // 実機では100 Hzがちょうどよかった．
        Timer::after(Duration::from_millis(MOUSE_POLL_INTERVAL_MS)).await;
    }
}
