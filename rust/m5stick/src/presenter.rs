#![allow(dead_code)]

use crate::config::{Action, DEFAULT_PROFILE_INDEX, PROFILES};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresenterAction {
    ButtonA,
    ButtonB,
}

impl PresenterAction {
    pub fn key_code(self) -> u8 {
        let input = &PROFILES[DEFAULT_PROFILE_INDEX].input;
        let action = match self {
            Self::ButtonA => input.button_a.tap,
            Self::ButtonB => input.button_b.tap,
        };

        match action {
            Action::KeyboardKey(key) => key,
            _ => 0,
        }
    }
}
