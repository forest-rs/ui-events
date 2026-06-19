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
//! job of the `xkb` feature described below.
//!
//! ## Logical keys with the `xkb` feature
//!
//! Enabling the `xkb` feature links `libxkbcommon` and resolves the logical key
//! from the compositor's keymap. The `keymap` event compiles an XKB keymap; each
//! key's scancode (offset by `8` to its xkb keycode) is then translated to a
//! [`Key`] — a [`Key::Character`] carrying the typed text, or a [`Key::Named`]
//! for an action key — and the [`KeyboardEvent`]'s `location` is derived from its
//! physical [`Code`]. Until a `keymap` event arrives, the keymap-less behavior
//! above still applies.
//!
//! ## Modifiers
//!
//! Without the keymap the `wl_keyboard` `modifiers` event is an opaque bitmask,
//! so the reducer ignores it and instead tracks the physical modifier keys it
//! sees pressed and released — Control, Alt, Shift, and Meta, by their
//! layout-independent [`Code`]s — assembling the modifier set through
//! [`mapping::modifiers_from_bools`]. A modifier stays active while either its
//! left or right key is held. The lock states (Caps Lock, Num Lock) and the
//! Alt Graph distinction are defined by the keymap, so with the `xkb` feature
//! the keymap interprets the `modifiers` event directly and yields the
//! authoritative set including those lock states and Alt Graph.
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
//! [`Key::Character`]: ui_events::keyboard::Key::Character
//! [`NamedKey::Unidentified`]: ui_events::keyboard::NamedKey::Unidentified
//! [`Location::Standard`]: ui_events::keyboard::Location::Standard

#[cfg(feature = "xkb")]
use std::fs::File;
#[cfg(feature = "xkb")]
use std::os::unix::io::OwnedFd;

use ui_events::keyboard::{Code, Key, KeyState, KeyboardEvent, Location, Modifiers, NamedKey};
use wayland_client::WEnum;
#[cfg(feature = "xkb")]
use wayland_client::protocol::wl_keyboard::KeymapFormat;
use wayland_client::protocol::wl_keyboard::{Event, KeyState as WlKeyState};
#[cfg(feature = "xkb")]
use xkbcommon::xkb;

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
    /// The XKB keymap state, present only under the `xkb` feature.
    #[cfg(feature = "xkb")]
    xkb: XkbState,
}

impl KeyboardEventReducer {
    /// Reduce a single `wl_keyboard` [`Event`] into an optional [`KeyboardEvent`].
    ///
    /// Only `key` events translate to a [`KeyboardEvent`]; `enter`, `leave`, and
    /// `repeat_info` update internal state and return `None`. The `keymap` and
    /// `modifiers` events feed the keymap state under the `xkb` feature and are
    /// otherwise ignored (the modifier set is then tracked from physical keys).
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
            // Under the `xkb` feature the keymap drives logical-key and
            // authoritative-modifier resolution.
            #[cfg(feature = "xkb")]
            Event::Keymap { format, fd, .. } => {
                self.xkb.set_keymap(*format, fd);
                None
            }
            #[cfg(feature = "xkb")]
            Event::Modifiers {
                mods_depressed,
                mods_latched,
                mods_locked,
                group,
                ..
            } => {
                self.xkb
                    .update_modifiers(*mods_depressed, *mods_latched, *mods_locked, *group);
                None
            }
            // Without the keymap (no `xkb` feature) the `keymap` event is unused
            // and the `modifiers` bitmask is opaque, so physical modifier keys
            // are tracked instead (see `modifiers`).
            #[cfg(not(feature = "xkb"))]
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
        // With a keymap, the modifier set — including the lock states and Alt
        // Graph — comes from the authoritative xkb state.
        #[cfg(feature = "xkb")]
        if let Some(modifiers) = self.xkb.modifiers() {
            return modifiers;
        }
        // Otherwise derive it from the physical modifier keys held.
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

        let code = mapping::code_from_evdev_scancode(scancode);
        // The logical key and location are resolved from the keymap under the
        // `xkb` feature; otherwise only the physical code is known.
        #[cfg(feature = "xkb")]
        let (key, location) = self.xkb.resolve(scancode, code);
        #[cfg(not(feature = "xkb"))]
        let (key, location) = (Key::Named(NamedKey::Unidentified), Location::Standard);

        Some(KeyboardEvent {
            state: key_state,
            key,
            code,
            location,
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

/// The XKB keymap state backing logical-key and modifier resolution.
///
/// Present only under the `xkb` feature. It owns an xkb context and, once a
/// `keymap` event has arrived, the derived keyboard state that the `modifiers`
/// events keep current.
#[cfg(feature = "xkb")]
struct XkbState {
    /// The xkb context used to compile keymaps.
    context: xkb::Context,
    /// The keyboard state for the current keymap, once one has been received.
    state: Option<xkb::State>,
}

#[cfg(feature = "xkb")]
impl Default for XkbState {
    fn default() -> Self {
        Self {
            context: xkb::Context::new(xkb::CONTEXT_NO_FLAGS),
            state: None,
        }
    }
}

#[cfg(feature = "xkb")]
impl core::fmt::Debug for XkbState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("XkbState")
            .field("keymap_loaded", &self.state.is_some())
            .finish_non_exhaustive()
    }
}

#[cfg(feature = "xkb")]
impl XkbState {
    /// Compile the keymap from a `wl_keyboard` `keymap` event and build the
    /// keyboard state from it.
    ///
    /// Only the XKB v1 text format is understood; any other format is ignored,
    /// as is a keymap that fails to compile (the previous state is kept). The
    /// file descriptor is duplicated and read rather than memory-mapped, so the
    /// reducer needs no `unsafe` and leaves the caller's descriptor undisturbed.
    fn set_keymap(&mut self, format: WEnum<KeymapFormat>, fd: &OwnedFd) {
        if !matches!(format, WEnum::Value(KeymapFormat::XkbV1)) {
            return;
        }
        let Ok(fd) = fd.try_clone() else {
            return;
        };
        let mut file = File::from(fd);
        if let Some(keymap) = xkb::Keymap::new_from_file(
            &self.context,
            &mut file,
            xkb::KEYMAP_FORMAT_TEXT_V1,
            xkb::KEYMAP_COMPILE_NO_FLAGS,
        ) {
            self.state = Some(xkb::State::new(&keymap));
        }
    }

    /// Apply a `wl_keyboard` `modifiers` event to the keyboard state.
    ///
    /// Wayland serializes the modifier and layout state, so the masks are fed in
    /// directly with [`State::update_mask`]; `group` is the effective (locked)
    /// layout.
    ///
    /// [`State::update_mask`]: xkb::State::update_mask
    fn update_modifiers(&mut self, depressed: u32, latched: u32, locked: u32, group: u32) {
        if let Some(state) = self.state.as_mut() {
            state.update_mask(depressed, latched, locked, 0, 0, group);
        }
    }

    /// The modifier set derived from the keymap state, or `None` if no keymap
    /// has arrived yet.
    fn modifiers(&self) -> Option<Modifiers> {
        let state = self.state.as_ref()?;
        Some(mapping::modifiers_from_active_mods(
            state.mod_name_is_active(xkb::MOD_NAME_CTRL, xkb::STATE_MODS_EFFECTIVE),
            state.mod_name_is_active(xkb::MOD_NAME_ALT, xkb::STATE_MODS_EFFECTIVE),
            state.mod_name_is_active(xkb::MOD_NAME_SHIFT, xkb::STATE_MODS_EFFECTIVE),
            state.mod_name_is_active(xkb::MOD_NAME_LOGO, xkb::STATE_MODS_EFFECTIVE),
            state.mod_name_is_active(xkb::MOD_NAME_CAPS, xkb::STATE_MODS_EFFECTIVE),
            state.mod_name_is_active(xkb::MOD_NAME_NUM, xkb::STATE_MODS_EFFECTIVE),
            state.mod_name_is_active(xkb::MOD_NAME_ISO_LEVEL3_SHIFT, xkb::STATE_MODS_EFFECTIVE),
        ))
    }

    /// Resolve a key's logical [`Key`] and [`Location`] from the keymap state.
    ///
    /// Without a keymap yet, this returns the keymap-less result, so the
    /// behavior matches a build without the feature until a keymap arrives.
    fn resolve(&self, scancode: u32, code: Code) -> (Key, Location) {
        match self.state.as_ref() {
            Some(state) => {
                // X11/XKB keycodes are the evdev scancodes offset by 8.
                let keycode = xkb::Keycode::new(scancode + 8);
                let keysym = state.key_get_one_sym(keycode);
                let text = state.key_get_utf8(keycode);
                (
                    mapping::key_from_keysym(keysym.raw(), &text),
                    mapping::location_from_code(code),
                )
            }
            None => (Key::Named(NamedKey::Unidentified), Location::Standard),
        }
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

    /// Compile a US-layout keymap state, or `None` if the system has no XKB
    /// keymap data (in which case the `xkb` integration tests skip).
    #[cfg(feature = "xkb")]
    fn us_keymap_state() -> Option<xkb::State> {
        let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
        let keymap = xkb::Keymap::new_from_names(
            &context,
            "",
            "",
            "us",
            "",
            None,
            xkb::KEYMAP_COMPILE_NO_FLAGS,
        )?;
        Some(xkb::State::new(&keymap))
    }

    #[cfg(feature = "xkb")]
    #[test]
    fn xkb_resolves_typed_text() {
        let Some(state) = us_keymap_state() else {
            return;
        };
        let mut reducer = KeyboardEventReducer::default();
        reducer.xkb.state = Some(state);

        // `KEY_A` produces the character 'a' on a US layout.
        let event = reducer
            .reduce(&key_event(KEY_A, WlKeyState::Pressed))
            .expect("a key press should translate");
        assert_eq!(event.key, Key::Character("a".into()));
        assert_eq!(event.code, Code::KeyA);
        assert_eq!(event.location, Location::Standard);
    }

    #[cfg(feature = "xkb")]
    #[test]
    fn xkb_resolves_named_key_and_side_location() {
        let Some(state) = us_keymap_state() else {
            return;
        };
        let mut reducer = KeyboardEventReducer::default();
        reducer.xkb.state = Some(state);

        // Enter is a named key even though xkb reports a control character for it.
        const KEY_ENTER: u32 = 28;
        let enter = reducer
            .reduce(&key_event(KEY_ENTER, WlKeyState::Pressed))
            .expect("enter should translate");
        assert_eq!(enter.key, Key::Named(NamedKey::Enter));

        // Left Shift resolves to the named modifier at the left location.
        let shift = reducer
            .reduce(&key_event(KEY_LEFTSHIFT, WlKeyState::Pressed))
            .expect("shift should translate");
        assert_eq!(shift.key, Key::Named(NamedKey::Shift));
        assert_eq!(shift.location, Location::Left);
    }

    #[cfg(feature = "xkb")]
    #[test]
    fn xkb_modifiers_come_from_keymap_state() {
        let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
        let Some(keymap) = xkb::Keymap::new_from_names(
            &context,
            "",
            "",
            "us",
            "",
            None,
            xkb::KEYMAP_COMPILE_NO_FLAGS,
        ) else {
            return;
        };
        let shift_index = keymap.mod_get_index(xkb::MOD_NAME_SHIFT);
        if shift_index == xkb::MOD_INVALID {
            return;
        }
        let mut reducer = KeyboardEventReducer::default();
        reducer.xkb.state = Some(xkb::State::new(&keymap));

        // The keymap state starts with no active modifiers.
        assert_eq!(reducer.modifiers(), Modifiers::empty());

        // Depressing the Shift modifier (as the compositor serializes it through
        // the `modifiers` event) makes the keymap-derived set report Shift.
        reducer.reduce(&Event::Modifiers {
            serial: 0,
            mods_depressed: 1_u32 << shift_index,
            mods_latched: 0,
            mods_locked: 0,
            group: 0,
        });
        assert!(reducer.modifiers().shift());
    }
}
