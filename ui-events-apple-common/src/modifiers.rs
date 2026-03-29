// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use ui_events::keyboard::Modifiers;

/// Build a [`Modifiers`] from booleans.
pub fn modifiers_from_bools(ctrl: bool, alt: bool, shift: bool, meta: bool) -> Modifiers {
    let mut m = Modifiers::default();
    if ctrl {
        m.insert(Modifiers::CONTROL);
    }
    if alt {
        m.insert(Modifiers::ALT);
    }
    if shift {
        m.insert(Modifiers::SHIFT);
    }
    if meta {
        m.insert(Modifiers::META);
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifiers_from_bools_sets_expected_bits() {
        let mods = modifiers_from_bools(true, true, true, true);
        assert!(mods.ctrl());
        assert!(mods.alt());
        assert!(mods.shift());
        assert!(mods.meta());
    }
}
