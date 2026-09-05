use crate::{
    config::{
        MOUSE_CURVE_WEIGHT_Q15, MOUSE_DEAD_ZONE, MOUSE_INVERT_X, MOUSE_INVERT_Y,
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

    residual_x_q16: i64,
    residual_y_q16: i64,
}

impl MouseMapper {
    pub fn new() -> Self {
        Self {
            filtered_x_q8: 0,
            filtered_y_q8: 0,
            residual_x_q16: 0,
            residual_y_q16: 0,
        }
    }

    pub fn update(&mut self, state: JoyState) -> MouseState {
        update_filter(&mut self.filtered_x_q8, state.x);

        update_filter(&mut self.filtered_y_q8, state.y);

        let velocity_x_q15 = axis_velocity_q15(self.filtered_x_q8, MOUSE_INVERT_X);

        let velocity_y_q15 = axis_velocity_q15(self.filtered_y_q8, MOUSE_INVERT_Y);

        // velocity は pixels/sec * 2^15．
        // 1 poll あたりの移動量を Q16 pixel へ変換する．
        self.residual_x_q16 += velocity_x_q15 * 2 / MOUSE_POLL_HZ;

        self.residual_y_q16 += velocity_y_q15 * 2 / MOUSE_POLL_HZ;

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

fn axis_velocity_q15(value_q8: i32, invert: bool) -> i64 {
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

    sign * shaped_q15 * i64::from(MOUSE_MAX_SPEED_PX_PER_SEC)
}

fn take_delta(residual_q16: &mut i64) -> i8 {
    let whole = *residual_q16 / Q16_ONE;

    let delta = whole.clamp(-127, 127);

    *residual_q16 -= delta * Q16_ONE;

    delta as i8
}
