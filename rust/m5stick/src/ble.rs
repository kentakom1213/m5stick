use core::{
    fmt::{Debug, Write},
    sync::atomic::{AtomicBool, Ordering},
};

use embassy_futures::{join::join, select::select4};
use embassy_time::{Duration, Timer};
use embedded_hal::i2c::I2c as I2cTrait;
use esp_hal::gpio::{Input, Output};
use esp_println::println;
use trouble_host::prelude::*;

use embedded_graphics::{pixelcolor::Rgb565, prelude::DrawTarget};
use embedded_storage_async::nor_flash::NorFlash;

use crate::display::{self, Status};
use crate::{battery, bond_store};
use crate::{
    config::{
        Action, DISPLAY_TIMEOUT_SECONDS, MOUSE_BUTTON_LEFT, MOUSE_BUTTON_MIDDLE, MOUSE_BUTTON_RIGHT,
    },
    input::{InputEngine, InputSample, InputState},
    mini_joyc::MiniJoyC,
    mouse::MouseMapper,
    profile::ProfileManager,
};

const CONNECTIONS_MAX: usize = 1;
const L2CAP_CHANNELS_MAX: usize = 4;
const BONDS_MAX: usize = 4;

const DEVICE_NAME: &str = "M5Stick Presenter";

const KEY_PRESS_DURATION_MS: u64 = 20;

static MOUSE_NOTIFY_ENABLED: AtomicBool = AtomicBool::new(false);

static KEYBOARD_NOTIFY_ENABLED: AtomicBool = AtomicBool::new(false);

#[rustfmt::skip]
static REPORT_MAP: [u8; 101] = [
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
    0x09, 0x38,
    0x15, 0x81,
    0x25, 0x7f,
    0x75, 0x08,
    0x95, 0x03,
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
    report_map: [u8; 101],

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
    mouse_report: [u8; 4],

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

pub async fn run<'d, C, I2C, D, S, RNG>(
    controller: C,
    rng: &mut RNG,
    mut button_a: Input<'d>,
    mut button_b: Input<'d>,
    mut joyc: MiniJoyC<I2C>,
    display: &mut D,
    mut backlight: Output<'d>,
    storage: &mut S,
) where
    C: Controller,
    I2C: I2cTrait,
    I2C::Error: Debug,
    D: DrawTarget<Color = Rgb565>,
    D::Error: Debug,
    S: NorFlash,
    RNG: rand_core::RngCore + rand_core::CryptoRng,
{
    let address = Address::random([0x42, 0x11, 0x22, 0x33, 0x44, 0xc0]);

    println!("BLE address: {:?}", address);

    let mut resources: HostResources<DefaultPacketPool, CONNECTIONS_MAX, L2CAP_CHANNELS_MAX> =
        HostResources::new();

    let builder = trouble_host::new(controller, &mut resources)
        .set_random_address(address)
        .set_random_generator_seed(rng);

    let bonds = bond_store::load_all::<_, BONDS_MAX>(storage).await;

    if !bonds.is_empty() {
        println!("loaded {} bond information", bonds.len());

        for bond in bonds {
            builder.add_bond_information(bond).unwrap();
        }
    } else {
        println!("no bond information");
    }

    let stack = builder.build();

    let mut runner = stack.runner;

    let mut peripheral = stack.peripheral;

    let server = Server::new_with_config(GapConfig::Peripheral(PeripheralConfig {
        name: DEVICE_NAME,
        appearance: &appearance::human_interface_device::GENERIC_HUMAN_INTERFACE_DEVICE,
    }))
    .unwrap();

    println!("GATT server initialized");

    let mut profile_manager = ProfileManager::new();

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
            backlight.set_high();
            display::render(
                display,
                Status::Waiting,
                profile_manager.current().label,
                battery::percent(),
                None,
            )
            .unwrap();

            let conn = match advertise(&mut peripheral, &server).await {
                Ok(conn) => conn,

                Err(err) => {
                    println!("advertising error: {:?}", err);

                    continue;
                }
            };

            println!("connection established");

            MOUSE_NOTIFY_ENABLED.store(false, Ordering::Release);
            KEYBOARD_NOTIFY_ENABLED.store(false, Ordering::Release);

            let mut peer_address = heapless::String::<24>::new();

            format_peer_address(&mut peer_address, conn.raw().peer_identity().bd_addr);

            display::render(
                display,
                Status::Connected,
                profile_manager.current().label,
                battery::percent(),
                Some(&peer_address),
            )
            .unwrap();

            let battery_level = battery::percent();

            server
                .set(&server.battery_service.level, &battery_level)
                .unwrap();

            conn.raw().set_bondable(true).unwrap();

            let gatt = gatt_events_task(storage, &server, &conn);

            let input = input_task(
                &server,
                &conn,
                &mut button_a,
                &mut button_b,
                &mut joyc,
                &mut backlight,
                &mut profile_manager,
                display,
            );

            let battery_service = battery_service_task(&server, &conn);

            select4(gatt, input, battery_service, core::future::pending::<()>()).await;

            println!("connection ended");
            display::render(
                display,
                Status::Waiting,
                profile_manager.current().label,
                battery::percent(),
                None,
            )
            .unwrap();
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

fn format_peer_address(output: &mut heapless::String<24>, address: BdAddr) {
    let raw = address.raw();

    write!(
        output,
        "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        raw[5], raw[4], raw[3], raw[2], raw[1], raw[0],
    )
    .unwrap();
}

async fn battery_service_task<P: PacketPool>(
    server: &Server<'_>,
    conn: &GattConnection<'_, '_, P>,
) {
    let characteristic = server.battery_service.level;

    let mut previous = u8::MAX;

    loop {
        let level = crate::battery::percent();

        if level != previous {
            server.set(&characteristic, &level).unwrap();

            let _ = characteristic.notify(conn, &level).await;

            previous = level;

            println!("battery={}%", level);
        }

        Timer::after(Duration::from_secs(5)).await;
    }
}

async fn gatt_events_task<P: PacketPool, S: NorFlash>(
    storage: &mut S,
    server: &Server<'_>,
    conn: &GattConnection<'_, '_, P>,
) {
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

            GattConnectionEvent::PairingComplete {
                security_level,
                bond,
            } => {
                println!("pairing complete: {:?}", security_level);

                if let Some(bond) = bond {
                    match bond_store::store(storage, &bond).await {
                        Ok(()) => {
                            println!("bond information stored");
                        }

                        Err(err) => {
                            println!("failed to store bond: {:?}", err);
                        }
                    }
                }
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

async fn input_task<P, I2C, D>(
    server: &Server<'_>,
    conn: &GattConnection<'_, '_, P>,
    button_a: &mut Input<'_>,
    button_b: &mut Input<'_>,
    joyc: &mut MiniJoyC<I2C>,
    backlight: &mut Output<'_>,
    profiles: &mut ProfileManager,
    display: &mut D,
) where
    P: PacketPool,
    I2C: I2cTrait,
    I2C::Error: Debug,
    D: DrawTarget<Color = Rgb565>,
    D::Error: Debug,
{
    while !MOUSE_NOTIFY_ENABLED.load(Ordering::Acquire)
        && !KEYBOARD_NOTIFY_ENABLED.load(Ordering::Acquire)
    {
        Timer::after(Duration::from_millis(50)).await;
    }

    println!("input ready");

    let mouse_report = server.hid_service.mouse_report;
    let keyboard_report = server.hid_service.keyboard_report;
    let mut mapper = MouseMapper::new();
    let mut input = InputEngine::new();
    let mut previous_mouse_buttons = 0u8;
    let mut now_ms = 0u64;
    let mut last_activity_ms = 0u64;
    let mut display_awake = true;

    loop {
        let profile = profiles.current();

        match joyc.read() {
            Ok(joy) => {
                let sample = InputSample {
                    buttons: InputState {
                        button_a: button_a.is_low(),
                        button_b: button_b.is_low(),
                        joy_click: joy.pressed,
                    },
                    joy,
                };

                let outcome = input.update(sample, &profile.input, &profile.scroll, now_ms);

                let mut activity = sample.buttons.button_mask() != 0;

                if let Some(event) = outcome.event {
                    activity = true;
                    match event.action {
                        Action::KeyboardKey(key) => {
                            send_key(&keyboard_report, conn, key).await;
                        }

                        Action::MouseButton(button) => {
                            send_mouse_click(&mouse_report, conn, button).await;
                            previous_mouse_buttons = 0;
                        }

                        Action::NextProfile => {
                            let profile = profiles.next();

                            println!("profile={}", profile.label);

                            if !display_awake {
                                backlight.set_high();
                                display_awake = true;
                            }

                            display::render(
                                display,
                                Status::Connected,
                                profile.label,
                                battery::percent(),
                                None,
                            )
                            .unwrap();
                        }

                        Action::ScrollMode | Action::None => {}
                    }
                }

                if outcome.scroll_mode {
                    let wheel = mapper.update_scroll(joy, &profile.scroll, profile.mouse.poll_hz);

                    if wheel != 0 {
                        activity = true;
                        send_mouse_report(&mouse_report, conn, previous_mouse_buttons, 0, 0, wheel)
                            .await;
                    }
                } else {
                    let pointer = mapper.update_pointer(joy, &profile.mouse, profile.orientation);

                    if pointer.dx != 0 || pointer.dy != 0 || previous_mouse_buttons != 0 {
                        activity = true;
                        send_mouse_report(
                            &mouse_report,
                            conn,
                            previous_mouse_buttons,
                            pointer.dx,
                            pointer.dy,
                            0,
                        )
                        .await;
                    }
                }

                if activity {
                    last_activity_ms = now_ms;

                    if !display_awake {
                        backlight.set_high();
                        display_awake = true;
                        display::render(
                            display,
                            Status::Connected,
                            profiles.current().label,
                            battery::percent(),
                            None,
                        )
                        .unwrap();
                    }
                } else if display_awake
                    && now_ms.saturating_sub(last_activity_ms)
                        >= DISPLAY_TIMEOUT_SECONDS.saturating_mul(1_000)
                {
                    backlight.set_low();
                    display_awake = false;
                }

                now_ms = now_ms.saturating_add(profile.mouse.poll_interval_us / 1_000);
                Timer::after(Duration::from_micros(profile.mouse.poll_interval_us)).await;
            }

            Err(err) => {
                println!("Mini JoyC error: {:?}", err);
                Timer::after(Duration::from_millis(50)).await;
            }
        }
    }
}

async fn send_key<P: PacketPool>(
    keyboard: &Characteristic<[u8; 8]>,
    conn: &GattConnection<'_, '_, P>,
    key_code: u8,
) {
    if !KEYBOARD_NOTIFY_ENABLED.load(Ordering::Acquire) {
        return;
    }

    let down = [0x00, 0x00, key_code, 0, 0, 0, 0, 0];

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

async fn send_mouse_click<P: PacketPool>(
    mouse: &Characteristic<[u8; 4]>,
    conn: &GattConnection<'_, '_, P>,
    button: u8,
) {
    let button = button & (MOUSE_BUTTON_LEFT | MOUSE_BUTTON_RIGHT | MOUSE_BUTTON_MIDDLE);

    if button == 0 {
        return;
    }

    send_mouse_report(mouse, conn, button, 0, 0, 0).await;
    Timer::after(Duration::from_millis(KEY_PRESS_DURATION_MS)).await;
    send_mouse_report(mouse, conn, 0, 0, 0, 0).await;
}

async fn send_mouse_report<P: PacketPool>(
    mouse: &Characteristic<[u8; 4]>,
    conn: &GattConnection<'_, '_, P>,
    buttons: u8,
    dx: i8,
    dy: i8,
    wheel: i8,
) {
    if !MOUSE_NOTIFY_ENABLED.load(Ordering::Acquire) {
        return;
    }

    let report = [buttons, dx as u8, dy as u8, wheel as u8];

    if let Err(err) = mouse.notify(conn, &report).await {
        println!("mouse notify error: {:?}", err);
    }
}
