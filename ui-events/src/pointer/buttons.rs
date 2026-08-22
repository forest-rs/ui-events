// Copyright 2025 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Describes a button of a pointer input device, such as a mouse or stylus pen.
///
/// B7..B32 exist for the purpose of supporting pointer devices with
/// large numbers of buttons.
/// These exotic pointer buttons top out around the 24 buttons range
/// in practice, and Windows doesn't support more than 32 mouse buttons
/// in most APIs, therefore 32 was chosen as the upper limit.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[repr(u32)]
pub enum PointerButton {
    /// Primary button, commonly the left mouse button, touch contact, pen contact.
    Primary = 1,
    /// Secondary button, commonly the right mouse button, pen barrel button.
    Secondary = 1 << 1,
    /// Auxiliary button, commonly the middle mouse button.
    Auxiliary = 1 << 2,
    /// X1 (back) Mouse.
    X1 = 1 << 3,
    /// X2 (forward) Mouse.
    X2 = 1 << 4,
    /// Pen erase button.
    PenEraser = 1 << 5,
    /// Button 7.
    B7 = 1 << 6,
    /// Button 8.
    B8 = 1 << 7,
    /// Button 9.
    B9 = 1 << 8,
    /// Button 10.
    B10 = 1 << 9,
    /// Button 11.
    B11 = 1 << 10,
    /// Button 12.
    B12 = 1 << 11,
    /// Button 13.
    B13 = 1 << 12,
    /// Button 14.
    B14 = 1 << 13,
    /// Button 15.
    B15 = 1 << 14,
    /// Button 16.
    B16 = 1 << 15,
    /// Button 17.
    B17 = 1 << 16,
    /// Button 18.
    B18 = 1 << 17,
    /// Button 19.
    B19 = 1 << 18,
    /// Button 20.
    B20 = 1 << 19,
    /// Button 21.
    B21 = 1 << 20,
    /// Button 22.
    B22 = 1 << 21,
    /// Button 23.
    B23 = 1 << 22,
    /// Button 24.
    B24 = 1 << 23,
    /// Button 25.
    B25 = 1 << 24,
    /// Button 26.
    B26 = 1 << 25,
    /// Button 27.
    B27 = 1 << 26,
    /// Button 28.
    B28 = 1 << 27,
    /// Button 29.
    B29 = 1 << 28,
    /// Button 30.
    B30 = 1 << 29,
    /// Button 31.
    B31 = 1 << 30,
    /// Button 32.
    B32 = 1 << 31,
}

/// A set of [`PointerButton`]s.
#[derive(Clone, Copy, Default, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct PointerButtons(u32);

impl PointerButtons {
    /// Create a new empty set.
    #[inline]
    pub fn new() -> Self {
        Self(0)
    }

    /// Add the `button` to the set.
    #[inline]
    pub fn insert(&mut self, button: PointerButton) {
        self.0 |= button as u32;
    }

    /// Remove the `button` from the set.
    #[inline]
    pub fn remove(&mut self, button: PointerButton) {
        self.0 &= !(button as u32);
    }

    /// Returns `true` if the `button` is in the set.
    #[inline]
    pub fn contains(self, button: PointerButton) -> bool {
        (self.0 & button as u32) != 0
    }

    /// Returns `true` if the set is empty.
    #[inline]
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Returns `true` if all the `buttons` are in the set.
    #[inline]
    pub fn contains_all(self, buttons: Self) -> bool {
        self.0 & buttons.0 == buttons.0
    }

    /// Adds all the `buttons` to the set.
    #[inline]
    pub fn extend(&mut self, buttons: Self) {
        self.0 |= buttons.0;
    }

    /// Clear the set.
    #[inline]
    pub fn clear(&mut self) {
        self.0 = 0;
    }

    /// Count the number of buttons in the set.
    #[inline]
    pub fn count(self) -> u32 {
        self.0.count_ones()
    }
}

const NONZERO_VARIANTS: [PointerButton; 32] = [
    PointerButton::Primary,
    PointerButton::Secondary,
    PointerButton::Auxiliary,
    PointerButton::X1,
    PointerButton::X2,
    PointerButton::PenEraser,
    PointerButton::B7,
    PointerButton::B8,
    PointerButton::B9,
    PointerButton::B10,
    PointerButton::B11,
    PointerButton::B12,
    PointerButton::B13,
    PointerButton::B14,
    PointerButton::B15,
    PointerButton::B16,
    PointerButton::B17,
    PointerButton::B18,
    PointerButton::B19,
    PointerButton::B20,
    PointerButton::B21,
    PointerButton::B22,
    PointerButton::B23,
    PointerButton::B24,
    PointerButton::B25,
    PointerButton::B26,
    PointerButton::B27,
    PointerButton::B28,
    PointerButton::B29,
    PointerButton::B30,
    PointerButton::B31,
    PointerButton::B32,
];

impl core::fmt::Debug for PointerButtons {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.is_empty() {
            return f.write_str("PointerButtons(None)");
        }

        f.write_str("PointerButtons(")?;

        if f.alternate() && self.count() > 2 {
            f.write_str("\n    ")?;
        }

        let mut first = true;
        for button in NONZERO_VARIANTS {
            if self.contains(button) {
                if !first {
                    if f.alternate() && self.count() > 2 {
                        f.write_str("\n    | ")?;
                    } else {
                        f.write_str(" | ")?;
                    }
                }
                first = false;
                button.fmt(f)?;
            }
        }

        if f.alternate() && self.count() > 2 {
            f.write_str("\n)")
        } else {
            f.write_str(")")
        }
    }
}
impl core::fmt::Binary for PointerButtons {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Binary::fmt(&self.0, f)
    }
}

impl core::ops::BitOr for PointerButton {
    type Output = PointerButtons;

    fn bitor(self, rhs: Self) -> Self::Output {
        PointerButtons(self as u32 | rhs as u32)
    }
}

impl core::ops::BitOr<PointerButton> for PointerButtons {
    type Output = Self;

    fn bitor(self, rhs: PointerButton) -> Self {
        Self(self.0 | rhs as u32)
    }
}

impl core::ops::BitOrAssign<PointerButton> for PointerButtons {
    fn bitor_assign(&mut self, rhs: PointerButton) {
        self.0 |= rhs as u32;
    }
}

impl From<PointerButton> for PointerButtons {
    fn from(button: PointerButton) -> Self {
        Self(button as u32)
    }
}

#[cfg(test)]
mod tests {
    /// `PointerButtons` debug formatting behavior.
    #[test]
    fn debug_fmt() {
        use crate::pointer::{PointerButton, PointerButtons};
        extern crate std;
        use std::format;

        assert_eq!(
            format!("{:?}", PointerButtons::default()),
            "PointerButtons(None)"
        );
        assert_eq!(
            format!("{:?}", PointerButtons::from(PointerButton::Primary)),
            "PointerButtons(Primary)"
        );
        assert_eq!(
            format!("{:?}", PointerButton::Primary | PointerButton::Auxiliary),
            "PointerButtons(Primary | Auxiliary)"
        );
        assert_eq!(
            format!(
                "{:?}",
                PointerButton::Primary | PointerButton::Auxiliary | PointerButton::Secondary
            ),
            "PointerButtons(Primary | Secondary | Auxiliary)"
        );
        assert_eq!(
            format!(
                "{:#?}",
                (
                    PointerButton::Primary | PointerButton::Auxiliary | PointerButton::Secondary,
                    PointerButton::B7 | PointerButton::X2
                )
            ),
            "(
    PointerButtons(
        Primary
        | Secondary
        | Auxiliary
    ),
    PointerButtons(X2 | B7),
)"
        );
        assert_eq!(
            format!("{:?}", PointerButton::B32 | PointerButton::Primary),
            "PointerButtons(Primary | B32)"
        );
    }

    /// Verify `PointerButton` is same size as `Option<PointerButton>`.
    #[test]
    fn option_niche_opt() {
        use crate::pointer::PointerButton;
        use core::mem::size_of;
        assert_eq!(
            size_of::<Option<PointerButton>>(),
            size_of::<PointerButton>()
        );
    }
}
