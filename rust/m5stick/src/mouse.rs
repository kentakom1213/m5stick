use crate::{
    config::{
        MOUSE_BASE_SPEED_PX_PER_SEC, MOUSE_CURVE_WEIGHT_Q15, MOUSE_DEAD_ZONE,
        MOUSE_GAIN_FALL_Q15_PER_SEC, MOUSE_GAIN_RISE_Q15_PER_SEC, MOUSE_INVERT_X, MOUSE_INVERT_Y,
        MOUSE_MAX_SPEED_PX_PER_SEC, MOUSE_POLL_HZ, MOUSE_SMOOTHING_Q15,
    },
    mini_joyc::JoyState,
};

const Q8_ONE: i32 = 256;
const Q15_ONE: i64 = 32768;
const Q16_ONE: i64 = 65536;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseState {
    pub dx: i8,
    pub dy: i8,
    pub left_pressed: bool,
}

pub struct MouseMapper {
    filtered_x_q8: i32,
    filtered_y_q8: i32,

    speed_gain_q15: i64,

    residual_x_q16: i64,
    residual_y_q16: i64,
}

impl MouseMapper {
    pub fn new() -> Self {
        Self {
            filtered_x_q8: 0,
            filtered_y_q8: 0,
            speed_gain_q15: Q15_ONE,
            residual_x_q16: 0,
            residual_y_q16: 0,
        }
    }

    pub fn update(&mut self, state: JoyState) -> MouseState {
        update_filter(&mut self.filtered_x_q8, state.x);

        update_filter(&mut self.filtered_y_q8, state.y);

        let shaped_x_q15 = axis_shaped_q15(self.filtered_x_q8, MOUSE_INVERT_X);

        let shaped_y_q15 = axis_shaped_q15(self.filtered_y_q8, MOUSE_INVERT_Y);

        update_speed_gain(
            &mut self.speed_gain_q15,
            shaped_x_q15 != 0 || shaped_y_q15 != 0,
        );

        let velocity_x_q16 = axis_velocity_q16(shaped_x_q15, self.speed_gain_q15);

        let velocity_y_q16 = axis_velocity_q16(shaped_y_q15, self.speed_gain_q15);

        self.residual_x_q16 += velocity_x_q16 / MOUSE_POLL_HZ;

        self.residual_y_q16 += velocity_y_q16 / MOUSE_POLL_HZ;

        MouseState {
            dx: take_delta(&mut self.residual_x_q16),
            dy: take_delta(&mut self.residual_y_q16),
            left_pressed: state.pressed,
        }
    }
}

fn update_filter(current_q8: &mut i32, sample: i8) {
    let target_q8 = i32::from(sample) * Q8_ONE;

    let difference = target_q8 - *current_q8;

    *current_q8 += (i64::from(difference) * i64::from(MOUSE_SMOOTHING_Q15) / Q15_ONE) as i32;
}

fn update_speed_gain(speed_gain_q15: &mut i64, active: bool) {
    let max_gain_q15 =
        i64::from(MOUSE_MAX_SPEED_PX_PER_SEC) * Q15_ONE / i64::from(MOUSE_BASE_SPEED_PX_PER_SEC);

    if active {
        *speed_gain_q15 += i64::from(MOUSE_GAIN_RISE_Q15_PER_SEC) / MOUSE_POLL_HZ;
        *speed_gain_q15 = (*speed_gain_q15).clamp(Q15_ONE, max_gain_q15);
    } else {
        *speed_gain_q15 -= i64::from(MOUSE_GAIN_FALL_Q15_PER_SEC) / MOUSE_POLL_HZ;
        *speed_gain_q15 = (*speed_gain_q15).max(Q15_ONE);
    }
}

fn axis_shaped_q15(value_q8: i32, invert: bool) -> i64 {
    let mut value = i64::from(value_q8);

    if invert {
        value = -value;
    }

    let magnitude = value.abs();

    let dead_zone = i64::from(MOUSE_DEAD_ZONE) * i64::from(Q8_ONE);

    if magnitude <= dead_zone {
        return 0;
    }

    let sign = if value < 0 { -1 } else { 1 };

    let range = i64::from(127 - MOUSE_DEAD_ZONE) * i64::from(Q8_ONE);

    let normalized_q15 = ((magnitude - dead_zone) * Q15_ONE / range).clamp(0, Q15_ONE);

    let squared_q15 = normalized_q15 * normalized_q15 / Q15_ONE;

    let weight = i64::from(MOUSE_CURVE_WEIGHT_Q15);

    let shaped_q15 = ((Q15_ONE - weight) * normalized_q15 + weight * squared_q15) / Q15_ONE;

    sign * shaped_q15
}

fn axis_velocity_q16(shaped_q15: i64, speed_gain_q15: i64) -> i64 {
    let velocity_q16 =
        shaped_q15 * speed_gain_q15 * i64::from(MOUSE_BASE_SPEED_PX_PER_SEC) * 2 / Q15_ONE;

    let max_velocity_q16 = i64::from(MOUSE_MAX_SPEED_PX_PER_SEC) * Q16_ONE;

    velocity_q16.clamp(-max_velocity_q16, max_velocity_q16)
}

fn take_delta(residual_q16: &mut i64) -> i8 {
    let whole = *residual_q16 / Q16_ONE;

    let delta = whole.clamp(-127, 127);

    *residual_q16 -= delta * Q16_ONE;

    delta as i8
}
