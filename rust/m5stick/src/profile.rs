use crate::config::{DEFAULT_PROFILE_INDEX, PROFILE_COUNT, PROFILES, ProfileConfig};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProfileManager {
    selected_index: usize,
}

impl ProfileManager {
    pub fn new() -> Self {
        Self {
            selected_index: DEFAULT_PROFILE_INDEX,
        }
    }

    pub fn current(&self) -> &'static ProfileConfig {
        &PROFILES[self.selected_index]
    }

    pub fn next(&mut self) -> &'static ProfileConfig {
        self.selected_index = (self.selected_index + 1) % PROFILE_COUNT;

        self.current()
    }
}
