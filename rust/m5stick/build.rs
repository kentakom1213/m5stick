use serde::Deserialize;
use std::{collections::BTreeMap, env, fs, path::PathBuf};

#[derive(Debug, Deserialize)]
struct Config {
    general: GeneralConfig,
    #[allow(dead_code)]
    display: DisplayConfig,
    input: InputConfig,
    scroll: ScrollConfig,
    profiles: BTreeMap<String, ProfileConfig>,
    battery: BatteryConfig,
}

#[derive(Debug, Deserialize)]
struct GeneralConfig {
    default_profile: String,
    profile_order: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct DisplayConfig {
    timeout_seconds: u64,
    brightness: u8,
    show_battery: bool,
    show_connection: bool,
    show_profile: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct InputConfig {
    combo_window_ms: u64,
    tap_max_ms: u64,
    default_hold_ms: u64,
    button_a: ButtonInputConfig,
    button_b: ButtonInputConfig,
    joy_click: ButtonInputConfig,
    #[serde(default)]
    combo: Vec<ComboConfig>,
}

#[derive(Debug, Clone, Deserialize)]
struct ButtonInputConfig {
    tap: Option<String>,
    hold: Option<String>,
    hold_ms: Option<u64>,
    hold_joy: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ComboConfig {
    buttons: Vec<String>,
    action: String,
    hold_ms: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
struct ScrollConfig {
    dead_zone: i32,
    speed: i32,
    horizontal: bool,
    invert_vertical: bool,
    invert_horizontal: bool,
}

#[derive(Debug, Deserialize)]
struct ProfileConfig {
    label: String,
    orientation: String,
    mouse: MouseConfig,
    input: Option<InputConfig>,
    scroll: Option<ScrollConfig>,
}

#[derive(Debug, Deserialize)]
struct MouseConfig {
    poll_hz: u64,
    dead_zone: i32,
    base_speed_px_per_sec: f64,
    max_speed_px_per_sec: f64,
    gain_rise_per_sec: f64,
    gain_fall_per_sec: f64,
    curve_weight: f64,
    smoothing: f64,
    invert_x: bool,
    invert_y: bool,
}

#[derive(Debug, Deserialize)]
struct BatteryConfig {
    raw_empty: u16,
    raw_full: u16,
    empty_mv: u16,
    full_mv: u16,
    poll_seconds: u64,
}

fn main() {
    // esp-generate の Xtensa 用処理
    linker_be_nice();
    check_xtensa_linker_available();

    // presenter.toml -> Rust 定数
    generate_presenter_config();

    // 重要:
    // linkall.x は最後の linker script にする．
    println!("cargo:rustc-link-arg=-Tlinkall.x");
}

fn generate_presenter_config() {
    println!("cargo:rerun-if-changed=presenter.toml");

    let source = fs::read_to_string("presenter.toml").expect("failed to read presenter.toml");

    let config: Config = toml::from_str(&source).expect("invalid presenter.toml");

    validate_display(&config.display);
    validate_battery(&config.battery);
    validate_input(&config.input);
    validate_scroll(&config.scroll, "scroll");
    validate_profiles(&config);

    let default_profile = config
        .profiles
        .get(&config.general.default_profile)
        .expect("general.default_profile must exist in profiles");

    let default_input = default_profile.input.as_ref().unwrap_or(&config.input);
    let default_scroll = default_profile.scroll.as_ref().unwrap_or(&config.scroll);
    let mouse = &default_profile.mouse;
    let battery = &config.battery;

    validate_mouse(
        mouse,
        &format!("profiles.{}.mouse", config.general.default_profile),
    );

    let interval_us = 1_000_000 / mouse.poll_hz;

    // Q15 fixed-point.
    //
    // presenter.toml では 0.0..1.0 の小数で指定するが，
    // ESP32 上では浮動小数点を使わない．
    let curve_weight_q15 = (mouse.curve_weight * 32768.0).round() as i32;
    let smoothing_q15 = (mouse.smoothing * 32768.0).round() as i32;
    let base_speed = mouse.base_speed_px_per_sec.round() as i32;
    let max_speed = mouse.max_speed_px_per_sec.round() as i32;
    let gain_rise_q15 = (mouse.gain_rise_per_sec * 32768.0).round() as i32;
    let gain_fall_q15 = (mouse.gain_fall_per_sec * 32768.0).round() as i32;

    let button_a_key = action_keyboard_usage(default_input.button_a.tap.as_deref())
        .expect("input.button_a.tap must be a keyboard action");
    let button_b_key = action_keyboard_usage(default_input.button_b.tap.as_deref())
        .expect("input.button_b.tap must be a keyboard action");

    let default_profile_index = config
        .general
        .profile_order
        .iter()
        .position(|name| name == &config.general.default_profile)
        .expect("general.default_profile must exist in general.profile_order");

    let profile_names = config
        .general
        .profile_order
        .iter()
        .map(|name| format!("    {:?}", name))
        .collect::<Vec<_>>()
        .join(",\n");

    let profile_labels = config
        .general
        .profile_order
        .iter()
        .map(|name| {
            let profile = config
                .profiles
                .get(name)
                .expect("profile_order entry missing");
            format!("    {:?}", profile.label)
        })
        .collect::<Vec<_>>()
        .join(",\n");

    let generated = format!(
        r#"
// This file is automatically generated by build.rs.
// Do not edit manually.

pub const PROFILE_COUNT: usize = {profile_count};
pub const DEFAULT_PROFILE_INDEX: usize = {default_profile_index};
pub const PROFILE_NAMES: [&str; PROFILE_COUNT] = [
{profile_names}
];
pub const PROFILE_LABELS: [&str; PROFILE_COUNT] = [
{profile_labels}
];

pub const MOUSE_POLL_HZ: i64 = {poll_hz};
pub const MOUSE_POLL_INTERVAL_US: u64 = {interval_us};

pub const MOUSE_DEAD_ZONE: i32 = {dead_zone};
pub const MOUSE_BASE_SPEED_PX_PER_SEC: i32 = {base_speed};
pub const MOUSE_MAX_SPEED_PX_PER_SEC: i32 = {max_speed};
pub const MOUSE_GAIN_RISE_Q15_PER_SEC: i32 = {gain_rise_q15};
pub const MOUSE_GAIN_FALL_Q15_PER_SEC: i32 = {gain_fall_q15};

pub const MOUSE_CURVE_WEIGHT_Q15: i32 = {curve_weight_q15};
pub const MOUSE_SMOOTHING_Q15: i32 = {smoothing_q15};

pub const MOUSE_INVERT_X: bool = {invert_x};
pub const MOUSE_INVERT_Y: bool = {invert_y};

pub const BUTTON_A_KEY: u8 = {button_a_key:#04x};
pub const BUTTON_B_KEY: u8 = {button_b_key:#04x};

pub const SCROLL_DEAD_ZONE: i32 = {scroll_dead_zone};
pub const SCROLL_SPEED: i32 = {scroll_speed};
pub const SCROLL_HORIZONTAL: bool = {scroll_horizontal};
pub const SCROLL_INVERT_VERTICAL: bool = {scroll_invert_vertical};
pub const SCROLL_INVERT_HORIZONTAL: bool = {scroll_invert_horizontal};

pub const INPUT_COMBO_WINDOW_MS: u64 = {combo_window_ms};
pub const INPUT_TAP_MAX_MS: u64 = {tap_max_ms};
pub const INPUT_DEFAULT_HOLD_MS: u64 = {default_hold_ms};

pub const BATTERY_RAW_EMPTY: u16 = {battery_raw_empty};
pub const BATTERY_RAW_FULL: u16 = {battery_raw_full};

pub const BATTERY_EMPTY_MV: u16 = {battery_empty_mv};
pub const BATTERY_FULL_MV: u16 = {battery_full_mv};

pub const BATTERY_POLL_SECONDS: u64 = {battery_poll_seconds};
"#,
        profile_count = config.general.profile_order.len(),
        default_profile_index = default_profile_index,
        profile_names = profile_names,
        profile_labels = profile_labels,
        poll_hz = mouse.poll_hz,
        interval_us = interval_us,
        dead_zone = mouse.dead_zone,
        base_speed = base_speed,
        max_speed = max_speed,
        gain_rise_q15 = gain_rise_q15,
        gain_fall_q15 = gain_fall_q15,
        curve_weight_q15 = curve_weight_q15,
        smoothing_q15 = smoothing_q15,
        invert_x = mouse.invert_x,
        invert_y = mouse.invert_y,
        button_a_key = button_a_key,
        button_b_key = button_b_key,
        scroll_dead_zone = default_scroll.dead_zone,
        scroll_speed = default_scroll.speed,
        scroll_horizontal = default_scroll.horizontal,
        scroll_invert_vertical = default_scroll.invert_vertical,
        scroll_invert_horizontal = default_scroll.invert_horizontal,
        combo_window_ms = default_input.combo_window_ms,
        tap_max_ms = default_input.tap_max_ms,
        default_hold_ms = default_input.default_hold_ms,
        battery_raw_empty = battery.raw_empty,
        battery_raw_full = battery.raw_full,
        battery_empty_mv = battery.empty_mv,
        battery_full_mv = battery.full_mv,
        battery_poll_seconds = battery.poll_seconds,
    );

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR is not set"));
    let output = out_dir.join("presenter_config.rs");

    fs::write(&output, generated).expect("failed to write presenter_config.rs");

    println!(
        "cargo:warning=generated presenter config: {}",
        output.display()
    );
}

fn validate_profiles(config: &Config) {
    assert!(
        !config.general.profile_order.is_empty(),
        "general.profile_order must not be empty"
    );

    assert!(
        config
            .profiles
            .contains_key(&config.general.default_profile),
        "general.default_profile must exist in profiles"
    );

    for name in &config.general.profile_order {
        let profile = config
            .profiles
            .get(name)
            .unwrap_or_else(|| panic!("profile_order entry `{name}` is not defined in profiles"));

        validate_orientation(
            &profile.orientation,
            &format!("profiles.{name}.orientation"),
        );
        validate_mouse(&profile.mouse, &format!("profiles.{name}.mouse"));

        if let Some(input) = &profile.input {
            validate_input(input);
        }

        if let Some(scroll) = &profile.scroll {
            validate_scroll(scroll, &format!("profiles.{name}.scroll"));
        }
    }
}

fn validate_display(display: &DisplayConfig) {
    assert!(
        display.timeout_seconds > 0,
        "display.timeout_seconds must be greater than 0"
    );

    assert!(
        display.brightness <= 100,
        "display.brightness must be in 0..=100"
    );

    let _ = (
        display.show_battery,
        display.show_connection,
        display.show_profile,
    );
}

fn validate_battery(battery: &BatteryConfig) {
    assert!(
        battery.raw_full > battery.raw_empty,
        "battery.raw_full must be greater than battery.raw_empty"
    );

    assert!(
        battery.full_mv > battery.empty_mv,
        "battery.full_mv must be greater than battery.empty_mv"
    );

    assert!(
        battery.poll_seconds > 0,
        "battery.poll_seconds must be greater than 0"
    );
}

fn validate_input(input: &InputConfig) {
    assert!(
        input.combo_window_ms > 0,
        "input.combo_window_ms must be greater than 0"
    );

    assert!(
        input.tap_max_ms > 0,
        "input.tap_max_ms must be greater than 0"
    );

    assert!(
        input.default_hold_ms > 0,
        "input.default_hold_ms must be greater than 0"
    );

    validate_button_input(&input.button_a, "input.button_a");
    validate_button_input(&input.button_b, "input.button_b");
    validate_button_input(&input.joy_click, "input.joy_click");

    for combo in &input.combo {
        assert!(
            combo.buttons.len() >= 2,
            "combo must contain at least 2 buttons"
        );

        for button in &combo.buttons {
            validate_button_id(button);
        }

        validate_action(&combo.action);

        if let Some(hold_ms) = combo.hold_ms {
            assert!(hold_ms > 0, "combo.hold_ms must be greater than 0");
        }
    }
}

fn validate_button_input(input: &ButtonInputConfig, name: &str) {
    if let Some(action) = &input.tap {
        validate_action(action);
    }

    if let Some(action) = &input.hold {
        validate_action(action);
    }

    if let Some(hold_ms) = input.hold_ms {
        assert!(hold_ms > 0, "{name}.hold_ms must be greater than 0");
    }

    if let Some(action) = &input.hold_joy {
        validate_action(action);
    }
}

fn validate_scroll(scroll: &ScrollConfig, name: &str) {
    assert!(
        (0..127).contains(&scroll.dead_zone),
        "{name}.dead_zone must be in 0..127"
    );

    assert!(scroll.speed > 0, "{name}.speed must be greater than 0");
}

fn validate_mouse(mouse: &MouseConfig, name: &str) {
    assert!(mouse.poll_hz > 0, "{name}.poll_hz must be greater than 0");

    assert!(
        (0..127).contains(&mouse.dead_zone),
        "{name}.dead_zone must be in 0..127"
    );

    assert!(
        mouse.base_speed_px_per_sec > 0.0,
        "{name}.base_speed_px_per_sec must be greater than 0"
    );

    assert!(
        mouse.max_speed_px_per_sec >= mouse.base_speed_px_per_sec,
        "{name}.max_speed_px_per_sec must be greater than or equal to base speed"
    );

    assert!(
        mouse.gain_rise_per_sec > 0.0,
        "{name}.gain_rise_per_sec must be greater than 0"
    );

    assert!(
        mouse.gain_fall_per_sec > 0.0,
        "{name}.gain_fall_per_sec must be greater than 0"
    );

    assert!(
        (0.0..=1.0).contains(&mouse.curve_weight),
        "{name}.curve_weight must be in 0.0..=1.0"
    );

    assert!(
        (0.0..=1.0).contains(&mouse.smoothing),
        "{name}.smoothing must be in 0.0..=1.0"
    );
}

fn validate_orientation(value: &str, name: &str) {
    match value {
        "normal" | "right" | "inverted" | "left" => {}
        other => panic!("unknown {name}: {other}"),
    }
}

fn validate_button_id(value: &str) {
    match value {
        "button_a" | "button_b" | "joy_click" => {}
        other => panic!("unknown combo button: {other}"),
    }
}

fn validate_action(value: &str) {
    match value {
        "RightArrow" | "LeftArrow" | "PageDown" | "PageUp" | "Space" | "Enter" | "Escape"
        | "F5" | "MouseLeft" | "MouseRight" | "MouseMiddle" | "Scroll" | "NextProfile" | "None" => {
        }
        other => panic!("unknown action: {other}"),
    }
}

fn action_keyboard_usage(action: Option<&str>) -> Option<u8> {
    match action? {
        "RightArrow" => Some(0x4f),
        "LeftArrow" => Some(0x50),
        "PageDown" => Some(0x4e),
        "PageUp" => Some(0x4b),
        "Space" => Some(0x2c),
        "Enter" => Some(0x28),
        "Escape" => Some(0x29),
        "F5" => Some(0x3e),
        _ => None,
    }
}

// -----------------------------------------------------------------------------
// ESP32 / Xtensa linker support
//
// The functions below are based on the esp-generate Xtensa project template.
// -----------------------------------------------------------------------------

#[cfg(unix)]
fn check_xtensa_linker_available() {
    println!("cargo:rerun-if-env-changed=PATH");

    let target = env::var("TARGET").unwrap_or_default();

    println!(
        "cargo:rerun-if-env-changed={}",
        cargo_linker_env_var(&target)
    );

    let linker = xtensa_linker(&target);

    let error = match std::process::Command::new(&linker)
        .arg("--version")
        .output()
    {
        Ok(_) => return,
        Err(error) => error,
    };

    let export_file = env::var("HOME")
        .map(|home| format!("{home}/export-esp.sh"))
        .unwrap_or_else(|_| "$HOME/export-esp.sh".to_string());

    panic!(
        "Xtensa linker `{linker}` was not found in PATH \
         or could not be executed: {error}.\n\n\
         Xtensa targets on Unix require espup's environment \
         export file to be sourced before building.\n\
         Try: `source {export_file}`\n\n\
         If the export file does not exist, \
         run `espup install` first."
    );
}

#[cfg(not(unix))]
fn check_xtensa_linker_available() {}

#[cfg(unix)]
fn xtensa_linker(target: &str) -> String {
    if let Some(linker) = cargo_configured_linker(target) {
        return linker;
    }

    target
        .strip_prefix("xtensa-")
        .and_then(|target| target.strip_suffix("-none-elf"))
        .map(|chip| format!("xtensa-{chip}-elf-gcc"))
        .unwrap_or_else(|| "xtensa-esp32-elf-gcc".to_string())
}

#[cfg(unix)]
fn cargo_configured_linker(target: &str) -> Option<String> {
    env::var(cargo_linker_env_var(target)).ok()
}

#[cfg(unix)]
fn cargo_linker_env_var(target: &str) -> String {
    let target = target.replace('-', "_").to_ascii_uppercase();

    format!("CARGO_TARGET_{target}_LINKER")
}

fn linker_be_nice() {
    let args: Vec<String> = env::args().collect();

    if args.len() > 2 {
        let kind = &args[1];
        let what = &args[2];

        match kind.as_str() {
            "undefined-symbol" => match what.as_str() {
                "_stack_start" => {
                    eprintln!();
                    eprintln!("linkall.x may be missing");
                    eprintln!();
                }

                what if what.starts_with("esp_rtos_") => {
                    eprintln!();
                    eprintln!(
                        "esp-radio has no scheduler enabled. \
                             Make sure esp-rtos is initialized."
                    );
                    eprintln!();
                }

                "free"
                | "malloc"
                | "calloc"
                | "get_free_internal_heap_size"
                | "malloc_internal"
                | "realloc_internal"
                | "calloc_internal"
                | "free_internal" => {
                    eprintln!();
                    eprintln!(
                        "esp-alloc may be missing \
                             or incorrectly configured"
                    );
                    eprintln!();
                }

                _ => {}
            },

            _ => {
                std::process::exit(1);
            }
        }

        std::process::exit(0);
    }

    // Xtensa linker の診断をこの build.rs へ戻す．
    println!(
        "cargo:rustc-link-arg=-Wl,--error-handling-script={}",
        env::current_exe().unwrap().display()
    );
}
