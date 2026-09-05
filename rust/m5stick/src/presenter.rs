use crate::config::{BUTTON_A_KEY, BUTTON_B_KEY};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresenterAction {
    ButtonA,
    ButtonB,
}

impl PresenterAction {
    pub fn key_code(self) -> u8 {
        match self {
            Self::ButtonA => BUTTON_A_KEY,
            Self::ButtonB => BUTTON_B_KEY,
        }
    }
}
