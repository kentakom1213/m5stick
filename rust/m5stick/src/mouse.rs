use crate::mini_joyc::JoyState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseState {
    pub dx: i8,
    pub dy: i8,
    pub left_pressed: bool,
}

pub fn from_joystick(state: JoyState) -> MouseState {
    MouseState {
        dx: axis_to_delta(state.x),
        // マウス座標とJoyCの向きが逆ならここで反転
        dy: axis_to_delta(state.y),
        left_pressed: state.pressed,
    }
}

fn axis_to_delta(value: i8) -> i8 {
    const DEAD_ZONE: i32 = 12;
    const MAX_INPUT: i32 = 127;
    const MAX_SPEED: i32 = 12;

    let value = i32::from(value);

    let sign = value.signum();
    let magnitude = value.abs();

    if magnitude <= DEAD_ZONE {
        return 0;
    }

    let normalized = magnitude - DEAD_ZONE;
    let range = MAX_INPUT - DEAD_ZONE;

    let speed =
        normalized * normalized * MAX_SPEED
        / (range * range);

    (sign * speed) as i8
}
