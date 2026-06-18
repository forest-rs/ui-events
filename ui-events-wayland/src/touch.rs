// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Reduces `wl_touch` event streams into [`PointerEvent`]s.
//!
//! [`TouchEventReducer`] mirrors the stateful-reducer pattern of the other
//! adapters: feed it each `wl_touch` event for a seat's touch device together
//! with a caller-provided monotonic timestamp, and it tracks every active
//! contact and emits touch [`PointerEvent`]s.
//!
//! Wayland batches the touch changes that logically belong together — for
//! example two fingers moving at once, or a contact's position alongside its
//! updated shape — between `frame` events, and the order of `motion`, `shape`,
//! and `orientation` within a frame is unspecified. The reducer therefore
//! records each contact's new state as the events arrive and emits the resulting
//! [`PointerEvent`]s only on `frame`, so a contact's geometry and orientation
//! are fully assembled before its event is produced. Every emitted event carries
//! `pointer_type` [`PointerType::Touch`] and no [`PointerButton`].
//!
//! Contacts are identified by their `wl_touch` contact id. The lowest active id
//! is mapped to [`PointerId::PRIMARY`] and the rest are offset to avoid colliding
//! with it; see [`mapping::touch_pointer_info`].
//!
//! [`PointerState`]: ui_events::pointer::PointerState
//! [`PointerButton`]: ui_events::pointer::PointerButton
//! [`PointerType::Touch`]: ui_events::pointer::PointerType::Touch
//! [`PointerId::PRIMARY`]: ui_events::pointer::PointerId::PRIMARY

use ui_events::pointer::{
    PointerButtonEvent, PointerEvent, PointerInfo, PointerState, PointerUpdate,
};
use wayland_client::protocol::wl_touch::Event;

use crate::mapping;
use crate::pointer::TapCounter;

/// Pressure reported while a touch contact is active.
///
/// `wl_touch` does not report pressure, so contacts follow the `ui-events`
/// convention of `0.5` while down and `0.0` once released.
const ACTIVE_PRESSURE: f32 = 0.5;
/// Pressure reported once a touch contact is released or cancelled.
const RELEASED_PRESSURE: f32 = 0.0;

/// The latest change recorded for a contact within the current `frame`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    /// The contact was established (`wl_touch::down`).
    Down,
    /// The contact moved (`wl_touch::motion`).
    Motion,
    /// The contact was released (`wl_touch::up`).
    Up,
}

/// A pending per-contact change to flush on the next `frame`.
#[derive(Debug)]
struct PendingChange {
    /// The `wl_touch` contact id.
    id: i32,
    /// The latest phase seen for this contact in the current frame.
    phase: Phase,
}

/// An active touch contact and its accumulated state.
#[derive(Debug)]
struct TouchContact {
    /// The `wl_touch` contact id, unique among active contacts.
    id: i32,
    /// State accumulated from `down`, `motion`, `shape`, and `orientation`.
    state: PointerState,
}

/// Reduces a `wl_touch` event stream into [`PointerEvent`]s.
///
/// Keep one reducer per seat touch device and call [`reduce`] with each
/// `wl_touch` event. See the [module documentation] for the batching, identity,
/// and timing model.
///
/// [`reduce`]: TouchEventReducer::reduce
/// [module documentation]: self
#[derive(Debug, Default)]
pub struct TouchEventReducer {
    /// Active contacts, keyed by `wl_touch` contact id.
    contacts: Vec<TouchContact>,
    /// Per-contact changes accumulated since the last `frame`.
    pending: Vec<PendingChange>,
    /// Multi-tap counter, shared with the pointer reducer.
    counter: TapCounter,
    /// Last caller-provided timestamp, for the monotonicity debug assertion.
    last_seen_time: Option<u64>,
}

impl TouchEventReducer {
    /// Reduce a single `wl_touch` [`Event`] into zero or more [`PointerEvent`]s.
    ///
    /// `time` is a monotonic nanosecond timestamp in the consumer's clock
    /// domain, stamped into the [`PointerState`] of every contact this call
    /// updates; Wayland's own 32-bit millisecond event timestamps are
    /// deliberately not used as the clock. `scale_factor` converts surface-local
    /// logical coordinates and contact dimensions to physical pixels.
    ///
    /// Down, motion, up, shape, and orientation events update the affected
    /// contact but emit nothing; the accumulated [`PointerEvent`]s are returned
    /// from the `frame` event that closes the batch. A `cancel` event returns a
    /// [`PointerEvent::Cancel`] for every active contact immediately.
    ///
    /// `time` must be monotonic across calls; a regression trips a
    /// `debug_assert!`, since tap detection measures durations in nanoseconds.
    pub fn reduce(&mut self, scale_factor: f64, event: &Event, time: u64) -> Vec<PointerEvent> {
        self.check_time_monotonic(time);

        match event {
            // `down` carries a `wl_surface` that cannot be constructed without a
            // live connection, so the surface-independent logic lives in `down`.
            Event::Down { id, x, y, .. } => {
                self.down(*id, *x, *y, time, scale_factor);
                Vec::new()
            }
            Event::Motion { id, x, y, .. } => {
                self.motion(*id, *x, *y, time, scale_factor);
                Vec::new()
            }
            Event::Up { id, .. } => {
                self.up(*id, time, scale_factor);
                Vec::new()
            }
            Event::Shape { id, major, minor } => {
                self.shape(*id, *major, *minor, scale_factor);
                Vec::new()
            }
            Event::Orientation { id, orientation } => {
                self.orientation(*id, *orientation);
                Vec::new()
            }
            Event::Frame => self.flush(scale_factor),
            Event::Cancel => self.cancel(scale_factor),
            // `wl_touch::Event` is `#[non_exhaustive]`; ignore future additions.
            _ => Vec::new(),
        }
    }

    /// Handle a `down` event: (re)establish a contact at the given surface-local
    /// logical coordinates and record it for the next `frame`.
    ///
    /// This takes the coordinates rather than the `wl_touch::down` event, whose
    /// `wl_surface` cannot be constructed without a live connection, so the
    /// surface-independent logic stays unit-testable.
    fn down(&mut self, id: i32, surface_x: f64, surface_y: f64, time: u64, scale_factor: f64) {
        let state = PointerState {
            time,
            position: mapping::physical_position_from_logical(surface_x, surface_y, scale_factor),
            pressure: ACTIVE_PRESSURE,
            scale_factor,
            ..PointerState::default()
        };
        if let Some(index) = self.contacts.iter().position(|c| c.id == id) {
            self.contacts[index].state = state;
        } else {
            self.contacts.push(TouchContact { id, state });
        }
        self.mark_pending(id, Phase::Down);
    }

    /// Handle a `motion` event: update an existing contact's position.
    fn motion(&mut self, id: i32, surface_x: f64, surface_y: f64, time: u64, scale_factor: f64) {
        let Some(index) = self.contacts.iter().position(|c| c.id == id) else {
            return;
        };
        let state = &mut self.contacts[index].state;
        state.time = time;
        state.scale_factor = scale_factor;
        state.position =
            mapping::physical_position_from_logical(surface_x, surface_y, scale_factor);
        state.pressure = ACTIVE_PRESSURE;
        self.mark_pending(id, Phase::Motion);
    }

    /// Handle an `up` event: mark an existing contact released.
    fn up(&mut self, id: i32, time: u64, scale_factor: f64) {
        let Some(index) = self.contacts.iter().position(|c| c.id == id) else {
            return;
        };
        let state = &mut self.contacts[index].state;
        state.time = time;
        state.scale_factor = scale_factor;
        state.pressure = RELEASED_PRESSURE;
        self.mark_pending(id, Phase::Up);
    }

    /// Handle a `shape` event: update a contact's [`ContactGeometry`].
    ///
    /// [`ContactGeometry`]: ui_events::pointer::ContactGeometry
    fn shape(&mut self, id: i32, major: f64, minor: f64, scale_factor: f64) {
        if let Some(index) = self.contacts.iter().position(|c| c.id == id) {
            self.contacts[index].state.contact_geometry =
                mapping::contact_geometry_from_shape(major, minor, scale_factor);
        }
    }

    /// Handle an `orientation` event: update a contact's orientation.
    fn orientation(&mut self, id: i32, orientation_deg: f64) {
        if let Some(index) = self.contacts.iter().position(|c| c.id == id) {
            self.contacts[index].state.orientation =
                mapping::touch_orientation_from_degrees(orientation_deg);
        }
    }

    /// Emit the changes accumulated since the last `frame`, then drop any
    /// contacts that were released.
    fn flush(&mut self, scale_factor: f64) -> Vec<PointerEvent> {
        if self.pending.is_empty() {
            return Vec::new();
        }
        let Some(primary_id) = self.contacts.iter().map(|c| c.id).min() else {
            self.pending.clear();
            return Vec::new();
        };
        let pending = core::mem::take(&mut self.pending);

        let mut events = Vec::with_capacity(pending.len());
        for change in &pending {
            let Some(contact) = self.contacts.iter().find(|c| c.id == change.id) else {
                continue;
            };
            let state = contact.state.clone();
            let pointer = mapping::touch_pointer_info(change.id as u64, primary_id as u64);
            let event = match change.phase {
                Phase::Down => PointerEvent::Down(PointerButtonEvent {
                    button: None,
                    pointer,
                    state,
                }),
                Phase::Motion => PointerEvent::Move(PointerUpdate {
                    pointer,
                    current: state,
                    coalesced: Vec::new(),
                    predicted: Vec::new(),
                }),
                Phase::Up => PointerEvent::Up(PointerButtonEvent {
                    button: None,
                    pointer,
                    state,
                }),
            };
            events.push(self.counter.attach_count(scale_factor, event));
        }

        // Drop contacts that were released in this frame.
        self.contacts
            .retain(|c| !pending.iter().any(|p| p.id == c.id && p.phase == Phase::Up));

        events
    }

    /// Handle a `cancel` event: emit a [`PointerEvent::Cancel`] for every active
    /// contact and forget all touch state.
    fn cancel(&mut self, scale_factor: f64) -> Vec<PointerEvent> {
        let pointers: Vec<PointerInfo> = match self.contacts.iter().map(|c| c.id).min() {
            Some(primary_id) => self
                .contacts
                .iter()
                .map(|c| mapping::touch_pointer_info(c.id as u64, primary_id as u64))
                .collect(),
            None => Vec::new(),
        };
        self.contacts.clear();
        self.pending.clear();
        pointers
            .into_iter()
            .map(|pointer| {
                self.counter
                    .attach_count(scale_factor, PointerEvent::Cancel(pointer))
            })
            .collect()
    }

    /// Record the latest [`Phase`] seen for a contact in the current frame.
    fn mark_pending(&mut self, id: i32, phase: Phase) {
        if let Some(index) = self.pending.iter().position(|p| p.id == id) {
            self.pending[index].phase = phase;
        } else {
            self.pending.push(PendingChange { id, phase });
        }
    }

    /// Debug-assert that caller timestamps do not regress.
    fn check_time_monotonic(&mut self, time: u64) {
        if let Some(previous) = self.last_seen_time {
            debug_assert!(
                time >= previous,
                "TouchEventReducer::reduce timestamps must be monotonic nanoseconds"
            );
        }
        self.last_seen_time = Some(time);
    }
}

#[cfg(test)]
mod tests {
    use dpi::{PhysicalPosition, PhysicalSize};
    use ui_events::pointer::{PointerId, PointerType};

    use super::*;

    fn frame() -> Event {
        Event::Frame
    }

    fn motion(id: i32, x: f64, y: f64) -> Event {
        Event::Motion { time: 0, id, x, y }
    }

    fn up(id: i32) -> Event {
        Event::Up {
            serial: 0,
            time: 0,
            id,
        }
    }

    #[test]
    fn single_contact_down_is_primary_with_scaled_position_and_pressure() {
        let mut reducer = TouchEventReducer::default();
        reducer.down(4, 10.0, 20.0, 100, 2.0);
        let events = reducer.reduce(2.0, &frame(), 100);

        assert_eq!(events.len(), 1);
        let PointerEvent::Down(down) = &events[0] else {
            panic!("expected a touch down, got {:?}", events[0]);
        };
        assert!(down.pointer.is_primary_pointer());
        assert_eq!(down.pointer.pointer_type, PointerType::Touch);
        assert_eq!(down.button, None);
        assert_eq!(down.state.time, 100);
        assert_eq!(down.state.scale_factor, 2.0);
        assert_eq!(down.state.position, PhysicalPosition { x: 20.0, y: 40.0 });
        assert_eq!(down.state.pressure, ACTIVE_PRESSURE);
        assert_eq!(down.state.count, 1);
    }

    #[test]
    fn motion_updates_position_and_keeps_primary() {
        let mut reducer = TouchEventReducer::default();
        reducer.down(4, 0.0, 0.0, 1, 1.0);
        let _ = reducer.reduce(1.0, &frame(), 1);

        let _ = reducer.reduce(1.0, &motion(4, 30.0, 40.0), 2);
        let events = reducer.reduce(1.0, &frame(), 2);

        assert_eq!(events.len(), 1);
        let PointerEvent::Move(update) = &events[0] else {
            panic!("expected a touch move, got {:?}", events[0]);
        };
        assert!(update.pointer.is_primary_pointer());
        assert_eq!(update.pointer.pointer_type, PointerType::Touch);
        assert_eq!(
            update.current.position,
            PhysicalPosition { x: 30.0, y: 40.0 }
        );
    }

    #[test]
    fn up_releases_contact_with_zero_pressure_and_drops_it() {
        let mut reducer = TouchEventReducer::default();
        reducer.down(4, 0.0, 0.0, 1, 1.0);
        let _ = reducer.reduce(1.0, &frame(), 1);

        let _ = reducer.reduce(1.0, &up(4), 2);
        let release = reducer.reduce(1.0, &frame(), 2);
        assert_eq!(release.len(), 1);
        let PointerEvent::Up(up) = &release[0] else {
            panic!("expected a touch up, got {:?}", release[0]);
        };
        assert!(up.pointer.is_primary_pointer());
        assert_eq!(up.state.pressure, RELEASED_PRESSURE);

        // The contact was dropped, so a later, higher-id touch becomes primary
        // (the lowest active id) rather than being offset behind a lingering id.
        reducer.down(9, 0.0, 0.0, 3, 1.0);
        let next = reducer.reduce(1.0, &frame(), 3);
        assert!(next[0].is_primary_pointer());
    }

    #[test]
    fn lowest_concurrent_contact_is_primary_others_are_offset() {
        let mut reducer = TouchEventReducer::default();
        // Recorded id 5 first, then the lower id 2.
        reducer.down(5, 0.0, 0.0, 1, 1.0);
        reducer.down(2, 0.0, 0.0, 1, 1.0);
        let events = reducer.reduce(1.0, &frame(), 1);

        assert_eq!(events.len(), 2);
        let PointerEvent::Down(higher) = &events[0] else {
            panic!("expected a touch down for id 5, got {:?}", events[0]);
        };
        let PointerEvent::Down(lower) = &events[1] else {
            panic!("expected a touch down for id 2, got {:?}", events[1]);
        };
        // The lowest active id (2) is the primary pointer.
        assert!(lower.pointer.is_primary_pointer());
        // The higher id (5) is offset by `POINTER_ID_OFFSET` (2) to 7.
        assert!(!higher.pointer.is_primary_pointer());
        assert_eq!(higher.pointer.pointer_id, PointerId::new(7));
    }

    #[test]
    fn shape_and_orientation_within_frame_populate_state() {
        let mut reducer = TouchEventReducer::default();
        reducer.down(0, 5.0, 5.0, 1, 1.0);
        let _ = reducer.reduce(
            1.0,
            &Event::Shape {
                id: 0,
                major: 10.0,
                minor: 4.0,
            },
            1,
        );
        let _ = reducer.reduce(
            1.0,
            &Event::Orientation {
                id: 0,
                orientation: 0.0,
            },
            1,
        );
        let events = reducer.reduce(1.0, &frame(), 1);

        let PointerEvent::Down(down) = &events[0] else {
            panic!("expected a touch down, got {:?}", events[0]);
        };
        // Minor axis maps to width, major to height.
        assert_eq!(
            down.state.contact_geometry,
            PhysicalSize {
                width: 4.0,
                height: 10.0
            }
        );
        // A touch contact lies flat, so altitude stays perpendicular.
        assert_eq!(
            down.state.orientation.altitude,
            core::f32::consts::FRAC_PI_2
        );
    }

    #[test]
    fn cancel_emits_for_all_contacts_and_clears_state() {
        let mut reducer = TouchEventReducer::default();
        reducer.down(0, 0.0, 0.0, 1, 1.0);
        reducer.down(1, 0.0, 0.0, 1, 1.0);
        let _ = reducer.reduce(1.0, &frame(), 1);

        let cancelled = reducer.reduce(1.0, &Event::Cancel, 2);
        assert_eq!(cancelled.len(), 2);
        assert!(
            cancelled
                .iter()
                .all(|e| matches!(e, PointerEvent::Cancel(_)))
        );

        // State was cleared, so a fresh, higher-id touch is the primary pointer.
        reducer.down(9, 0.0, 0.0, 3, 1.0);
        let next = reducer.reduce(1.0, &frame(), 3);
        assert!(next[0].is_primary_pointer());
    }

    #[test]
    fn frame_without_changes_emits_nothing() {
        let mut reducer = TouchEventReducer::default();
        assert!(reducer.reduce(1.0, &frame(), 1).is_empty());
    }

    #[test]
    #[should_panic(expected = "timestamps must be monotonic nanoseconds")]
    fn non_monotonic_time_panics_in_debug() {
        let mut reducer = TouchEventReducer::default();
        let _ = reducer.reduce(1.0, &frame(), 2_000);
        let _ = reducer.reduce(1.0, &frame(), 1_000);
    }
}
