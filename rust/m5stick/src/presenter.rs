#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresenterAction {
    NextSlide,
    PreviousSlide,
}

impl PresenterAction {
    pub fn key_code(self) -> u8 {
        match self {
            Self::NextSlide => 0x4f,     // Right Arrow
            Self::PreviousSlide => 0x50, // Left Arrow
        }
    }
}
