use crate::config::{
    Action, BUTTON_A_BIT, BUTTON_B_BIT, ButtonInputConfig, ComboConfig, InputConfig, JOY_CLICK_BIT,
    Orientation, ScrollConfig,
};
use crate::mini_joyc::JoyState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputState {
    pub button_a: bool,
    pub button_b: bool,
    pub joy_click: bool,
}

impl InputState {
    pub fn button_mask(self) -> u8 {
        let mut mask = 0;

        if self.button_a {
            mask |= BUTTON_A_BIT;
        }

        if self.button_b {
            mask |= BUTTON_B_BIT;
        }

        if self.joy_click {
            mask |= JOY_CLICK_BIT;
        }

        mask
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputSample {
    pub buttons: InputState,
    pub joy: JoyState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputEvent {
    pub action: Action,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputOutcome {
    pub event: Option<InputEvent>,
    pub scroll_mode: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ButtonRuntime {
    down: bool,
    pressed_at_ms: u64,
    consumed: bool,
    hold_sent: bool,
}

impl ButtonRuntime {
    const fn new() -> Self {
        Self {
            down: false,
            pressed_at_ms: 0,
            consumed: false,
            hold_sent: false,
        }
    }

    fn update_down(&mut self, down: bool, now_ms: u64) {
        if down && !self.down {
            self.pressed_at_ms = now_ms;
            self.consumed = false;
            self.hold_sent = false;
        }

        self.down = down;
    }

    fn released_tap(
        &mut self,
        was_down: bool,
        is_down: bool,
        now_ms: u64,
        config: &ButtonInputConfig,
        tap_max_ms: u64,
    ) -> Option<Action> {
        if was_down
            && !is_down
            && !self.consumed
            && now_ms.saturating_sub(self.pressed_at_ms) <= tap_max_ms
        {
            self.consumed = true;
            return action_or_none(config.tap);
        }

        None
    }

    fn hold_event(&mut self, now_ms: u64, config: &ButtonInputConfig) -> Option<Action> {
        if self.down
            && !self.consumed
            && !self.hold_sent
            && config.hold != Action::None
            && now_ms.saturating_sub(self.pressed_at_ms) >= config.hold_ms
        {
            self.consumed = true;
            self.hold_sent = true;
            return Some(config.hold);
        }

        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputEngine {
    button_a: ButtonRuntime,
    button_b: ButtonRuntime,
    joy_click: ButtonRuntime,
    combo_lock: u8,
    scroll_mode: bool,
}

impl InputEngine {
    pub const fn new() -> Self {
        Self {
            button_a: ButtonRuntime::new(),
            button_b: ButtonRuntime::new(),
            joy_click: ButtonRuntime::new(),
            combo_lock: 0,
            scroll_mode: false,
        }
    }

    pub fn update(
        &mut self,
        sample: InputSample,
        config: &InputConfig,
        scroll: &ScrollConfig,
        orientation: Orientation,
        now_ms: u64,
    ) -> InputOutcome {
        let previous = InputState {
            button_a: self.button_a.down,
            button_b: self.button_b.down,
            joy_click: self.joy_click.down,
        };

        self.button_a.update_down(sample.buttons.button_a, now_ms);
        self.button_b.update_down(sample.buttons.button_b, now_ms);
        self.joy_click.update_down(sample.buttons.joy_click, now_ms);

        let mask = sample.buttons.button_mask();

        if self.combo_lock != 0 && (mask & self.combo_lock) != self.combo_lock {
            self.combo_lock = 0;
        }

        let mut event = None;

        if self.combo_lock == 0 {
            event = self.combo_event(mask, config, now_ms);
        }

        if event.is_none() {
            event = self.hold_event(config, now_ms);
        }

        self.update_scroll_mode(sample, config, scroll, orientation);

        if event.is_none() {
            event = self.release_event(sample.buttons, previous, config, now_ms);
        }

        InputOutcome {
            event: event.map(|action| InputEvent { action }),
            scroll_mode: self.scroll_mode,
        }
    }

    fn combo_event(&mut self, mask: u8, config: &InputConfig, now_ms: u64) -> Option<Action> {
        let mut selected = None;
        let mut selected_bits = 0;

        for combo in config.combos {
            if (mask & combo.buttons) != combo.buttons {
                continue;
            }

            if combo.hold_ms == 0 && !self.within_combo_window(combo, config.combo_window_ms) {
                continue;
            }

            let bits = combo.buttons.count_ones();

            if bits > selected_bits {
                selected = Some(combo);
                selected_bits = bits;
            }
        }

        let combo = selected?;
        let first_pressed_at_ms = self.first_pressed_at_ms(combo.buttons);

        if combo.hold_ms > 0 && now_ms.saturating_sub(first_pressed_at_ms) < combo.hold_ms {
            return None;
        }

        self.consume(combo.buttons);
        self.combo_lock = combo.buttons;

        Some(combo.action)
    }

    fn hold_event(&mut self, config: &InputConfig, now_ms: u64) -> Option<Action> {
        self.button_a
            .hold_event(now_ms, &config.button_a)
            .or_else(|| self.button_b.hold_event(now_ms, &config.button_b))
            .or_else(|| self.joy_click.hold_event(now_ms, &config.joy_click))
    }

    fn release_event(
        &mut self,
        current: InputState,
        previous: InputState,
        config: &InputConfig,
        now_ms: u64,
    ) -> Option<Action> {
        let mut event = None;

        if previous.button_a && !current.button_a {
            event = self.button_a.released_tap(
                previous.button_a,
                current.button_a,
                now_ms,
                &config.button_a,
                config.tap_max_ms,
            );
            self.scroll_mode = false;
        }

        if event.is_none() && previous.button_b && !current.button_b {
            event = self.button_b.released_tap(
                previous.button_b,
                current.button_b,
                now_ms,
                &config.button_b,
                config.tap_max_ms,
            );
        }

        if event.is_none() && previous.joy_click && !current.joy_click {
            event = self.joy_click.released_tap(
                previous.joy_click,
                current.joy_click,
                now_ms,
                &config.joy_click,
                config.tap_max_ms,
            );
        }

        event
    }

    fn update_scroll_mode(
        &mut self,
        sample: InputSample,
        config: &InputConfig,
        scroll: &ScrollConfig,
        orientation: Orientation,
    ) {
        if !sample.buttons.button_a {
            self.scroll_mode = false;
            return;
        }

        if config.button_a.hold_joy != Action::ScrollMode {
            return;
        }

        let (_, rotated_y) = rotate_joy(
            i32::from(sample.joy.x),
            i32::from(sample.joy.y),
            orientation,
        );

        if rotated_y.abs() > scroll.dead_zone {
            self.button_a.consumed = true;
            self.scroll_mode = true;
        }
    }

    fn within_combo_window(&self, combo: &ComboConfig, combo_window_ms: u64) -> bool {
        let first = self.first_pressed_at_ms(combo.buttons);
        let last = self.last_pressed_at_ms(combo.buttons);

        last.saturating_sub(first) <= combo_window_ms
    }

    fn first_pressed_at_ms(&self, buttons: u8) -> u64 {
        let mut first = u64::MAX;

        if buttons & BUTTON_A_BIT != 0 {
            first = first.min(self.button_a.pressed_at_ms);
        }

        if buttons & BUTTON_B_BIT != 0 {
            first = first.min(self.button_b.pressed_at_ms);
        }

        if buttons & JOY_CLICK_BIT != 0 {
            first = first.min(self.joy_click.pressed_at_ms);
        }

        first
    }

    fn last_pressed_at_ms(&self, buttons: u8) -> u64 {
        let mut last = 0;

        if buttons & BUTTON_A_BIT != 0 {
            last = last.max(self.button_a.pressed_at_ms);
        }

        if buttons & BUTTON_B_BIT != 0 {
            last = last.max(self.button_b.pressed_at_ms);
        }

        if buttons & JOY_CLICK_BIT != 0 {
            last = last.max(self.joy_click.pressed_at_ms);
        }

        last
    }

    fn consume(&mut self, buttons: u8) {
        if buttons & BUTTON_A_BIT != 0 {
            self.button_a.consumed = true;
        }

        if buttons & BUTTON_B_BIT != 0 {
            self.button_b.consumed = true;
        }

        if buttons & JOY_CLICK_BIT != 0 {
            self.joy_click.consumed = true;
        }
    }
}

fn rotate_joy(x: i32, y: i32, orientation: Orientation) -> (i32, i32) {
    match orientation {
        Orientation::Normal => (x, y),
        Orientation::Right => (-y, x),
        Orientation::Inverted => (-x, -y),
        Orientation::Left => (y, -x),
    }
}

fn action_or_none(action: Action) -> Option<Action> {
    match action {
        Action::None => None,
        other => Some(other),
    }
}
