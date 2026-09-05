#![allow(dead_code)]

use crate::config::{DEFAULT_PROFILE_INDEX, PROFILE_COUNT, PROFILE_LABELS, PROFILE_NAMES};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Profile {
    pub index: usize,
    pub name: &'static str,
    pub label: &'static str,
}

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

    pub fn current(&self) -> Profile {
        profile_at(self.selected_index)
    }

    pub fn next(&mut self) -> Profile {
        self.selected_index = (self.selected_index + 1) % PROFILE_COUNT;

        self.current()
    }
}

fn profile_at(index: usize) -> Profile {
    Profile {
        index,
        name: PROFILE_NAMES[index],
        label: PROFILE_LABELS[index],
    }
}
