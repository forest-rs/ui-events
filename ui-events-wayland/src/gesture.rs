// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Reduces `zwp_pointer_gesture_pinch_v1` event streams into pinch and rotate
//! [`PointerEvent`]s.
//!
//! [`PinchGestureReducer`] mirrors the stateful-reducer pattern of the other
//! adapters: feed it each touchpad pinch-gesture event together with a
//! caller-provided monotonic timestamp, and it emits the
//! [`PointerEvent::Gesture`]s describing the scale and rotation changes.
//!
//! Wayland's pinch gesture reports an *absolute* scale relative to the start of
//! the gesture and a *per-update* clockwise rotation, whereas [`PointerGesture`]
//! is incremental on both axes. The reducer therefore tracks the previous
//! absolute scale and emits the per-update scale fraction (see
//! [`mapping::pinch_scale_fraction`]) alongside the per-update rotation in
//! radians (see [`mapping::rotation_radians_from_degrees`]). A single pinch
//! `update` thus yields up to two events — a [`PointerEvent::Gesture`] carrying a
//! [`PointerGesture::Pinch`] followed by one carrying a
//! [`PointerGesture::Rotate`] — omitting whichever component did not change.
//!
//! The pinch protocol reports only a *relative* logical center, not an absolute
//! pointer position, so the [`PointerState`] on the emitted events keeps the
//! default position; correlate it with the `wl_pointer` reducer's position if a
//! location is needed. Wayland's swipe and hold gestures, and the
//! relative-pointer protocol, have no [`ui-events`] analog and are out of scope.
//!
//! [`PointerState`]: ui_events::pointer::PointerState
//! [`ui-events`]: https://docs.rs/ui-events/

use ui_events::pointer::{
    PointerEvent, PointerGesture, PointerGestureEvent, PointerInfo, PointerState, PointerType,
};
use wayland_protocols::wp::pointer_gestures::zv1::client::zwp_pointer_gesture_pinch_v1::Event;

use crate::mapping;

/// The [`PointerInfo`] describing a seat's touchpad pinch gesture source.
///
/// Touchpad gestures arrive on the seat's `wl_pointer`, so they are reported
/// against the primary mouse pointer, matching the `wl_pointer` reducer.
fn gesture_pointer() -> PointerInfo {
    mapping::primary_pointer_info(PointerType::Mouse)
}

/// Reduces a `zwp_pointer_gesture_pinch_v1` event stream into pinch and rotate
/// [`PointerEvent`]s.
///
/// Keep one reducer per pinch-gesture object and call [`reduce`] with each
/// event. See the [module documentation] for the scale, rotation, position, and
/// timing model.
///
/// [`reduce`]: PinchGestureReducer::reduce
/// [module documentation]: self
#[derive(Debug)]
pub struct PinchGestureReducer {
    /// Absolute pinch scale reported by the most recent `update`, relative to
    /// the `begin` baseline of `1.0`. Reset to `1.0` whenever a gesture begins
    /// or ends.
    last_scale: f64,
    /// Pointer state stamped onto emitted gesture events.
    state: PointerState,
    /// Last caller-provided timestamp, for the monotonicity debug assertion.
    last_seen_time: Option<u64>,
}

impl Default for PinchGestureReducer {
    fn default() -> Self {
        Self {
            last_scale: 1.0,
            state: PointerState::default(),
            last_seen_time: None,
        }
    }
}

impl PinchGestureReducer {
    /// Reduce a single pinch-gesture [`Event`] into zero, one, or two
    /// [`PointerEvent`]s.
    ///
    /// `time` is a monotonic nanosecond timestamp in the consumer's clock
    /// domain, stamped into the [`PointerState`] of every event this call
    /// produces; Wayland's own 32-bit millisecond event timestamps are
    /// deliberately not used as the clock. `scale_factor` records the scale of
    /// the surface the gesture occurred on; it does not affect the pinch
    /// fraction or rotation, which are dimensionless.
    ///
    /// A `begin` or `end` event only updates internal state and returns no
    /// events. An `update` returns a [`PointerGesture::Pinch`] when the scale
    /// changed and a [`PointerGesture::Rotate`] when the rotation was non-zero,
    /// in that order.
    ///
    /// `time` must be monotonic across calls; a regression trips a
    /// `debug_assert!`.
    pub fn reduce(&mut self, scale_factor: f64, event: &Event, time: u64) -> Vec<PointerEvent> {
        self.check_time_monotonic(time);
        self.state.time = time;
        self.state.scale_factor = scale_factor;

        match event {
            // `begin` carries a `wl_surface` that cannot be constructed without
            // a live connection; only the scale baseline reset is needed, so all
            // fields are ignored.
            Event::Begin { .. } => {
                self.last_scale = 1.0;
                Vec::new()
            }
            Event::Update {
                scale, rotation, ..
            } => self.update(*scale, *rotation),
            Event::End { .. } => {
                self.last_scale = 1.0;
                Vec::new()
            }
            // `Event` is `#[non_exhaustive]`; ignore future additions.
            _ => Vec::new(),
        }
    }

    /// Handle an `update` event: emit the per-update pinch and rotate deltas.
    fn update(&mut self, scale: f64, rotation: f64) -> Vec<PointerEvent> {
        let pinch = mapping::pinch_scale_fraction(self.last_scale, scale);
        // Remember the absolute scale for the next update's delta, ignoring a
        // degenerate value so one bad update cannot poison the baseline.
        if scale.is_finite() && scale > 0.0 {
            self.last_scale = scale;
        }
        let rotate = mapping::rotation_radians_from_degrees(rotation);

        let mut events = Vec::new();
        if pinch != 0.0 {
            events.push(self.gesture_event(PointerGesture::Pinch(pinch)));
        }
        if rotate != 0.0 {
            events.push(self.gesture_event(PointerGesture::Rotate(rotate)));
        }
        events
    }

    /// Build a [`PointerEvent::Gesture`] for `gesture` with the current state.
    fn gesture_event(&self, gesture: PointerGesture) -> PointerEvent {
        PointerEvent::Gesture(PointerGestureEvent {
            pointer: gesture_pointer(),
            gesture,
            state: self.state.clone(),
        })
    }

    /// Debug-assert that caller timestamps do not regress.
    fn check_time_monotonic(&mut self, time: u64) {
        if let Some(previous) = self.last_seen_time {
            debug_assert!(
                time >= previous,
                "PinchGestureReducer::reduce timestamps must be monotonic nanoseconds"
            );
        }
        self.last_seen_time = Some(time);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build an `update` event. Pinch updates carry no surface, so unlike
    /// `begin` they can be constructed directly in tests.
    fn update(scale: f64, rotation: f64) -> Event {
        Event::Update {
            time: 0,
            dx: 0.0,
            dy: 0.0,
            scale,
            rotation,
        }
    }

    /// Build an `end` event.
    fn end() -> Event {
        Event::End {
            serial: 0,
            time: 0,
            cancelled: 0,
        }
    }

    fn pinch_fraction(event: &PointerEvent) -> f32 {
        match event {
            PointerEvent::Gesture(PointerGestureEvent {
                gesture: PointerGesture::Pinch(fraction),
                ..
            }) => *fraction,
            other => panic!("expected a pinch gesture, got {other:?}"),
        }
    }

    fn rotation_radians(event: &PointerEvent) -> f32 {
        match event {
            PointerEvent::Gesture(PointerGestureEvent {
                gesture: PointerGesture::Rotate(radians),
                ..
            }) => *radians,
            other => panic!("expected a rotate gesture, got {other:?}"),
        }
    }

    #[test]
    fn scale_change_emits_pinch_against_begin_baseline() {
        let mut reducer = PinchGestureReducer::default();
        // No `begin` is required: the baseline starts at 1.0.
        let events = reducer.reduce(1.0, &update(1.1, 0.0), 10);

        assert_eq!(events.len(), 1);
        assert!((pinch_fraction(&events[0]) - 0.1).abs() < 1e-6);
        // Touchpad gestures are reported against the primary mouse pointer.
        assert!(events[0].is_primary_pointer());
    }

    #[test]
    fn successive_updates_report_incremental_pinch() {
        let mut reducer = PinchGestureReducer::default();
        let first = reducer.reduce(1.0, &update(1.1, 0.0), 1);
        let second = reducer.reduce(1.0, &update(1.21, 0.0), 2);

        assert!((pinch_fraction(&first[0]) - 0.1).abs() < 1e-5);
        // 1.21 / 1.1 - 1 == 0.1, not 0.21, because the protocol's scale is
        // absolute relative to `begin`, not per-update.
        assert!((pinch_fraction(&second[0]) - 0.1).abs() < 1e-5);
    }

    #[test]
    fn rotation_is_emitted_as_clockwise_radians() {
        let mut reducer = PinchGestureReducer::default();
        let events = reducer.reduce(1.0, &update(1.0, 90.0), 1);

        assert_eq!(events.len(), 1);
        assert!((rotation_radians(&events[0]) - core::f32::consts::FRAC_PI_2).abs() < 1e-6);
    }

    #[test]
    fn scale_and_rotation_emit_pinch_then_rotate() {
        let mut reducer = PinchGestureReducer::default();
        let events = reducer.reduce(1.0, &update(1.5, 30.0), 1);

        assert_eq!(events.len(), 2);
        assert!((pinch_fraction(&events[0]) - 0.5).abs() < 1e-6);
        assert!((rotation_radians(&events[1]) - 30.0_f32.to_radians()).abs() < 1e-6);
    }

    #[test]
    fn noop_update_emits_nothing() {
        let mut reducer = PinchGestureReducer::default();
        // Scale unchanged from the 1.0 baseline and no rotation.
        assert!(reducer.reduce(1.0, &update(1.0, 0.0), 1).is_empty());
    }

    #[test]
    fn end_resets_the_scale_baseline() {
        let mut reducer = PinchGestureReducer::default();
        let _ = reducer.reduce(1.0, &update(2.0, 0.0), 1);
        // End the gesture; the next gesture's first update is relative to 1.0.
        let _ = reducer.reduce(1.0, &end(), 2);
        let events = reducer.reduce(1.0, &update(1.1, 0.0), 3);

        assert!((pinch_fraction(&events[0]) - 0.1).abs() < 1e-6);
    }

    #[test]
    fn state_carries_time_and_scale_factor() {
        let mut reducer = PinchGestureReducer::default();
        let events = reducer.reduce(2.0, &update(1.1, 0.0), 42);

        let PointerEvent::Gesture(gesture) = &events[0] else {
            panic!("expected a gesture, got {:?}", events[0]);
        };
        assert_eq!(gesture.state.time, 42);
        assert_eq!(gesture.state.scale_factor, 2.0);
    }

    #[test]
    #[should_panic(expected = "timestamps must be monotonic nanoseconds")]
    fn non_monotonic_time_panics_in_debug() {
        let mut reducer = PinchGestureReducer::default();
        let _ = reducer.reduce(1.0, &update(1.0, 0.0), 2_000);
        let _ = reducer.reduce(1.0, &update(1.0, 0.0), 1_000);
    }
}
