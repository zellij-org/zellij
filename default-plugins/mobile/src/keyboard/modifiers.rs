//! Modifier state for the in-plugin keyboard.
//!
//! Shift / Ctrl / Alt are **one-shot** sticky modifiers — tapping the
//! modifier cell arms it, the next `SendKey` consumes the arm. Fn is a
//! **toggle** — F-keys are typed in bursts (function-key dialogs in
//! vim, mc, etc.), so requiring a re-arm per F-key would be hostile.
//! The controller calls `consume_one_shots` after every `SendKey`.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Modifier {
    Shift,
    Ctrl,
    Alt,
    Fn,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct KeyboardModifiers {
    /// One-shot. Set by tapping ⇧; cleared by `consume_one_shots`.
    pub shift_armed: bool,
    /// One-shot. Aliased to `State::ctrl_held` so the hardware-key
    /// passthrough path and the in-plugin keyboard share the same
    /// armed state — toggling on the keyboard arms a hardware-tapped
    /// follow-up, and vice versa.
    pub ctrl_armed: bool,
    /// One-shot. Aliased to `State::alt_held` (see `ctrl_armed`).
    pub alt_armed: bool,
    /// Toggle. Cleared only by another tap on Fn — `consume_one_shots`
    /// deliberately leaves this field alone.
    pub fn_armed: bool,
}

impl KeyboardModifiers {
    /// Drop the three one-shot mods. Called by the controller after
    /// emitting a `SendKey`, so a `⇧ a` sequence sends `A` and then
    /// resets to lowercase for the next tap.
    pub fn consume_one_shots(&mut self) {
        self.shift_armed = false;
        self.ctrl_armed = false;
        self.alt_armed = false;
    }

    pub fn is_armed(&self, m: Modifier) -> bool {
        match m {
            Modifier::Shift => self.shift_armed,
            Modifier::Ctrl => self.ctrl_armed,
            Modifier::Alt => self.alt_armed,
            Modifier::Fn => self.fn_armed,
        }
    }

    pub fn toggle(&mut self, m: Modifier) {
        match m {
            Modifier::Shift => self.shift_armed = !self.shift_armed,
            Modifier::Ctrl => self.ctrl_armed = !self.ctrl_armed,
            Modifier::Alt => self.alt_armed = !self.alt_armed,
            Modifier::Fn => self.fn_armed = !self.fn_armed,
        }
    }
}
