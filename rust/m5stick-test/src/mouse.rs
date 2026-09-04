use crate::mini_joyc::JoyState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseState {
    pub dx: i8,
    pub dy: i8,
    pub left_pressed: bool,
}

pub fn from_joystick(state: JoyState) -> MouseState {
    MouseState {
        dx: apply_dead_zone(state.x),
        dy: apply_dead_zone(state.y),
        left_pressed: state.pressed,
    }
}

fn apply_dead_zone(value: i8) -> i8 {
    const DEAD_ZONE: i8 = 10;

    if value.abs() <= DEAD_ZONE { 0 } else { value }
}
