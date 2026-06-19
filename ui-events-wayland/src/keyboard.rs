// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Reduces `wl_keyboard` event streams into [`KeyboardEvent`]s.
//!
//! [`KeyboardEventReducer`] mirrors the stateful-reducer pattern of the other
//! adapters: feed it each `wl_keyboard` event for a seat's keyboard and it
//! tracks the focused keyboard state, returning a [`KeyboardEvent`] for each key
//! press and release.
//!
//! ## Physical keys without a keymap
//!
//! This reducer resolves the *physical* key. Each key's evdev scancode is mapped
//! to its W3C [`Code`] through [`mapping::code_from_evdev_scancode`], and the key
//! state to [`KeyState::Down`] or [`KeyState::Up`]. The `repeated` key state
//! added in `wl_keyboard` version 10 is reported as a [`KeyState::Down`] with the
//! event's `repeat` flag set; the reducer does not otherwise synthesize repeats.
//!
//! The *logical* key value depends on the active layout, dead keys, and
//! composition, all of which need the keymap, so the [`KeyboardEvent`]'s `key` is
//! [`Key::Named`]\([`NamedKey::Unidentified`]) and its `location` is
//! [`Location::Standard`] in this tier. Resolving them, and typed text, is the
//! job of the `xkb` feature.
//!
//! ## Modifiers
//!
//! Without the keymap the `wl_keyboard` `modifiers` event is an opaque bitmask,
//! so the reducer ignores it and instead tracks the physical modifier keys it
//! sees pressed and released — Control, Alt, Shift, and Meta, by their
//! layout-independent [`Code`]s — assembling the modifier set through
//! [`mapping::modifiers_from_bools`]. A modifier stays active while either its
//! left or right key is held. The lock states (Caps Lock, Num Lock) and the
//! Alt Graph distinction are defined by the keymap and resolved only under the
//! `xkb` feature.
//!
//! ## Focus and repeat
//!
//! A focus `enter` lists the keys already logically down; the reducer seeds its
//! pressed-key state from them so the modifier set is correct from the first
//! event, but emits nothing (the protocol advises against emulating presses from
//! that list). A `leave` clears the state. The `keymap` event is used only under
//! the `xkb` feature, and the `repeat_info` parameters are surfaced through
//! [`KeyboardEventReducer::repeat_info`] for a consumer that drives key repeat.
//!
//! [`Key::Named`]: ui_events::keyboard::Key::Named
//! [`NamedKey::Unidentified`]: ui_events::keyboard::NamedKey::Unidentified
//! [`Location::Standard`]: ui_events::keyboard::Location::Standard

use ui_events::keyboard::{Code, Key, KeyState, KeyboardEvent, Location, Modifiers, NamedKey};
use wayland_client::WEnum;
use wayland_client::protocol::wl_keyboard::{Event, KeyState as WlKeyState};

use crate::mapping;

/// Key-repeat parameters reported by `wl_keyboard`'s `repeat_info` event.
///
/// The compositor advertises how the seat's keyboard should repeat a held key.
/// The reducer does not synthesize repeat events itself — that needs a timer in
/// the consumer's event loop — so it surfaces these parameters for the consumer
/// to drive repetition (or to defer to a compositor that takes over repetition,
/// which it signals by advertising a `rate` of `0`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RepeatInfo {
    /// The repeat rate in keys per second. A rate of `0` disables key repeat.
    pub rate: i32,
    /// The delay in milliseconds from a key going down until it starts
    /// repeating.
    pub delay: i32,
}

/// Reduces a `wl_keyboard` event stream into [`KeyboardEvent`]s.
///
/// Keep one reducer per seat keyboard and call [`reduce`] with each
/// `wl_keyboard` event. See the [module documentation] for what this tier does
/// and does not resolve.
///
/// [`reduce`]: KeyboardEventReducer::reduce
/// [module documentation]: self
#[derive(Debug, Default)]
pub struct KeyboardEventReducer {
    /// The evdev scancodes currently logically down.
    ///
    /// Seeded from a focus `enter`, updated by each `key` event, and cleared on
    /// `leave`. The modifier set is derived from this state, so a left/right
    /// modifier pair stays active until both keys are released.
    pressed: Vec<u32>,
    /// The most recent key-repeat parameters from `repeat_info`, if any.
    repeat_info: Option<RepeatInfo>,
}

impl KeyboardEventReducer {
    /// Reduce a single `wl_keyboard` [`Event`] into an optional [`KeyboardEvent`].
    ///
    /// Only `key` events translate to a [`KeyboardEvent`]; `enter`, `leave`, and
    /// `repeat_info` update internal state and return `None`, and the `keymap`
    /// and (without the keymap, opaque) `modifiers` events are ignored.
    pub fn reduce(&mut self, event: &Event) -> Option<KeyboardEvent> {
        match event {
            Event::Key { key, state, .. } => self.key(*key, *state),
            Event::Enter { keys, .. } => {
                self.enter(keys);
                None
            }
            Event::Leave { .. } => {
                self.leave();
                None
            }
            Event::RepeatInfo { rate, delay } => {
                self.repeat_info = Some(RepeatInfo {
                    rate: *rate,
                    delay: *delay,
                });
                None
            }
            // The keymap is consumed only under the `xkb` feature; without it the
            // raw scancodes are interpreted directly. The `modifiers` bitmask is
            // opaque without the keymap, so physical modifier keys are tracked
            // instead (see `modifiers`).
            Event::Keymap { .. } | Event::Modifiers { .. } => None,
            // `wl_keyboard::Event` is `#[non_exhaustive]`; ignore future additions.
            _ => None,
        }
    }

    /// The most recently advertised key-repeat parameters, if the compositor has
    /// sent a `repeat_info` event.
    pub fn repeat_info(&self) -> Option<RepeatInfo> {
        self.repeat_info
    }

    /// The current modifier set, derived from the physical modifier keys held.
    ///
    /// This reflects the same state stamped into each emitted [`KeyboardEvent`].
    /// It is useful right after a focus `enter`, whose held keys seed the state
    /// before any [`KeyboardEvent`] is produced. Lock states and the Alt Graph
    /// distinction are resolved only under the `xkb` feature.
    pub fn modifiers(&self) -> Modifiers {
        let (mut ctrl, mut alt, mut shift, mut meta) = (false, false, false, false);
        for &scancode in &self.pressed {
            match mapping::code_from_evdev_scancode(scancode) {
                Code::ControlLeft | Code::ControlRight => ctrl = true,
                Code::AltLeft | Code::AltRight => alt = true,
                Code::ShiftLeft | Code::ShiftRight => shift = true,
                Code::MetaLeft | Code::MetaRight => meta = true,
                _ => {}
            }
        }
        mapping::modifiers_from_bools(ctrl, alt, shift, meta)
    }

    /// Handle a `key` event: update the pressed-key state and build the event.
    fn key(&mut self, scancode: u32, state: WEnum<WlKeyState>) -> Option<KeyboardEvent> {
        let (key_state, repeat) = match state {
            WEnum::Value(WlKeyState::Pressed) => (KeyState::Down, false),
            // The `repeated` state (version 10) means "still pressed", flagged as
            // a repeat.
            WEnum::Value(WlKeyState::Repeated) => (KeyState::Down, true),
            WEnum::Value(WlKeyState::Released) => (KeyState::Up, false),
            // Unknown key state; ignore.
            _ => return None,
        };

        if key_state == KeyState::Down {
            if !self.pressed.contains(&scancode) {
                self.pressed.push(scancode);
            }
        } else {
            self.pressed.retain(|&held| held != scancode);
        }

        Some(KeyboardEvent {
            state: key_state,
            key: Key::Named(NamedKey::Unidentified),
            code: mapping::code_from_evdev_scancode(scancode),
            location: Location::Standard,
            modifiers: self.modifiers(),
            repeat,
            is_composing: false,
        })
    }

    /// Seed the pressed-key state from a focus `enter`'s key array.
    ///
    /// `keys` is the wire array of currently-pressed evdev scancodes, each
    /// encoded as a little-endian `u32`. Trailing bytes that do not form a whole
    /// `u32` are ignored.
    ///
    /// This takes the key bytes rather than the `wl_surface`, so the
    /// surface-independent logic stays unit-testable without a live connection
    /// (a `wl_surface` cannot be constructed without one).
    fn enter(&mut self, keys: &[u8]) {
        self.pressed.clear();
        for chunk in keys.chunks_exact(4) {
            let scancode = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            if !self.pressed.contains(&scancode) {
                self.pressed.push(scancode);
            }
        }
    }

    /// Handle a `leave` event: clear the pressed-key state.
    fn leave(&mut self) {
        self.pressed.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `KEY_A` from `linux/input-event-codes.h`.
    const KEY_A: u32 = 30;
    /// `KEY_LEFTCTRL`.
    const KEY_LEFTCTRL: u32 = 29;
    /// `KEY_LEFTSHIFT`.
    const KEY_LEFTSHIFT: u32 = 42;
    /// `KEY_RIGHTSHIFT`.
    const KEY_RIGHTSHIFT: u32 = 54;

    fn key_event(scancode: u32, state: WlKeyState) -> Event {
        Event::Key {
            serial: 0,
            time: 0,
            key: scancode,
            state: WEnum::Value(state),
        }
    }

    #[test]
    fn key_press_maps_physical_code_and_down_state() {
        let mut reducer = KeyboardEventReducer::default();
        let event = reducer
            .reduce(&key_event(KEY_A, WlKeyState::Pressed))
            .expect("a key press should translate");

        assert_eq!(event.state, KeyState::Down);
        assert_eq!(event.code, Code::KeyA);
        assert_eq!(event.key, Key::Named(NamedKey::Unidentified));
        assert_eq!(event.location, Location::Standard);
        assert!(!event.repeat);
        assert!(!event.is_composing);
        assert_eq!(event.modifiers, Modifiers::empty());
    }

    #[test]
    fn key_release_maps_up_state() {
        let mut reducer = KeyboardEventReducer::default();
        let _ = reducer.reduce(&key_event(KEY_A, WlKeyState::Pressed));
        let event = reducer
            .reduce(&key_event(KEY_A, WlKeyState::Released))
            .expect("a key release should translate");

        assert_eq!(event.state, KeyState::Up);
        assert_eq!(event.code, Code::KeyA);
    }

    #[test]
    fn repeated_key_state_sets_repeat_flag() {
        let mut reducer = KeyboardEventReducer::default();
        let _ = reducer.reduce(&key_event(KEY_A, WlKeyState::Pressed));
        let event = reducer
            .reduce(&key_event(KEY_A, WlKeyState::Repeated))
            .expect("a repeated key should translate");

        assert_eq!(event.state, KeyState::Down);
        assert!(event.repeat);
    }

    #[test]
    fn modifiers_track_physical_modifier_keys() {
        let mut reducer = KeyboardEventReducer::default();

        let ctrl_down = reducer
            .reduce(&key_event(KEY_LEFTCTRL, WlKeyState::Pressed))
            .expect("control down should translate");
        // The set includes the modifier currently being pressed.
        assert!(ctrl_down.modifiers.ctrl());
        assert_eq!(ctrl_down.code, Code::ControlLeft);

        let a_down = reducer
            .reduce(&key_event(KEY_A, WlKeyState::Pressed))
            .expect("a down should translate");
        assert!(a_down.modifiers.ctrl());

        let ctrl_up = reducer
            .reduce(&key_event(KEY_LEFTCTRL, WlKeyState::Released))
            .expect("control up should translate");
        assert!(!ctrl_up.modifiers.ctrl());
    }

    #[test]
    fn left_and_right_modifier_stay_active_until_both_released() {
        let mut reducer = KeyboardEventReducer::default();
        let _ = reducer.reduce(&key_event(KEY_LEFTSHIFT, WlKeyState::Pressed));
        let _ = reducer.reduce(&key_event(KEY_RIGHTSHIFT, WlKeyState::Pressed));

        let left_up = reducer
            .reduce(&key_event(KEY_LEFTSHIFT, WlKeyState::Released))
            .expect("left shift up should translate");
        // The right shift is still held, so shift stays active.
        assert!(left_up.modifiers.shift());

        let right_up = reducer
            .reduce(&key_event(KEY_RIGHTSHIFT, WlKeyState::Released))
            .expect("right shift up should translate");
        assert!(!right_up.modifiers.shift());
    }

    #[test]
    fn enter_seeds_modifier_state_from_pressed_keys() {
        let mut reducer = KeyboardEventReducer::default();
        // The focus `enter` reports Left Control already held, as a
        // little-endian u32 scancode array.
        reducer.enter(&KEY_LEFTCTRL.to_le_bytes());

        assert!(reducer.modifiers().ctrl());
        // A subsequent key carries the seeded modifier state.
        let a_down = reducer
            .reduce(&key_event(KEY_A, WlKeyState::Pressed))
            .expect("a down should translate");
        assert!(a_down.modifiers.ctrl());
    }

    #[test]
    fn leave_clears_modifier_state() {
        let mut reducer = KeyboardEventReducer::default();
        reducer.enter(&KEY_LEFTCTRL.to_le_bytes());
        reducer.leave();

        assert_eq!(reducer.modifiers(), Modifiers::empty());
    }

    #[test]
    fn repeat_info_is_exposed_and_emits_nothing() {
        let mut reducer = KeyboardEventReducer::default();
        assert_eq!(reducer.repeat_info(), None);

        let emitted = reducer.reduce(&Event::RepeatInfo {
            rate: 25,
            delay: 600,
        });

        assert_eq!(emitted, None);
        assert_eq!(
            reducer.repeat_info(),
            Some(RepeatInfo {
                rate: 25,
                delay: 600
            })
        );
    }

    #[test]
    fn opaque_modifiers_event_is_ignored() {
        let mut reducer = KeyboardEventReducer::default();
        // The bitmask is meaningless without a keymap, so it must neither change
        // the tracked modifier state nor emit an event.
        let emitted = reducer.reduce(&Event::Modifiers {
            serial: 0,
            mods_depressed: u32::MAX,
            mods_latched: 0,
            mods_locked: 0,
            group: 0,
        });

        assert_eq!(emitted, None);
        assert_eq!(reducer.modifiers(), Modifiers::empty());
    }

    #[test]
    fn unmapped_scancode_is_unidentified() {
        let mut reducer = KeyboardEventReducer::default();
        let event = reducer
            .reduce(&key_event(0, WlKeyState::Pressed))
            .expect("an unmapped key should still translate");
        assert_eq!(event.code, Code::Unidentified);
    }
}
