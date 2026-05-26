//! Modifier state for the bottom modifier bar.
//!
//! Ctrl / Alt are **one-shot** sticky modifiers — tapping the
//! modifier cell arms the flag, the next `SendKey` consumes it. The
//! controller calls `consume_one_shots` after every emitted key.
//!
//! Shift is intentionally absent — the bar has no shift key. The
//! native OS keyboard handles letter case directly via its own shift
//! glyph.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Modifier {
    Ctrl,
    Alt,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct KeyboardModifiers {
    /// One-shot. Aliased to `State::ctrl_held` so hardware-key
    /// passthrough and the modifier bar share the same armed state —
    /// arming Ctrl on the bar carries through to a hardware-tapped
    /// follow-up, and vice versa.
    pub ctrl_armed: bool,
    /// One-shot. Aliased to `State::alt_held` (see `ctrl_armed`).
    pub alt_armed: bool,
}

impl KeyboardModifiers {
    /// Drop the one-shot mods. Called by the controller after
    /// emitting a `SendKey`, so a `Ctrl Right` sequence sends
    /// `Ctrl+Right` and the next tap goes through with no modifier.
    pub fn consume_one_shots(&mut self) {
        self.ctrl_armed = false;
        self.alt_armed = false;
    }

    pub fn is_armed(&self, m: Modifier) -> bool {
        match m {
            Modifier::Ctrl => self.ctrl_armed,
            Modifier::Alt => self.alt_armed,
        }
    }

    pub fn toggle(&mut self, m: Modifier) {
        match m {
            Modifier::Ctrl => self.ctrl_armed = !self.ctrl_armed,
            Modifier::Alt => self.alt_armed = !self.alt_armed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consume_one_shots_clears_modifiers() {
        let mut m = KeyboardModifiers {
            ctrl_armed: true,
            alt_armed: true,
        };
        m.consume_one_shots();
        assert!(!m.ctrl_armed);
        assert!(!m.alt_armed);
    }

    #[test]
    fn toggle_flips_modifier_state() {
        let mut m = KeyboardModifiers::default();
        assert!(!m.is_armed(Modifier::Ctrl));
        m.toggle(Modifier::Ctrl);
        assert!(m.is_armed(Modifier::Ctrl));
        m.toggle(Modifier::Ctrl);
        assert!(!m.is_armed(Modifier::Ctrl));
    }
}
