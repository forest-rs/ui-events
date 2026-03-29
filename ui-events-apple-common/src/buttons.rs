// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use ui_events::pointer::{PointerButton, PointerButtons};

/// Map a platform button index to a [`PointerButton`].
///
/// Common mapping: 0=Primary (left), 1=Secondary (right), 2=Auxiliary
/// (middle), 3=X1, 4=X2; higher indices map to B7..B32 when possible.
pub fn try_from_button_index(i: i64) -> Option<PointerButton> {
    Some(match i {
        0 => PointerButton::Primary,
        1 => PointerButton::Secondary,
        2 => PointerButton::Auxiliary,
        3 => PointerButton::X1,
        4 => PointerButton::X2,
        5 => PointerButton::PenEraser,
        6 => PointerButton::B7,
        7 => PointerButton::B8,
        8 => PointerButton::B9,
        9 => PointerButton::B10,
        10 => PointerButton::B11,
        11 => PointerButton::B12,
        12 => PointerButton::B13,
        13 => PointerButton::B14,
        14 => PointerButton::B15,
        15 => PointerButton::B16,
        16 => PointerButton::B17,
        17 => PointerButton::B18,
        18 => PointerButton::B19,
        19 => PointerButton::B20,
        20 => PointerButton::B21,
        21 => PointerButton::B22,
        22 => PointerButton::B23,
        23 => PointerButton::B24,
        24 => PointerButton::B25,
        25 => PointerButton::B26,
        26 => PointerButton::B27,
        27 => PointerButton::B28,
        28 => PointerButton::B29,
        29 => PointerButton::B30,
        30 => PointerButton::B31,
        31 => PointerButton::B32,
        _ => return None,
    })
}

/// Convert a bitmask of pressed buttons (LSB = button 0) into
/// [`PointerButtons`].
pub fn buttons_from_bitmask(mask: u64) -> PointerButtons {
    let mut out = PointerButtons::default();
    for idx in 0..32 {
        if (mask & (1_u64 << idx)) != 0 {
            if let Some(b) = try_from_button_index(idx as i64) {
                out.insert(b);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buttons_from_bitmask_maps_common_buttons() {
        let buttons = buttons_from_bitmask((1 << 0) | (1 << 1) | (1 << 4));
        assert!(buttons.contains(PointerButton::Primary));
        assert!(buttons.contains(PointerButton::Secondary));
        assert!(buttons.contains(PointerButton::X2));
    }
}
