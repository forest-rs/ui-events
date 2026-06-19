// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Reduces `zwp_tablet_tool_v2` event streams into pen [`PointerEvent`]s.
//!
//! [`TabletToolReducer`] mirrors the stateful-reducer pattern of the other
//! adapters: feed it each `zwp_tablet_tool_v2` event for one tablet tool together
//! with a caller-provided monotonic timestamp, and it tracks the tool's
//! [`PointerState`] and emits [`PointerEvent`]s. Keep one reducer per tool; the
//! tool is reported as the primary pointer of its [`PointerType`].
//!
//! ## Frame batching
//!
//! Like `wl_touch`, the tablet protocol groups the events that belong to one
//! logical update — motion together with its pressure, tilt, and slider axes, or
//! a proximity change together with the motion that positions it — between
//! `frame` events, and the order within a frame is unspecified. The reducer
//! therefore accumulates the tool's state as the events arrive and emits the
//! resulting [`PointerEvent`]s only on `frame`, so position and axes are fully
//! assembled first. Within a frame a tip press or release supersedes a bare
//! move, since it already carries the freshest state.
//!
//! ## Tools, contact, and axes
//!
//! The `type` event sets the [`PointerType`]: the pen-like tools report as
//! [`PointerType::Pen`], the puck-style mouse and lens tools as
//! [`PointerType::Mouse`], and the finger tool as [`PointerType::Touch`] (see
//! [`mapping::pointer_type_from_tool`]). The tool tip's `down`/`up` become
//! [`PointerEvent::Down`]/[`PointerEvent::Up`] carrying [`PointerButton::Primary`],
//! or [`PointerButton::PenEraser`] for an eraser tool. Stylus barrel buttons are
//! mapped through [`mapping::pen_button_from_evdev`]. `pressure` populates
//! [`PointerState::pressure`] (falling back to the active-unknown `0.5` while the
//! tip is down with no pressure reported for the contact), `slider` populates
//! [`PointerState::tangential_pressure`], and `tilt` populates
//! [`PointerState::orientation`]. `proximity_in`/`proximity_out` become
//! [`PointerEvent::Enter`]/[`PointerEvent::Leave`], and the serial of the most
//! recent `proximity_in` is exposed through
//! [`TabletToolReducer::proximity_serial`] for `set_cursor`.
//!
//! The tool's `distance` (hover height), `rotation` (barrel twist), and `wheel`
//! axes have no [`ui-events`] representation and are ignored, as is the
//! descriptive lifecycle (`capability`, `hardware_serial`, `hardware_id_wacom`,
//! `done`, `removed`), which the consumer handles when managing tools. The twist
//! omission matches the web backend, which likewise does not map Pointer Events
//! `twist`.
//!
//! Reducers built on these helpers take a caller-provided monotonic nanosecond
//! timestamp, not Wayland's 32-bit millisecond event timestamps, so input shares
//! one clock domain with frame sampling and timers.
//!
//! [`PointerState`]: ui_events::pointer::PointerState
//! [`PointerState::pressure`]: ui_events::pointer::PointerState::pressure
//! [`PointerState::tangential_pressure`]: ui_events::pointer::PointerState::tangential_pressure
//! [`PointerState::orientation`]: ui_events::pointer::PointerState::orientation
//! [`PointerType`]: ui_events::pointer::PointerType
//! [`PointerType::Pen`]: ui_events::pointer::PointerType::Pen
//! [`PointerType::Mouse`]: ui_events::pointer::PointerType::Mouse
//! [`PointerType::Touch`]: ui_events::pointer::PointerType::Touch
//! [`PointerButton::Primary`]: ui_events::pointer::PointerButton::Primary
//! [`PointerButton::PenEraser`]: ui_events::pointer::PointerButton::PenEraser
//! [`ui-events`]: https://docs.rs/ui-events/

use alloc::vec::Vec;

use ui_events::pointer::{
    PointerButton, PointerButtonEvent, PointerEvent, PointerInfo, PointerState, PointerType,
    PointerUpdate,
};
use wayland_client::WEnum;
use wayland_protocols::wp::tablet::zv2::client::zwp_tablet_tool_v2::{ButtonState, Event, Type};

use crate::mapping::{self, ToolType};
use crate::pointer::TapCounter;

/// Pressure reported while a tool is in contact but reports no pressure axis.
///
/// Tools without a pressure sensor follow the `ui-events` convention of `0.5`
/// while the tip is down.
const ACTIVE_PRESSURE: f32 = 0.5;
/// Pressure reported once the tool tip leaves the surface.
const RELEASED_PRESSURE: f32 = 0.0;

/// The per-frame changes to flush on the next `frame`.
#[derive(Debug, Default)]
struct PendingFrame {
    /// The tool entered proximity (`proximity_in`).
    enter: bool,
    /// The tool left proximity (`proximity_out`).
    leave: bool,
    /// The tool's position or an axis changed (`motion`, `pressure`, `tilt`,
    /// `slider`).
    motion: bool,
    /// The tool tip touched the surface (`down`).
    tip_down: bool,
    /// The tool tip left the surface (`up`).
    tip_up: bool,
    /// Barrel-button changes, each `(button, pressed)`, in arrival order. The
    /// button is `None` for a code with no [`PointerButton`] mapping.
    buttons: Vec<(Option<PointerButton>, bool)>,
}

/// Reduces a `zwp_tablet_tool_v2` event stream into [`PointerEvent`]s.
///
/// Keep one reducer per tablet tool and call [`reduce`] with each event. See the
/// [module documentation] for the batching, tool, axis, and timing model.
///
/// [`reduce`]: TabletToolReducer::reduce
/// [module documentation]: self
#[derive(Debug)]
pub struct TabletToolReducer {
    /// Accumulated state of the tool.
    state: PointerState,
    /// Pointer type derived from the `type` event; [`PointerType::Pen`] until one
    /// arrives.
    pointer_type: PointerType,
    /// The [`PointerButton`] the tool tip maps to, derived from the `type` event.
    tip_button: PointerButton,
    /// Whether the tool is currently between `proximity_in` and `proximity_out`.
    in_proximity: bool,
    /// Serial of the most recent `proximity_in`, for `set_cursor`.
    proximity_serial: Option<u32>,
    /// Whether a `pressure` value has been reported for the current contact. A
    /// `pressure` event sets it; lifting the tip or leaving proximity clears it,
    /// so a tip-down with no reported pressure falls back to the active-unknown
    /// `ACTIVE_PRESSURE` rather than emitting a stale release-level value.
    pressure_known: bool,
    /// Per-frame changes accumulated since the last `frame`.
    pending: PendingFrame,
    /// Click counter, shared with the pointer reducer.
    counter: TapCounter,
    /// Last caller-provided timestamp, for the monotonicity debug assertion.
    last_seen_time: Option<u64>,
}

impl Default for TabletToolReducer {
    fn default() -> Self {
        Self {
            state: PointerState::default(),
            pointer_type: PointerType::Pen,
            tip_button: PointerButton::Primary,
            in_proximity: false,
            proximity_serial: None,
            pressure_known: false,
            pending: PendingFrame::default(),
            counter: TapCounter::default(),
            last_seen_time: None,
        }
    }
}

impl TabletToolReducer {
    /// The serial of the most recent `proximity_in`, while the tool is in
    /// proximity.
    ///
    /// `zwp_tablet_tool_v2::set_cursor` requires the serial of the latest
    /// `proximity_in`; consumers read it from here. It is cleared on
    /// `proximity_out`.
    pub fn proximity_serial(&self) -> Option<u32> {
        self.proximity_serial
    }

    /// Reduce a single `zwp_tablet_tool_v2` [`Event`] into zero or more
    /// [`PointerEvent`]s.
    ///
    /// `time` is a monotonic nanosecond timestamp in the consumer's clock
    /// domain, stamped into the [`PointerState`] of every event this call
    /// produces; Wayland's own 32-bit millisecond event timestamps are
    /// deliberately not used as the clock. `scale_factor` converts surface-local
    /// logical coordinates to physical pixels.
    ///
    /// Proximity, motion, axis, tip, and button events update the tool's state
    /// but emit nothing; the accumulated [`PointerEvent`]s are returned from the
    /// `frame` event that closes the batch.
    ///
    /// `time` must be monotonic across calls; a regression trips a
    /// `debug_assert!`, since click detection measures durations in nanoseconds.
    pub fn reduce(&mut self, scale_factor: f64, event: &Event, time: u64) -> Vec<PointerEvent> {
        self.check_time_monotonic(time);
        self.state.time = time;
        self.state.scale_factor = scale_factor;

        match event {
            Event::Type { tool_type } => self.set_tool(*tool_type),
            // `proximity_in` carries `tablet` and `surface` objects that cannot
            // be constructed without a live connection, so the
            // surface-independent logic lives in `proximity_in`.
            Event::ProximityIn { serial, .. } => self.proximity_in(*serial),
            Event::ProximityOut => self.pending.leave = true,
            Event::Down { .. } => self.pending.tip_down = true,
            Event::Up => self.pending.tip_up = true,
            Event::Motion { x, y } => self.motion(*x, *y),
            Event::Pressure { pressure } => self.set_pressure(*pressure),
            Event::Tilt { tilt_x, tilt_y } => self.set_tilt(*tilt_x, *tilt_y),
            Event::Slider { position } => self.set_slider(*position),
            Event::Button { button, state, .. } => self.button(*button, *state),
            Event::Frame { .. } => return self.flush(scale_factor),
            // No `ui-events` representation: hover distance, barrel rotation
            // (twist), and the tool wheel. See the module documentation.
            Event::Distance { .. } | Event::Rotation { .. } | Event::Wheel { .. } => {}
            // `Event` is `#[non_exhaustive]`, and the descriptive lifecycle events
            // (`capability`, `hardware_serial`, `hardware_id_wacom`, `done`,
            // `removed`) carry no input.
            _ => {}
        }
        Vec::new()
    }

    /// Handle a `type` event: derive the pointer type and tip button.
    fn set_tool(&mut self, value: WEnum<Type>) {
        if let Some(tool) = tool_type_from_protocol(value) {
            self.pointer_type = mapping::pointer_type_from_tool(tool);
            self.tip_button = mapping::tip_button_from_tool(tool);
        }
    }

    /// Handle a `proximity_in` event: record the serial and mark an enter.
    ///
    /// This takes the serial rather than the `proximity_in` event, whose `tablet`
    /// and `surface` objects cannot be constructed without a live connection, so
    /// the surface-independent logic stays unit-testable.
    fn proximity_in(&mut self, serial: u32) {
        self.in_proximity = true;
        self.proximity_serial = Some(serial);
        self.pending.enter = true;
    }

    /// Handle a `motion` event: update the position from surface-local logical
    /// coordinates.
    fn motion(&mut self, x: f64, y: f64) {
        self.state.position =
            mapping::physical_position_from_logical(x, y, self.state.scale_factor);
        self.pending.motion = true;
    }

    /// Handle a `pressure` event: update the normalized pressure.
    fn set_pressure(&mut self, pressure: u32) {
        self.pressure_known = true;
        self.state.pressure = mapping::pressure_from_normalized(pressure);
        self.pending.motion = true;
    }

    /// Handle a `tilt` event: update the orientation.
    fn set_tilt(&mut self, tilt_x: f64, tilt_y: f64) {
        self.state.orientation = mapping::pointer_orientation_from_tilt_degrees(tilt_x, tilt_y);
        self.pending.motion = true;
    }

    /// Handle a `slider` event: update the tangential pressure.
    fn set_slider(&mut self, position: i32) {
        self.state.tangential_pressure = mapping::tangential_pressure_from_slider(position);
        self.pending.motion = true;
    }

    /// Handle a `button` event: record the barrel-button change for the frame.
    fn button(&mut self, code: u32, state: WEnum<ButtonState>) {
        let button = mapping::pen_button_from_evdev(code);
        match state {
            WEnum::Value(ButtonState::Pressed) => self.pending.buttons.push((button, true)),
            WEnum::Value(ButtonState::Released) => self.pending.buttons.push((button, false)),
            // Unknown button state.
            _ => {}
        }
    }

    /// Emit the changes accumulated since the last `frame`.
    fn flush(&mut self, scale_factor: f64) -> Vec<PointerEvent> {
        let pending = core::mem::take(&mut self.pending);
        let mut events = Vec::new();

        if pending.enter {
            let event = PointerEvent::Enter(self.pointer_info());
            events.push(self.counter.attach_count(scale_factor, event));
        }

        // A tip press or release carries the freshest position and axes, so it
        // supersedes a bare move within the same frame.
        if pending.tip_down {
            self.state.buttons.insert(self.tip_button);
            if !self.pressure_known {
                self.state.pressure = ACTIVE_PRESSURE;
            }
            let event = self.button_event(Some(self.tip_button), true);
            events.push(self.counter.attach_count(scale_factor, event));
        } else if pending.tip_up {
            self.state.buttons.remove(self.tip_button);
            self.state.pressure = RELEASED_PRESSURE;
            // The contact ended; the next tip-down needs its own pressure value.
            self.pressure_known = false;
            let event = self.button_event(Some(self.tip_button), false);
            events.push(self.counter.attach_count(scale_factor, event));
        } else if pending.motion {
            let event = self.move_event();
            events.push(self.counter.attach_count(scale_factor, event));
        }

        for &(button, pressed) in &pending.buttons {
            if let Some(button) = button {
                if pressed {
                    self.state.buttons.insert(button);
                } else {
                    self.state.buttons.remove(button);
                }
            }
            let event = self.button_event(button, pressed);
            events.push(self.counter.attach_count(scale_factor, event));
        }

        if pending.leave {
            let event = PointerEvent::Leave(self.pointer_info());
            events.push(self.counter.attach_count(scale_factor, event));
            self.reset_on_leave();
        }

        events
    }

    /// The [`PointerInfo`] for this tool, reported as the primary pointer of its
    /// type.
    fn pointer_info(&self) -> PointerInfo {
        mapping::primary_pointer_info(self.pointer_type)
    }

    /// Build a [`PointerEvent::Down`] or [`PointerEvent::Up`] for `button` with
    /// the current state.
    fn button_event(&self, button: Option<PointerButton>, pressed: bool) -> PointerEvent {
        let event = PointerButtonEvent {
            button,
            pointer: self.pointer_info(),
            state: self.state.clone(),
        };
        if pressed {
            PointerEvent::Down(event)
        } else {
            PointerEvent::Up(event)
        }
    }

    /// Build a [`PointerEvent::Move`] with the current state.
    fn move_event(&self) -> PointerEvent {
        PointerEvent::Move(PointerUpdate {
            pointer: self.pointer_info(),
            current: self.state.clone(),
            coalesced: Vec::new(),
            predicted: Vec::new(),
        })
    }

    /// Reset the per-proximity state after a `proximity_out`, keeping the tool's
    /// derived type for the next time it enters.
    fn reset_on_leave(&mut self) {
        self.in_proximity = false;
        self.proximity_serial = None;
        self.pressure_known = false;
        let time = self.state.time;
        let scale_factor = self.state.scale_factor;
        self.state = PointerState {
            time,
            scale_factor,
            ..PointerState::default()
        };
    }

    /// Debug-assert that caller timestamps do not regress.
    fn check_time_monotonic(&mut self, time: u64) {
        if let Some(previous) = self.last_seen_time {
            debug_assert!(
                time >= previous,
                "TabletToolReducer::reduce timestamps must be monotonic nanoseconds"
            );
        }
        self.last_seen_time = Some(time);
    }
}

/// Translate a `zwp_tablet_tool_v2::type` value into the neutral [`ToolType`].
fn tool_type_from_protocol(value: WEnum<Type>) -> Option<ToolType> {
    match value {
        WEnum::Value(Type::Pen) => Some(ToolType::Pen),
        WEnum::Value(Type::Eraser) => Some(ToolType::Eraser),
        WEnum::Value(Type::Brush) => Some(ToolType::Brush),
        WEnum::Value(Type::Pencil) => Some(ToolType::Pencil),
        WEnum::Value(Type::Airbrush) => Some(ToolType::Airbrush),
        WEnum::Value(Type::Finger) => Some(ToolType::Finger),
        WEnum::Value(Type::Mouse) => Some(ToolType::Mouse),
        WEnum::Value(Type::Lens) => Some(ToolType::Lens),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use dpi::PhysicalPosition;

    use super::*;

    /// `BTN_STYLUS` from `linux/input-event-codes.h`.
    const BTN_STYLUS: u32 = 0x14b;

    fn frame() -> Event {
        Event::Frame { time: 0 }
    }

    fn motion(x: f64, y: f64) -> Event {
        Event::Motion { x, y }
    }

    fn down() -> Event {
        Event::Down { serial: 0 }
    }

    fn pressure(value: u32) -> Event {
        Event::Pressure { pressure: value }
    }

    fn tilt(tilt_x: f64, tilt_y: f64) -> Event {
        Event::Tilt { tilt_x, tilt_y }
    }

    fn slider(position: i32) -> Event {
        Event::Slider { position }
    }

    fn tool(tool_type: Type) -> Event {
        Event::Type {
            tool_type: WEnum::Value(tool_type),
        }
    }

    fn button(code: u32, state: ButtonState) -> Event {
        Event::Button {
            serial: 0,
            button: code,
            state: WEnum::Value(state),
        }
    }

    #[test]
    fn proximity_in_then_motion_emits_enter_then_move() {
        let mut reducer = TabletToolReducer::default();
        // `proximity_in` carries un-constructable objects, so call it directly.
        reducer.proximity_in(7);
        let _ = reducer.reduce(2.0, &motion(10.0, 20.0), 5);
        let events = reducer.reduce(2.0, &frame(), 5);

        assert_eq!(events.len(), 2);
        let PointerEvent::Enter(enter) = &events[0] else {
            panic!("expected an enter, got {:?}", events[0]);
        };
        assert!(enter.is_primary_pointer());
        assert_eq!(enter.pointer_type, PointerType::Pen);

        let PointerEvent::Move(update) = &events[1] else {
            panic!("expected a move, got {:?}", events[1]);
        };
        assert_eq!(
            update.current.position,
            PhysicalPosition { x: 20.0, y: 40.0 }
        );
        assert_eq!(update.current.time, 5);
        assert_eq!(reducer.proximity_serial(), Some(7));
    }

    #[test]
    fn tip_down_then_up_tracks_primary_button_and_pressure() {
        let mut reducer = TabletToolReducer::default();

        let _ = reducer.reduce(1.0, &down(), 1_000);
        let down_events = reducer.reduce(1.0, &frame(), 1_000);
        assert_eq!(down_events.len(), 1);
        let PointerEvent::Down(d) = &down_events[0] else {
            panic!("expected a down, got {:?}", down_events[0]);
        };
        assert_eq!(d.button, Some(PointerButton::Primary));
        assert_eq!(d.pointer.pointer_type, PointerType::Pen);
        assert!(d.state.buttons.contains(PointerButton::Primary));
        // No pressure axis was reported, so the active-unknown fallback applies.
        assert_eq!(d.state.pressure, ACTIVE_PRESSURE);
        assert_eq!(d.state.count, 1);

        let _ = reducer.reduce(1.0, &Event::Up, 2_000);
        let up_events = reducer.reduce(1.0, &frame(), 2_000);
        let PointerEvent::Up(u) = &up_events[0] else {
            panic!("expected an up, got {:?}", up_events[0]);
        };
        assert_eq!(u.button, Some(PointerButton::Primary));
        assert!(!u.state.buttons.contains(PointerButton::Primary));
        assert_eq!(u.state.pressure, RELEASED_PRESSURE);
    }

    #[test]
    fn reported_pressure_overrides_active_default() {
        let mut reducer = TabletToolReducer::default();
        let _ = reducer.reduce(1.0, &down(), 1);
        let _ = reducer.reduce(1.0, &pressure(65535), 1);
        let events = reducer.reduce(1.0, &frame(), 1);

        // The tip-down and the reported pressure coalesce into a single down.
        assert_eq!(events.len(), 1);
        let PointerEvent::Down(d) = &events[0] else {
            panic!("expected a down, got {:?}", events[0]);
        };
        assert!((d.state.pressure - 1.0).abs() < 1e-4);
    }

    #[test]
    fn tip_down_after_a_pressure_cycle_falls_back_to_active_pressure() {
        let mut reducer = TabletToolReducer::default();

        // A first contact reports real pressure.
        reducer.proximity_in(1);
        let _ = reducer.reduce(1.0, &down(), 1);
        let _ = reducer.reduce(1.0, &pressure(65535), 1);
        let _ = reducer.reduce(1.0, &frame(), 1);

        // Release the tip and leave proximity; both reset pressure to released.
        let _ = reducer.reduce(1.0, &Event::Up, 2);
        let _ = reducer.reduce(1.0, &frame(), 2);
        let _ = reducer.reduce(1.0, &Event::ProximityOut, 3);
        let _ = reducer.reduce(1.0, &frame(), 3);

        // A new contact whose down frame carries no pressure must still report
        // the active-unknown fallback, never the stale 0.0 from the last contact.
        reducer.proximity_in(4);
        let _ = reducer.reduce(1.0, &down(), 4);
        let events = reducer.reduce(1.0, &frame(), 4);
        let down = events
            .iter()
            .find_map(|e| match e {
                PointerEvent::Down(d) => Some(d),
                _ => None,
            })
            .expect("expected a tip down");
        assert_eq!(down.state.pressure, ACTIVE_PRESSURE);
    }

    #[test]
    fn second_tip_down_in_one_proximity_without_pressure_falls_back_to_active() {
        let mut reducer = TabletToolReducer::default();
        reducer.proximity_in(1);

        // First tap reports real pressure, then the tip lifts.
        let _ = reducer.reduce(1.0, &down(), 1);
        let _ = reducer.reduce(1.0, &pressure(65535), 1);
        let _ = reducer.reduce(1.0, &frame(), 1);
        let _ = reducer.reduce(1.0, &Event::Up, 2);
        let _ = reducer.reduce(1.0, &frame(), 2);

        // A second tap in the same proximity, with no fresh pressure this frame,
        // must not carry the released pressure from the first tap.
        let _ = reducer.reduce(1.0, &down(), 3);
        let events = reducer.reduce(1.0, &frame(), 3);
        let down = events
            .iter()
            .find_map(|e| match e {
                PointerEvent::Down(d) => Some(d),
                _ => None,
            })
            .expect("expected a tip down");
        assert_eq!(down.state.pressure, ACTIVE_PRESSURE);
    }

    #[test]
    fn tilt_and_slider_populate_orientation_and_tangential_pressure() {
        let mut reducer = TabletToolReducer::default();
        let _ = reducer.reduce(1.0, &tilt(30.0, 0.0), 1);
        let _ = reducer.reduce(1.0, &slider(32768), 1);
        let events = reducer.reduce(1.0, &frame(), 1);

        assert_eq!(events.len(), 1);
        let PointerEvent::Move(update) = &events[0] else {
            panic!("expected a move, got {:?}", events[0]);
        };
        // Tilt toward +x aims the azimuth along the x-axis.
        assert!(update.current.orientation.azimuth.abs() < 1e-5);
        assert!((update.current.tangential_pressure - 0.5).abs() < 1e-4);
    }

    #[test]
    fn eraser_tool_tip_maps_to_pen_eraser() {
        let mut reducer = TabletToolReducer::default();
        let _ = reducer.reduce(1.0, &tool(Type::Eraser), 1);
        let _ = reducer.reduce(1.0, &down(), 1);
        let events = reducer.reduce(1.0, &frame(), 1);

        let PointerEvent::Down(d) = &events[0] else {
            panic!("expected a down, got {:?}", events[0]);
        };
        assert_eq!(d.button, Some(PointerButton::PenEraser));
        assert_eq!(d.pointer.pointer_type, PointerType::Pen);
        assert!(d.state.buttons.contains(PointerButton::PenEraser));
    }

    #[test]
    fn barrel_button_press_maps_to_secondary() {
        let mut reducer = TabletToolReducer::default();
        let _ = reducer.reduce(1.0, &button(BTN_STYLUS, ButtonState::Pressed), 1);
        let events = reducer.reduce(1.0, &frame(), 1);

        assert_eq!(events.len(), 1);
        let PointerEvent::Down(d) = &events[0] else {
            panic!("expected a down, got {:?}", events[0]);
        };
        assert_eq!(d.button, Some(PointerButton::Secondary));
        assert!(d.state.buttons.contains(PointerButton::Secondary));
    }

    #[test]
    fn proximity_out_emits_leave_and_clears_serial() {
        let mut reducer = TabletToolReducer::default();
        reducer.proximity_in(3);
        let _ = reducer.reduce(1.0, &motion(1.0, 1.0), 1);
        let _ = reducer.reduce(1.0, &frame(), 1);

        let _ = reducer.reduce(1.0, &Event::ProximityOut, 2);
        let events = reducer.reduce(1.0, &frame(), 2);
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], PointerEvent::Leave(_)));
        assert_eq!(reducer.proximity_serial(), None);
    }

    #[test]
    fn frame_without_changes_emits_nothing() {
        let mut reducer = TabletToolReducer::default();
        assert!(reducer.reduce(1.0, &frame(), 1).is_empty());
    }

    #[test]
    #[should_panic(expected = "timestamps must be monotonic nanoseconds")]
    fn non_monotonic_time_panics_in_debug() {
        let mut reducer = TabletToolReducer::default();
        let _ = reducer.reduce(1.0, &frame(), 2_000);
        let _ = reducer.reduce(1.0, &frame(), 1_000);
    }
}
