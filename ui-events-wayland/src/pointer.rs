// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Reduces `wl_pointer` event streams into [`PointerEvent`]s.
//!
//! [`PointerEventReducer`] mirrors the stateful-reducer pattern of the other
//! adapters: feed it each `wl_pointer` event for a seat's pointer together with
//! a caller-provided monotonic timestamp, and it tracks the primary pointer's
//! [`PointerState`] and emits [`PointerEvent`]s.
//!
//! Wayland groups the events that logically belong together — for example the
//! horizontal and vertical axes of one diagonal scroll — between `frame`
//! events. The reducer accumulates scroll axes across a frame and emits a
//! single [`PointerEvent::Scroll`] when the `frame` arrives. Every other event
//! is translated as it arrives, so most calls return zero or one event; the
//! returned [`Vec`] exists so a `frame` can flush an accumulated scroll.
//!
//! Surface-local coordinates are converted to physical pixels through
//! [`mapping::physical_position_from_logical`], and the serial of the most
//! recent `enter` event is exposed through [`PointerEventReducer::enter_serial`]
//! for issuing `wl_pointer::set_cursor`.
//!
//! [`PointerState`]: ui_events::pointer::PointerState

use ui_events::pointer::{
    PointerButtonEvent, PointerEvent, PointerId, PointerInfo, PointerScrollEvent, PointerState,
    PointerType, PointerUpdate,
};
use wayland_client::WEnum;
use wayland_client::protocol::wl_pointer::{self, Axis, ButtonState, Event};

use crate::mapping::{self, AxisFrame};

/// The [`PointerInfo`] describing a seat's primary mouse pointer.
fn primary_mouse() -> PointerInfo {
    mapping::primary_pointer_info(PointerType::Mouse)
}

/// Reduces a `wl_pointer` event stream into [`PointerEvent`]s.
///
/// Keep one reducer per seat pointer and call [`reduce`] with each `wl_pointer`
/// event. See the [module documentation] for the batching and timing model.
///
/// [`reduce`]: PointerEventReducer::reduce
/// [module documentation]: self
#[derive(Debug)]
pub struct PointerEventReducer {
    /// State of the primary pointer.
    primary_state: PointerState,
    /// Click counter.
    counter: TapCounter,
    /// Last caller-provided timestamp, for the monotonicity debug assertion.
    last_seen_time: Option<u64>,
    /// Serial of the most recent `enter` event, cleared on `leave`.
    enter_serial: Option<u32>,
    /// Scroll accumulated since the last `frame` event.
    axis_frame: AxisFrame,
    /// Whether axis events are grouped by `frame` (`wl_pointer` version 5+).
    frame_batched: bool,
}

impl Default for PointerEventReducer {
    fn default() -> Self {
        Self {
            primary_state: PointerState::default(),
            counter: TapCounter::default(),
            last_seen_time: None,
            enter_serial: None,
            axis_frame: AxisFrame::default(),
            frame_batched: true,
        }
    }
}

impl PointerEventReducer {
    /// Create a reducer for a `wl_pointer` bound at the given protocol `version`.
    ///
    /// Version 5 introduced the `frame` event that groups the axis events of one
    /// logical scroll. For version 5 and later (the [`Default`]), the reducer
    /// accumulates scroll axes and emits a single [`PointerEvent::Scroll`] on
    /// `frame`. Earlier versions have no `frame` event, so each axis event is
    /// emitted as its own scroll.
    pub fn for_version(version: u32) -> Self {
        Self {
            frame_batched: version >= 5,
            ..Self::default()
        }
    }

    /// The serial of the most recent `enter` event, while the pointer is over a
    /// surface.
    ///
    /// `wl_pointer::set_cursor` requires the serial of the latest `enter` event;
    /// consumers read it from here. It is cleared on `leave`.
    pub fn enter_serial(&self) -> Option<u32> {
        self.enter_serial
    }

    /// Reduce a single `wl_pointer` [`Event`] into zero or more [`PointerEvent`]s.
    ///
    /// `time` is a monotonic nanosecond timestamp in the consumer's clock
    /// domain. It is stamped into every [`PointerState`] this call produces,
    /// including the `frame`-driven scroll, so input shares one timeline with
    /// frame sampling, timers, and diagnostics; Wayland's own 32-bit millisecond
    /// event timestamps are deliberately not used as the clock. `scale_factor`
    /// converts surface-local logical coordinates to physical pixels.
    ///
    /// `time` must be monotonic across calls; a regression trips a
    /// `debug_assert!`, since click detection measures durations in nanoseconds.
    pub fn reduce(&mut self, scale_factor: f64, event: &Event, time: u64) -> Vec<PointerEvent> {
        self.check_time_monotonic(time);
        self.primary_state.time = time;
        self.primary_state.scale_factor = scale_factor;

        match event {
            Event::Enter {
                serial,
                surface_x,
                surface_y,
                ..
            } => vec![self.enter(*serial, *surface_x, *surface_y)],
            Event::Leave { .. } => vec![self.leave()],
            Event::Motion {
                surface_x,
                surface_y,
                ..
            } => {
                self.set_position(*surface_x, *surface_y);
                vec![self.counter.attach_count(
                    scale_factor,
                    PointerEvent::Move(PointerUpdate {
                        pointer: primary_mouse(),
                        current: self.primary_state.clone(),
                        coalesced: Vec::new(),
                        predicted: Vec::new(),
                    }),
                )]
            }
            Event::Button { button, state, .. } => self.button(*button, *state, scale_factor),
            Event::Axis { axis, value, .. } => {
                self.accumulate_axis_value(*axis, *value);
                // Versions before 5 have no `frame`, so each axis stands alone.
                if self.frame_batched {
                    Vec::new()
                } else {
                    self.flush_scroll(scale_factor)
                }
            }
            Event::AxisSource { axis_source } => {
                if let Some(source) = map_axis_source(*axis_source) {
                    self.axis_frame.source = Some(source);
                }
                Vec::new()
            }
            Event::AxisValue120 { axis, value120 } => {
                self.accumulate_axis_value120(*axis, *value120);
                Vec::new()
            }
            Event::AxisDiscrete { axis, discrete } => {
                self.accumulate_axis_discrete(*axis, *discrete);
                Vec::new()
            }
            Event::Frame => self.flush_scroll(scale_factor),
            // `axis_stop` (kinetic-scroll end) and `axis_relative_direction`
            // (a natural-scroll hint) have no ui-events representation.
            Event::AxisStop { .. } | Event::AxisRelativeDirection { .. } => Vec::new(),
            // `wl_pointer::Event` is `#[non_exhaustive]`; ignore future additions.
            _ => Vec::new(),
        }
    }

    /// Update the tracked position from surface-local logical coordinates.
    fn set_position(&mut self, surface_x: f64, surface_y: f64) {
        self.primary_state.position = mapping::physical_position_from_logical(
            surface_x,
            surface_y,
            self.primary_state.scale_factor,
        );
    }

    /// Handle an `enter` event: record the serial and update the position.
    ///
    /// This takes the surface-local coordinates rather than the `wl_surface`, so
    /// the surface-independent logic stays unit-testable without a live
    /// connection (a `wl_surface` cannot be constructed without one).
    fn enter(&mut self, serial: u32, surface_x: f64, surface_y: f64) -> PointerEvent {
        self.enter_serial = Some(serial);
        self.set_position(surface_x, surface_y);
        PointerEvent::Enter(primary_mouse())
    }

    /// Handle a `leave` event: clear the enter serial.
    fn leave(&mut self) -> PointerEvent {
        self.enter_serial = None;
        PointerEvent::Leave(primary_mouse())
    }

    /// Handle a `button` event: update the button set and emit a down or up.
    fn button(
        &mut self,
        code: u32,
        state: WEnum<ButtonState>,
        scale_factor: f64,
    ) -> Vec<PointerEvent> {
        let button = mapping::pointer_button_from_evdev(code);
        match state {
            WEnum::Value(ButtonState::Pressed) => {
                if let Some(button) = button {
                    self.primary_state.buttons.insert(button);
                }
                vec![self.counter.attach_count(
                    scale_factor,
                    PointerEvent::Down(PointerButtonEvent {
                        button,
                        pointer: primary_mouse(),
                        state: self.primary_state.clone(),
                    }),
                )]
            }
            WEnum::Value(ButtonState::Released) => {
                if let Some(button) = button {
                    self.primary_state.buttons.remove(button);
                }
                vec![self.counter.attach_count(
                    scale_factor,
                    PointerEvent::Up(PointerButtonEvent {
                        button,
                        pointer: primary_mouse(),
                        state: self.primary_state.clone(),
                    }),
                )]
            }
            // Unknown button state.
            _ => Vec::new(),
        }
    }

    /// Add a continuous axis value (logical pixels) to the current frame.
    fn accumulate_axis_value(&mut self, axis: WEnum<Axis>, value: f64) {
        match axis {
            WEnum::Value(Axis::VerticalScroll) => self.axis_frame.value.1 += value,
            WEnum::Value(Axis::HorizontalScroll) => self.axis_frame.value.0 += value,
            _ => {}
        }
    }

    /// Add a high-resolution (`value120`) axis step to the current frame.
    fn accumulate_axis_value120(&mut self, axis: WEnum<Axis>, value120: i32) {
        match axis {
            WEnum::Value(Axis::VerticalScroll) => self.axis_frame.value120.1 += value120,
            WEnum::Value(Axis::HorizontalScroll) => self.axis_frame.value120.0 += value120,
            _ => {}
        }
    }

    /// Add a deprecated discrete-detent axis step to the current frame.
    fn accumulate_axis_discrete(&mut self, axis: WEnum<Axis>, discrete: i32) {
        match axis {
            WEnum::Value(Axis::VerticalScroll) => self.axis_frame.discrete.1 += discrete,
            WEnum::Value(Axis::HorizontalScroll) => self.axis_frame.discrete.0 += discrete,
            _ => {}
        }
    }

    /// Emit the scroll accumulated since the last flush, if any, and reset the
    /// frame.
    fn flush_scroll(&mut self, scale_factor: f64) -> Vec<PointerEvent> {
        let frame = core::mem::take(&mut self.axis_frame);
        match mapping::scroll_delta_from_axis_frame(frame, scale_factor) {
            Some(delta) => vec![PointerEvent::Scroll(PointerScrollEvent {
                pointer: primary_mouse(),
                delta,
                state: self.primary_state.clone(),
            })],
            None => Vec::new(),
        }
    }

    /// Debug-assert that caller timestamps do not regress.
    fn check_time_monotonic(&mut self, time: u64) {
        if let Some(previous) = self.last_seen_time {
            debug_assert!(
                time >= previous,
                "PointerEventReducer::reduce timestamps must be monotonic nanoseconds"
            );
        }
        self.last_seen_time = Some(time);
    }
}

/// Translate a `wl_pointer` axis source into the neutral [`mapping::AxisSource`].
fn map_axis_source(source: WEnum<wl_pointer::AxisSource>) -> Option<mapping::AxisSource> {
    match source {
        WEnum::Value(wl_pointer::AxisSource::Wheel) => Some(mapping::AxisSource::Wheel),
        WEnum::Value(wl_pointer::AxisSource::Finger) => Some(mapping::AxisSource::Finger),
        WEnum::Value(wl_pointer::AxisSource::Continuous) => Some(mapping::AxisSource::Continuous),
        WEnum::Value(wl_pointer::AxisSource::WheelTilt) => Some(mapping::AxisSource::WheelTilt),
        _ => None,
    }
}

#[derive(Clone, Debug)]
struct TapState {
    /// Pointer ID this tap is tracked against.
    pointer_id: Option<PointerId>,
    /// Nanosecond timestamp when the tap went down.
    down_time: u64,
    /// Nanosecond timestamp when the tap went up.
    ///
    /// Resets to `down_time` when the tap goes down.
    up_time: u64,
    /// The local tap count as of the last down phase.
    count: u8,
    /// x coordinate of the anchor point.
    x: f64,
    /// y coordinate of the anchor point.
    y: f64,
}

/// Tracks multi-click and multi-tap `count` by watching the [`PointerEvent`]s a
/// reducer emits. Shared by the pointer and touch reducers.
#[derive(Debug, Default)]
pub(crate) struct TapCounter {
    taps: Vec<TapState>,
}

impl TapCounter {
    /// Enhance a [`PointerEvent`] with a `count`.
    pub(crate) fn attach_count(&mut self, scale_factor: f64, e: PointerEvent) -> PointerEvent {
        match e {
            PointerEvent::Down(mut event) => {
                let pointer_id = event.pointer.pointer_id;
                let position = event.state.position;
                let time = event.state.time;

                let slop = match event.pointer.pointer_type {
                    // This is on the low side of double tap slop, validated
                    // experimentally to work on a few touchscreen laptops.
                    PointerType::Touch => 12.0,
                    PointerType::Pen => 6.0,
                    // This is slightly more forgiving than the default on Windows for mice.
                    // In order to make the slop calculation more similar between devices,
                    // this uses a slightly different method than Windows, which tests if the
                    // tap is in a box, rather than in a circle, centered on the anchor point.
                    _ => 2.0,
                } * core::f64::consts::SQRT_2
                    * scale_factor;

                if let Some(tap) =
                    self.taps.iter_mut().find(|TapState { x, y, up_time, .. }| {
                        let dx = (x - position.x).abs();
                        let dy = (y - position.y).abs();
                        (dx * dx + dy * dy).sqrt() < slop && (up_time + 500_000_000) > time
                    })
                {
                    let count = tap.count + 1;
                    event.state.count = count;
                    tap.count = count;
                    tap.pointer_id = pointer_id;
                    tap.down_time = time;
                    tap.up_time = time;
                    tap.x = position.x;
                    tap.y = position.y;
                } else {
                    let s = TapState {
                        pointer_id,
                        down_time: time,
                        up_time: time,
                        count: 1,
                        x: position.x,
                        y: position.y,
                    };
                    if let Some(t) = self
                        .taps
                        .iter_mut()
                        .find(|state| state.pointer_id == pointer_id)
                    {
                        *t = s;
                    } else {
                        self.taps.push(s);
                    }
                    event.state.count = 1;
                };
                self.clear_expired(time);
                PointerEvent::Down(event)
            }
            PointerEvent::Up(mut event) => {
                let p_id = event.pointer.pointer_id;
                if let Some(tap) = self.taps.iter_mut().find(|state| state.pointer_id == p_id) {
                    tap.up_time = event.state.time;
                    event.state.count = tap.count;
                }
                PointerEvent::Up(event)
            }
            PointerEvent::Move(PointerUpdate {
                pointer,
                mut current,
                mut coalesced,
                mut predicted,
            }) => {
                if let Some(TapState { count, .. }) = self
                    .taps
                    .iter()
                    .find(
                        |TapState {
                             pointer_id,
                             down_time,
                             up_time,
                             ..
                         }| {
                            *pointer_id == pointer.pointer_id && down_time == up_time
                        },
                    )
                    .cloned()
                {
                    current.count = count;
                    for event in coalesced.iter_mut() {
                        event.count = count;
                    }
                    for event in predicted.iter_mut() {
                        event.count = count;
                    }
                    PointerEvent::Move(PointerUpdate {
                        pointer,
                        current,
                        coalesced,
                        predicted,
                    })
                } else {
                    PointerEvent::Move(PointerUpdate {
                        pointer,
                        current,
                        coalesced,
                        predicted,
                    })
                }
            }
            PointerEvent::Cancel(p) => {
                self.taps
                    .retain(|TapState { pointer_id, .. }| *pointer_id != p.pointer_id);
                PointerEvent::Cancel(p)
            }
            PointerEvent::Leave(p) => {
                self.taps
                    .retain(|TapState { pointer_id, .. }| *pointer_id != p.pointer_id);
                PointerEvent::Leave(p)
            }
            e
            @ (PointerEvent::Enter(..) | PointerEvent::Scroll(..) | PointerEvent::Gesture(..)) => e,
        }
    }

    /// Clear expired taps.
    ///
    /// `t` is the timestamp of the last received event, in the caller's
    /// monotonic clock domain shared by every event the reducer sees.
    fn clear_expired(&mut self, t: u64) {
        self.taps.retain(
            |TapState {
                 down_time, up_time, ..
             }| { down_time == up_time || (up_time + 500_000_000) > t },
        );
    }
}

#[cfg(test)]
mod tests {
    use dpi::PhysicalPosition;
    use ui_events::ScrollDelta;
    use ui_events::pointer::PointerButton;

    use super::*;

    /// `BTN_LEFT` from `linux/input-event-codes.h`.
    const BTN_LEFT: u32 = 0x110;

    fn motion(x: f64, y: f64) -> Event {
        Event::Motion {
            time: 0,
            surface_x: x,
            surface_y: y,
        }
    }

    fn button(code: u32, state: ButtonState) -> Event {
        Event::Button {
            serial: 0,
            time: 0,
            button: code,
            state: WEnum::Value(state),
        }
    }

    fn axis_source(source: wl_pointer::AxisSource) -> Event {
        Event::AxisSource {
            axis_source: WEnum::Value(source),
        }
    }

    #[test]
    fn motion_uses_caller_time_scale_and_scaled_position() {
        let mut reducer = PointerEventReducer::default();
        let events = reducer.reduce(2.0, &motion(10.0, 20.0), 42);

        assert_eq!(events.len(), 1);
        let PointerEvent::Move(update) = &events[0] else {
            panic!("expected a pointer move, got {:?}", events[0]);
        };
        assert!(update.pointer.is_primary_pointer());
        assert_eq!(update.current.time, 42);
        assert_eq!(update.current.scale_factor, 2.0);
        assert_eq!(
            update.current.position,
            PhysicalPosition { x: 20.0, y: 40.0 }
        );
    }

    #[test]
    fn button_press_release_tracks_state_and_click_count() {
        let mut reducer = PointerEventReducer::default();

        let down = reducer.reduce(1.0, &button(BTN_LEFT, ButtonState::Pressed), 1_000);
        let up = reducer.reduce(1.0, &button(BTN_LEFT, ButtonState::Released), 2_000);
        let down_again = reducer.reduce(1.0, &button(BTN_LEFT, ButtonState::Pressed), 3_000);

        let PointerEvent::Down(first) = &down[0] else {
            panic!("expected a pointer down");
        };
        assert_eq!(first.button, Some(PointerButton::Primary));
        assert!(first.state.buttons.contains(PointerButton::Primary));
        assert_eq!(first.state.count, 1);

        let PointerEvent::Up(released) = &up[0] else {
            panic!("expected a pointer up");
        };
        assert_eq!(released.button, Some(PointerButton::Primary));
        assert!(!released.state.buttons.contains(PointerButton::Primary));
        assert_eq!(released.state.count, 1);

        let PointerEvent::Down(second) = &down_again[0] else {
            panic!("expected a second pointer down");
        };
        assert_eq!(second.state.count, 2);
    }

    #[test]
    fn unknown_button_keeps_button_none_but_still_emits() {
        let mut reducer = PointerEventReducer::default();
        // `BTN_TOUCH` (0x14a) has no `PointerButton` mapping.
        let down = reducer.reduce(1.0, &button(0x14a, ButtonState::Pressed), 1);

        let PointerEvent::Down(event) = &down[0] else {
            panic!("expected a pointer down");
        };
        assert_eq!(event.button, None);
        assert!(event.state.buttons.is_empty());
    }

    #[test]
    fn wheel_scroll_accumulates_and_flushes_on_frame() {
        let mut reducer = PointerEventReducer::default();

        assert!(
            reducer
                .reduce(1.0, &axis_source(wl_pointer::AxisSource::Wheel), 1)
                .is_empty()
        );
        assert!(
            reducer
                .reduce(
                    1.0,
                    &Event::AxisValue120 {
                        axis: WEnum::Value(Axis::VerticalScroll),
                        value120: 240,
                    },
                    1,
                )
                .is_empty()
        );

        let flushed = reducer.reduce(1.0, &Event::Frame, 1);
        let PointerEvent::Scroll(scroll) = &flushed[0] else {
            panic!("expected a scroll on frame");
        };
        assert_eq!(scroll.delta, ScrollDelta::LineDelta(0.0, 2.0));
        assert_eq!(scroll.state.time, 1);
    }

    #[test]
    fn finger_scroll_is_scaled_pixel_delta() {
        let mut reducer = PointerEventReducer::default();

        let _ = reducer.reduce(2.0, &axis_source(wl_pointer::AxisSource::Finger), 1);
        let _ = reducer.reduce(
            2.0,
            &Event::Axis {
                time: 0,
                axis: WEnum::Value(Axis::VerticalScroll),
                value: 10.0,
            },
            1,
        );

        let flushed = reducer.reduce(2.0, &Event::Frame, 1);
        let PointerEvent::Scroll(scroll) = &flushed[0] else {
            panic!("expected a scroll on frame");
        };
        assert_eq!(
            scroll.delta,
            ScrollDelta::PixelDelta(PhysicalPosition { x: 0.0, y: 20.0 })
        );
    }

    #[test]
    fn enter_records_serial_and_scaled_position() {
        let mut reducer = PointerEventReducer::default();
        reducer.primary_state.scale_factor = 2.0;

        let event = reducer.enter(7, 3.0, 4.0);

        assert!(matches!(event, PointerEvent::Enter(_)));
        assert_eq!(reducer.enter_serial(), Some(7));
        assert_eq!(
            reducer.primary_state.position,
            PhysicalPosition { x: 6.0, y: 8.0 }
        );
    }

    #[test]
    fn leave_clears_serial() {
        let mut reducer = PointerEventReducer::default();
        let _ = reducer.enter(7, 0.0, 0.0);

        let event = reducer.leave();

        assert!(matches!(event, PointerEvent::Leave(_)));
        assert_eq!(reducer.enter_serial(), None);
    }

    #[test]
    fn legacy_pointer_emits_scroll_per_axis_without_frame() {
        let mut reducer = PointerEventReducer::for_version(3);

        let events = reducer.reduce(
            1.0,
            &Event::Axis {
                time: 0,
                axis: WEnum::Value(Axis::VerticalScroll),
                value: 12.0,
            },
            1,
        );

        let PointerEvent::Scroll(scroll) = &events[0] else {
            panic!("expected a scroll without a frame");
        };
        assert_eq!(
            scroll.delta,
            ScrollDelta::PixelDelta(PhysicalPosition { x: 0.0, y: 12.0 })
        );
    }

    #[test]
    #[should_panic(expected = "timestamps must be monotonic nanoseconds")]
    fn non_monotonic_time_panics_in_debug() {
        let mut reducer = PointerEventReducer::default();
        let _ = reducer.reduce(1.0, &motion(1.0, 1.0), 2_000);
        let _ = reducer.reduce(1.0, &motion(2.0, 2.0), 1_000);
    }
}
