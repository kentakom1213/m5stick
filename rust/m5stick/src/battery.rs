use core::sync::atomic::{AtomicU8, AtomicU16, Ordering};

use crate::config::{BATTERY_EMPTY_MV, BATTERY_FULL_MV, BATTERY_RAW_EMPTY, BATTERY_RAW_FULL};

static PERCENT: AtomicU8 = AtomicU8::new(0);

static MILLIVOLTS: AtomicU16 = AtomicU16::new(0);

pub fn update(raw: u16) {
    let raw = i32::from(raw);

    let raw_empty = i32::from(BATTERY_RAW_EMPTY);

    let raw_full = i32::from(BATTERY_RAW_FULL);

    let denominator = raw_full - raw_empty;

    let numerator = (raw - raw_empty).clamp(0, denominator);

    let percent = numerator * 100 / denominator;

    let mv = i32::from(BATTERY_EMPTY_MV)
        + numerator * (i32::from(BATTERY_FULL_MV) - i32::from(BATTERY_EMPTY_MV)) / denominator;

    PERCENT.store(percent as u8, Ordering::Relaxed);

    MILLIVOLTS.store(mv as u16, Ordering::Relaxed);
}

pub fn percent() -> u8 {
    PERCENT.load(Ordering::Relaxed)
}

pub fn millivolts() -> u16 {
    MILLIVOLTS.load(Ordering::Relaxed)
}
