use crate::{
    config::{MouseConfig, Orientation, ScrollConfig},
    mini_joyc::JoyState,
};

const Q8_ONE: i32 = 256;
const Q15_ONE: i64 = 32768;
const Q16_ONE: i64 = 65536;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PointerState {
    pub dx: i8,
    pub dy: i8,
}

pub struct MouseMapper {
    filtered_x_q8: i32,
    filtered_y_q8: i32,

    speed_gain_q15: i64,

    residual_x_q16: i64,
    residual_y_q16: i64,
    residual_wheel_q16: i64,
}

impl MouseMapper {
    pub fn new() -> Self {
        Self {
            filtered_x_q8: 0,
            filtered_y_q8: 0,
            speed_gain_q15: Q15_ONE,
            residual_x_q16: 0,
            residual_y_q16: 0,
            residual_wheel_q16: 0,
        }
    }

    pub fn update_pointer(
        &mut self,
        state: JoyState,
        mouse: &MouseConfig,
        orientation: Orientation,
    ) -> PointerState {
        update_filter(&mut self.filtered_x_q8, state.x, mouse.smoothing_q15);
        update_filter(&mut self.filtered_y_q8, state.y, mouse.smoothing_q15);

        let (x_q8, y_q8) = rotate(self.filtered_x_q8, self.filtered_y_q8, orientation);
        let shaped_x_q15 = axis_shaped_q15(
            x_q8,
            mouse.dead_zone,
            mouse.curve_weight_q15,
            mouse.invert_x,
        );
        let shaped_y_q15 = axis_shaped_q15(
            y_q8,
            mouse.dead_zone,
            mouse.curve_weight_q15,
            mouse.invert_y,
        );

        update_speed_gain(
            &mut self.speed_gain_q15,
            shaped_x_q15 != 0 || shaped_y_q15 != 0,
            mouse,
        );

        let velocity_x_q16 = axis_velocity_q16(shaped_x_q15, self.speed_gain_q15, mouse);
        let velocity_y_q16 = axis_velocity_q16(shaped_y_q15, self.speed_gain_q15, mouse);

        self.residual_x_q16 += velocity_x_q16 / mouse.poll_hz;
        self.residual_y_q16 += velocity_y_q16 / mouse.poll_hz;

        PointerState {
            dx: take_delta(&mut self.residual_x_q16),
            dy: take_delta(&mut self.residual_y_q16),
        }
    }

    pub fn update_scroll(&mut self, state: JoyState, scroll: &ScrollConfig, poll_hz: i64) -> i8 {
        let mut axis = i32::from(state.y);

        if scroll.invert_vertical {
            axis = -axis;
        }

        let magnitude = axis.abs();

        if magnitude <= scroll.dead_zone {
            return 0;
        }

        let sign = if axis < 0 { -1 } else { 1 };
        let range = 127 - scroll.dead_zone;
        let normalized_q16 = i64::from(magnitude - scroll.dead_zone) * Q16_ONE / i64::from(range);
        let velocity_q16 = normalized_q16 * i64::from(scroll.speed) * i64::from(sign);

        self.residual_wheel_q16 += velocity_q16 / poll_hz;

        take_delta(&mut self.residual_wheel_q16)
    }
}

fn update_filter(current_q8: &mut i32, sample: i8, smoothing_q15: i32) {
    let target_q8 = i32::from(sample) * Q8_ONE;
    let difference = target_q8 - *current_q8;

    *current_q8 += (i64::from(difference) * i64::from(smoothing_q15) / Q15_ONE) as i32;
}

fn update_speed_gain(speed_gain_q15: &mut i64, active: bool, mouse: &MouseConfig) {
    let max_gain_q15 =
        i64::from(mouse.max_speed_px_per_sec) * Q15_ONE / i64::from(mouse.base_speed_px_per_sec);

    if active {
        *speed_gain_q15 += i64::from(mouse.gain_rise_q15_per_sec) / mouse.poll_hz;
        *speed_gain_q15 = (*speed_gain_q15).clamp(Q15_ONE, max_gain_q15);
    } else {
        *speed_gain_q15 -= i64::from(mouse.gain_fall_q15_per_sec) / mouse.poll_hz;
        *speed_gain_q15 = (*speed_gain_q15).max(Q15_ONE);
    }
}

fn rotate(x: i32, y: i32, orientation: Orientation) -> (i32, i32) {
    match orientation {
        Orientation::Normal => (x, y),
        Orientation::Right => (-y, x),
        Orientation::Inverted => (-x, -y),
        Orientation::Left => (y, -x),
    }
}

fn axis_shaped_q15(value_q8: i32, dead_zone: i32, curve_weight_q15: i32, invert: bool) -> i64 {
    let mut value = i64::from(value_q8);

    if invert {
        value = -value;
    }

    let magnitude = value.abs();
    let dead_zone = i64::from(dead_zone) * i64::from(Q8_ONE);

    if magnitude <= dead_zone {
        return 0;
    }

    let sign = if value < 0 { -1 } else { 1 };
    let range = i64::from(127) * i64::from(Q8_ONE) - dead_zone;
    let normalized_q15 = ((magnitude - dead_zone) * Q15_ONE / range).clamp(0, Q15_ONE);
    let squared_q15 = normalized_q15 * normalized_q15 / Q15_ONE;
    let weight = i64::from(curve_weight_q15);
    let shaped_q15 = ((Q15_ONE - weight) * normalized_q15 + weight * squared_q15) / Q15_ONE;

    sign * shaped_q15
}

fn axis_velocity_q16(shaped_q15: i64, speed_gain_q15: i64, mouse: &MouseConfig) -> i64 {
    let velocity_q16 =
        shaped_q15 * speed_gain_q15 * i64::from(mouse.base_speed_px_per_sec) * 2 / Q15_ONE;

    let max_velocity_q16 = i64::from(mouse.max_speed_px_per_sec) * Q16_ONE;

    velocity_q16.clamp(-max_velocity_q16, max_velocity_q16)
}

fn take_delta(residual_q16: &mut i64) -> i8 {
    let whole = *residual_q16 / Q16_ONE;
    let delta = whole.clamp(-127, 127);

    *residual_q16 -= delta * Q16_ONE;

    delta as i8
}
