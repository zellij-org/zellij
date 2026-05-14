//! Modifier and layer state for the in-plugin keyboard.
//!
//! Shift / Ctrl / Alt are **one-shot** sticky modifiers — tapping the
//! modifier cell arms it, the next `SendKey` consumes the arm. The
//! controller calls `consume_one_shots` after every `SendKey`.
//!
//! `KeyLayer` is the active layer (Letters / Symbols / Functions) and
//! is *not* a modifier. Layer switches happen via `SwitchLayer` actions
//! and persist across `SendKey`s — so a `⌃` armed on Letters, then a
//! switch to Symbols, then a tap on `\` produces `Ctrl+\`.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Modifier {
    Shift,
    Ctrl,
    Alt,
}

/// Active keyboard layer. Mutually exclusive at any moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyLayer {
    Letters,
    Symbols,
    Functions,
}

#[derive(Debug, Clone, Copy)]
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
    /// Active layer (Letters / Symbols / Functions). Persists across
    /// `SendKey`s — `consume_one_shots` deliberately leaves this alone.
    pub layer: KeyLayer,
}

impl Default for KeyboardModifiers {
    fn default() -> Self {
        Self {
            shift_armed: false,
            ctrl_armed: false,
            alt_armed: false,
            layer: KeyLayer::Letters,
        }
    }
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
        }
    }

    pub fn toggle(&mut self, m: Modifier) {
        match m {
            Modifier::Shift => self.shift_armed = !self.shift_armed,
            Modifier::Ctrl => self.ctrl_armed = !self.ctrl_armed,
            Modifier::Alt => self.alt_armed = !self.alt_armed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consume_one_shots_clears_only_one_shot_modifiers() {
        let mut m = KeyboardModifiers {
            shift_armed: true,
            ctrl_armed: true,
            alt_armed: true,
            layer: KeyLayer::Symbols,
        };
        m.consume_one_shots();
        assert!(!m.shift_armed);
        assert!(!m.ctrl_armed);
        assert!(!m.alt_armed);
        // Layer must survive a one-shot sweep.
        assert_eq!(m.layer, KeyLayer::Symbols);
    }

    #[test]
    fn toggle_flips_modifier_state() {
        let mut m = KeyboardModifiers::default();
        assert!(!m.is_armed(Modifier::Shift));
        m.toggle(Modifier::Shift);
        assert!(m.is_armed(Modifier::Shift));
        m.toggle(Modifier::Shift);
        assert!(!m.is_armed(Modifier::Shift));
    }
}
