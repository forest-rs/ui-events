// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Aggregates a seat's per-device reducers behind one `wl_seat`.
//!
//! A Wayland `wl_seat` groups the input devices a user operates together —
//! typically a pointer, a keyboard, and a touchscreen — and advertises which of
//! them it has through its `capabilities` event. [`SeatReducer`] tracks those
//! capabilities and owns the matching per-device reducers
//! ([`PointerEventReducer`], [`KeyboardEventReducer`], and
//! [`TouchEventReducer`]), so a consumer can route every event for a seat
//! through one object and receive a uniform [`SeatEventTranslation`] stream of
//! [`PointerEvent`]s and [`KeyboardEvent`]s.
//!
//! Feed the seat's own events to [`reduce_seat`] and each device's events to
//! [`reduce_pointer`], [`reduce_keyboard`], or [`reduce_touch`]. A device
//! reducer is created the first time it is needed and dropped when its
//! capability is withdrawn, so losing a device clears its transient state (held
//! buttons, pressed keys) instead of leaving it stuck.
//!
//! Pointer gestures (`zwp_pointer_gestures_v1`) and tablet tools
//! (`zwp_tablet_v2`) are exposed through their own globals rather than as
//! `wl_seat` capabilities, so they are outside this aggregation. Drive
//! [`crate::gesture`] and [`crate::tablet`] directly and wrap their
//! [`PointerEvent`]s in [`SeatEventTranslation::Pointer`] to place them on the
//! same stream.
//!
//! [`reduce_seat`]: SeatReducer::reduce_seat
//! [`reduce_pointer`]: SeatReducer::reduce_pointer
//! [`reduce_keyboard`]: SeatReducer::reduce_keyboard
//! [`reduce_touch`]: SeatReducer::reduce_touch
//! [`KeyboardEvent`]: ui_events::keyboard::KeyboardEvent
//! [`PointerEvent`]: ui_events::pointer::PointerEvent

use alloc::vec::Vec;

use ui_events::keyboard::KeyboardEvent;
use ui_events::pointer::PointerEvent;
use wayland_client::WEnum;
use wayland_client::protocol::wl_seat::Capability;
use wayland_client::protocol::{wl_keyboard, wl_pointer, wl_seat, wl_touch};

use crate::keyboard::KeyboardEventReducer;
use crate::pointer::PointerEventReducer;
use crate::touch::TouchEventReducer;

/// The input devices a `wl_seat` currently advertises.
///
/// Produced from a `wl_seat::capabilities` event by [`SeatReducer::reduce_seat`]
/// and readable through [`SeatReducer::capabilities`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SeatCapabilities {
    /// Whether the seat has a pointer, such as a mouse or touchpad.
    pub pointer: bool,
    /// Whether the seat has one or more keyboards.
    pub keyboard: bool,
    /// Whether the seat has a touchscreen.
    pub touch: bool,
}

/// A [`ui-events`] event produced by a [`SeatReducer`].
///
/// This unifies the two event families a seat's devices produce, mirroring the
/// translation enum of the other adapters.
///
/// [`ui-events`]: https://docs.rs/ui-events/
#[derive(Debug)]
pub enum SeatEventTranslation {
    /// A keyboard event from the seat's keyboard.
    Keyboard(KeyboardEvent),
    /// A pointer event from the seat's pointer or touchscreen.
    Pointer(PointerEvent),
}

/// Aggregates a `wl_seat`'s pointer, keyboard, and touch reducers.
///
/// Keep one instance per seat. Call [`reduce_seat`] with the seat's own events
/// to track its [`capabilities`], and [`reduce_pointer`], [`reduce_keyboard`],
/// and [`reduce_touch`] with the corresponding device events; each returns the
/// translated [`SeatEventTranslation`]s. See the [module documentation] for the
/// model.
///
/// The device reducers are reachable through [`pointer`], [`keyboard`], and
/// [`touch`] once created, for reading device state such as the pointer's
/// [`enter_serial`] or the keyboard's [`repeat_info`].
///
/// [`reduce_seat`]: SeatReducer::reduce_seat
/// [`reduce_pointer`]: SeatReducer::reduce_pointer
/// [`reduce_keyboard`]: SeatReducer::reduce_keyboard
/// [`reduce_touch`]: SeatReducer::reduce_touch
/// [`capabilities`]: SeatReducer::capabilities
/// [`pointer`]: SeatReducer::pointer
/// [`keyboard`]: SeatReducer::keyboard
/// [`touch`]: SeatReducer::touch
/// [`enter_serial`]: PointerEventReducer::enter_serial
/// [`repeat_info`]: KeyboardEventReducer::repeat_info
/// [module documentation]: self
#[derive(Debug, Default)]
pub struct SeatReducer {
    /// The bound `wl_seat` version used to construct the pointer reducer;
    /// `None` selects the frame-batched [`Default`].
    version: Option<u32>,
    /// Capabilities last advertised by the seat.
    capabilities: SeatCapabilities,
    /// Pointer reducer, created on the first pointer event.
    pointer: Option<PointerEventReducer>,
    /// Keyboard reducer, created on the first keyboard event.
    keyboard: Option<KeyboardEventReducer>,
    /// Touch reducer, created on the first touch event.
    touch: Option<TouchEventReducer>,
}

impl SeatReducer {
    /// Create a reducer for a `wl_seat` bound at the given protocol `version`.
    ///
    /// A seat's `wl_pointer`, `wl_keyboard`, and `wl_touch` inherit the seat's
    /// version. The version currently only affects the pointer reducer's scroll
    /// batching: `wl_pointer` version 5 introduced the `frame` event that groups
    /// a scroll's axes, so earlier pointers emit a scroll per axis event (see
    /// [`PointerEventReducer::for_version`]). The [`Default`] reducer assumes
    /// frame batching, which is correct for every `wl_pointer` from version 5 on.
    pub fn for_version(version: u32) -> Self {
        Self {
            version: Some(version),
            ..Self::default()
        }
    }

    /// The capabilities most recently advertised by the seat.
    ///
    /// This reflects the last `wl_seat::capabilities` event passed to
    /// [`reduce_seat`](Self::reduce_seat); it is all-`false` until one arrives,
    /// independent of which device reducers have been created by routing events.
    pub fn capabilities(&self) -> SeatCapabilities {
        self.capabilities
    }

    /// The pointer reducer, once a pointer event has been routed to it.
    ///
    /// Useful for reading pointer state such as
    /// [`PointerEventReducer::enter_serial`].
    pub fn pointer(&self) -> Option<&PointerEventReducer> {
        self.pointer.as_ref()
    }

    /// The keyboard reducer, once a keyboard event has been routed to it.
    ///
    /// Useful for reading keyboard state such as
    /// [`KeyboardEventReducer::repeat_info`] and
    /// [`KeyboardEventReducer::modifiers`].
    pub fn keyboard(&self) -> Option<&KeyboardEventReducer> {
        self.keyboard.as_ref()
    }

    /// The touch reducer, once a touch event has been routed to it.
    pub fn touch(&self) -> Option<&TouchEventReducer> {
        self.touch.as_ref()
    }

    /// Process a `wl_seat` event, returning the seat's current capabilities.
    ///
    /// A `capabilities` event updates the tracked [`SeatCapabilities`]; when a
    /// capability is withdrawn, the corresponding device reducer is dropped so
    /// its transient state does not persist across the device's removal. Other
    /// `wl_seat` events (such as `name`) leave the capabilities unchanged.
    pub fn reduce_seat(&mut self, event: &wl_seat::Event) -> SeatCapabilities {
        if let wl_seat::Event::Capabilities { capabilities } = event {
            let capabilities = seat_capabilities(*capabilities);
            self.capabilities = capabilities;
            if !capabilities.pointer {
                self.pointer = None;
            }
            if !capabilities.keyboard {
                self.keyboard = None;
            }
            if !capabilities.touch {
                self.touch = None;
            }
        }
        self.capabilities
    }

    /// Process a `wl_pointer` event, returning any resulting pointer events.
    ///
    /// `scale_factor` and `time` are forwarded to [`PointerEventReducer::reduce`];
    /// see it for their meaning and requirements. The pointer reducer is created
    /// on first use.
    pub fn reduce_pointer(
        &mut self,
        scale_factor: f64,
        event: &wl_pointer::Event,
        time: u64,
    ) -> Vec<SeatEventTranslation> {
        let version = self.version;
        self.pointer
            .get_or_insert_with(|| match version {
                Some(version) => PointerEventReducer::for_version(version),
                None => PointerEventReducer::default(),
            })
            .reduce(scale_factor, event, time)
            .into_iter()
            .map(SeatEventTranslation::Pointer)
            .collect()
    }

    /// Process a `wl_keyboard` event, returning any resulting keyboard event.
    ///
    /// See [`KeyboardEventReducer::reduce`]. The keyboard reducer is created on
    /// first use.
    pub fn reduce_keyboard(&mut self, event: &wl_keyboard::Event) -> Option<SeatEventTranslation> {
        self.keyboard
            .get_or_insert_with(KeyboardEventReducer::default)
            .reduce(event)
            .map(SeatEventTranslation::Keyboard)
    }

    /// Process a `wl_touch` event, returning any resulting pointer events.
    ///
    /// `scale_factor` and `time` are forwarded to [`TouchEventReducer::reduce`];
    /// see it for their meaning and requirements. The touch reducer is created
    /// on first use.
    pub fn reduce_touch(
        &mut self,
        scale_factor: f64,
        event: &wl_touch::Event,
        time: u64,
    ) -> Vec<SeatEventTranslation> {
        self.touch
            .get_or_insert_with(TouchEventReducer::default)
            .reduce(scale_factor, event, time)
            .into_iter()
            .map(SeatEventTranslation::Pointer)
            .collect()
    }
}

/// Convert a `wl_seat` capability bitmask into [`SeatCapabilities`].
///
/// Bits from a newer protocol version are unknown to the locked bindings, which
/// makes the whole mask deserialize as [`WEnum::Unknown`]. Truncating to the
/// known bits keeps the recognized capabilities rather than discarding them all.
fn seat_capabilities(capabilities: WEnum<Capability>) -> SeatCapabilities {
    let capabilities = match capabilities {
        WEnum::Value(capabilities) => capabilities,
        WEnum::Unknown(bits) => Capability::from_bits_truncate(bits),
    };
    SeatCapabilities {
        pointer: capabilities.contains(Capability::Pointer),
        keyboard: capabilities.contains(Capability::Keyboard),
        touch: capabilities.contains(Capability::Touch),
    }
}

#[cfg(test)]
mod tests {
    use ui_events::pointer::PointerEvent;
    use wayland_client::WEnum;
    use wayland_client::protocol::wl_keyboard::{self, KeyState as WlKeyState};
    use wayland_client::protocol::wl_pointer;
    use wayland_client::protocol::wl_seat::{self, Capability};
    use wayland_client::protocol::wl_touch;

    use super::{SeatCapabilities, SeatEventTranslation, SeatReducer, seat_capabilities};

    /// `KEY_A` evdev scancode, a convenient non-modifier key.
    const KEY_A: u32 = 30;

    fn pointer_motion(x: f64, y: f64) -> wl_pointer::Event {
        wl_pointer::Event::Motion {
            time: 0,
            surface_x: x,
            surface_y: y,
        }
    }

    fn key_press(key: u32) -> wl_keyboard::Event {
        wl_keyboard::Event::Key {
            serial: 0,
            time: 0,
            key,
            state: WEnum::Value(WlKeyState::Pressed),
        }
    }

    fn capabilities(capabilities: WEnum<Capability>) -> wl_seat::Event {
        wl_seat::Event::Capabilities { capabilities }
    }

    #[test]
    fn seat_capabilities_reads_known_bits() {
        assert_eq!(
            seat_capabilities(WEnum::Value(Capability::Pointer | Capability::Touch)),
            SeatCapabilities {
                pointer: true,
                keyboard: false,
                touch: true,
            }
        );
        assert_eq!(
            seat_capabilities(WEnum::Value(Capability::empty())),
            SeatCapabilities::default()
        );
    }

    #[test]
    fn seat_capabilities_ignores_unknown_bits() {
        // Bit `0b1000` is not a known capability; truncating must keep the
        // pointer (`0b1`) and keyboard (`0b10`) bits rather than dropping all.
        assert_eq!(
            seat_capabilities(WEnum::Unknown(0b1011)),
            SeatCapabilities {
                pointer: true,
                keyboard: true,
                touch: false,
            }
        );
    }

    #[test]
    fn reduce_seat_tracks_capabilities() {
        let mut seat = SeatReducer::default();

        let caps = seat.reduce_seat(&capabilities(WEnum::Value(
            Capability::Pointer | Capability::Keyboard,
        )));

        let expected = SeatCapabilities {
            pointer: true,
            keyboard: true,
            touch: false,
        };
        assert_eq!(caps, expected);
        assert_eq!(seat.capabilities(), expected);
    }

    #[test]
    fn pointer_events_are_routed_and_wrapped() {
        let mut seat = SeatReducer::default();

        let out = seat.reduce_pointer(1.0, &pointer_motion(4.0, 5.0), 1_000);

        assert_eq!(out.len(), 1);
        assert!(matches!(
            out[0],
            SeatEventTranslation::Pointer(PointerEvent::Move(_))
        ));
        // The reducer is created on first use and then exposed.
        assert!(seat.pointer().is_some());
    }

    #[test]
    fn for_version_routes_pointer_events() {
        // A pre-frame pointer version still produces a working reducer.
        let mut seat = SeatReducer::for_version(1);

        let out = seat.reduce_pointer(1.0, &pointer_motion(0.0, 0.0), 10);

        assert_eq!(out.len(), 1);
        assert!(seat.pointer().is_some());
    }

    #[test]
    fn keyboard_events_are_routed_and_wrapped() {
        let mut seat = SeatReducer::default();

        let out = seat.reduce_keyboard(&key_press(KEY_A));

        assert!(matches!(out, Some(SeatEventTranslation::Keyboard(_))));
        assert!(seat.keyboard().is_some());
    }

    #[test]
    fn touch_events_are_routed() {
        let mut seat = SeatReducer::default();

        // A bare `frame` with no active contacts flushes nothing, but routing it
        // still creates the touch reducer.
        let out = seat.reduce_touch(1.0, &wl_touch::Event::Frame, 1_000);

        assert!(out.is_empty());
        assert!(seat.touch().is_some());
    }

    #[test]
    fn lost_capability_drops_reducer() {
        let mut seat = SeatReducer::default();

        // Route a pointer event so the pointer reducer exists.
        let _ = seat.reduce_pointer(1.0, &pointer_motion(1.0, 1.0), 1_000);
        assert!(seat.pointer().is_some());

        // Losing the pointer capability drops the reducer and its state.
        let caps = seat.reduce_seat(&capabilities(WEnum::Value(Capability::empty())));
        assert!(!caps.pointer);
        assert!(seat.pointer().is_none());
    }
}
