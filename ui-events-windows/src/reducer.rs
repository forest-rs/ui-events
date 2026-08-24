// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! [`EventReducer`]. See the crate-level documentation for an overview.

use std::mem;

use dpi::PhysicalPosition;
use ui_events::keyboard::{KeyState, KeyboardEvent};
use ui_events::pointer::{
    PointerButtonEvent, PointerEvent, PointerId, PointerInfo, PointerScrollEvent, PointerState,
    PointerType, PointerUpdate,
};
use ui_events::{ScrollDelta, text::TextInputEvent};
use windows_sys::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::ScreenToClient;
use windows_sys::Win32::UI::Controls::{HOVER_DEFAULT, WM_MOUSELEAVE};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    ReleaseCapture, SetCapture, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
};
use windows_sys::Win32::UI::Input::Touch::{
    CloseTouchInputHandle, GetTouchInputInfo, HTOUCHINPUT, TOUCHEVENTF_DOWN, TOUCHEVENTF_UP,
    TOUCHINPUT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    SPI_GETWHEELSCROLLCHARS, SPI_GETWHEELSCROLLLINES, SystemParametersInfoW, WHEEL_DELTA,
    WM_IME_COMPOSITION, WM_IME_ENDCOMPOSITION, WM_IME_STARTCOMPOSITION, WM_KEYDOWN, WM_KEYUP,
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEHWHEEL, WM_MOUSEMOVE,
    WM_MOUSEWHEEL, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SYSKEYDOWN, WM_SYSKEYUP, WM_TOUCH,
    WM_XBUTTONDOWN, WM_XBUTTONUP,
};

use crate::{keyboard, pointer, text};

/// Manages stateful transformations of raw Win32 window messages.
///
/// Store a single instance of this per window, then call [`EventReducer::reduce`] on each relevant
/// `WM_*` message for that window's `WNDPROC`.
/// Use the [`Event`] values to receive [`PointerEvent`], [`KeyboardEvent`],
/// and text-input event batches.
///
/// This handles:
///  - `WM_KEYDOWN`/`WM_KEYUP`/`WM_SYSKEYDOWN`/`WM_SYSKEYUP`
///  - `WM_IME_STARTCOMPOSITION`/`WM_IME_COMPOSITION`/`WM_IME_ENDCOMPOSITION`
///  - `WM_TOUCH`
///  - `WM_LBUTTONDOWN`/`WM_LBUTTONUP`/`WM_RBUTTONDOWN`/`WM_RBUTTONUP`/
///    `WM_MBUTTONDOWN`/`WM_MBUTTONUP`/`WM_XBUTTONDOWN`/`WM_XBUTTONUP`
///  - `WM_MOUSEWHEEL`/`WM_MOUSEHWHEEL`
///  - `WM_MOUSEMOVE`/`WM_MOUSELEAVE`
#[derive(Debug, Default)]
pub struct EventReducer {
    /// State of the primary mouse pointer.
    primary_state: PointerState,
    /// Whether the window currently has a non-empty IME composition.
    ime_composing: bool,
    /// Whether the cursor is currently known to be inside the window's client area,
    /// used to synthesize [`PointerEvent::Enter`] and to know when to re-arm `TrackMouseEvent`.
    mouse_in_window: bool,
    /// Click and tap counter, used to calculate [`PointerState::count`]
    counter: Vec<pointer::TapState>,
}

impl EventReducer {
    /// Process a raw Win32 window message.
    ///
    /// `hwnd`, `msg`, `wparam`, and `lparam` are exactly the parameters a `WNDPROC` receives
    /// for the window this reducer is tracking. Messages this reducer does not recognize produce
    /// an empty `Vec`. The caller should otherwise continue its normal default processing
    /// (e.g. calling `DefWindowProcW`) regardless of what this returns.
    ///
    /// `time` is monotonic nanoseconds in the consumer's event-stream clock domain.
    /// Every [`PointerState::time`] produced by this call uses this value.
    /// Passing the host/frame clock here lets a host keep input events, timers, frame samples,
    /// submission timestamps, and diagnostics on one timeline.
    ///
    /// The reducer does not interpret `time` as wall-clock or epoch time. It only preserves ordering
    /// and relative deltas within the caller's chosen clock domain. Tap detection depends on `time`
    /// being real monotonic nanoseconds because its timeout is measured in nanoseconds.
    ///
    /// # Safety
    ///
    ///   - `hwnd` must be a valid handle to a window.
    ///   - `msg`, `wparam` and `lparam` must come from an invocation of the window procedure
    ///     attached to `hwnd`.
    ///   - `scale_factor` must be correct for `hwnd`
    ///   - `time` must be nanoseconds timestamp that increases monotonically each call
    #[expect(
        clippy::cast_possible_truncation,
        reason = "Bitmasked and constant value, no data loss."
    )]
    pub unsafe fn reduce(
        &mut self,
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        scale_factor: f64,
        time: u64,
    ) -> Vec<Event> {
        const PRIMARY_MOUSE: PointerInfo = PointerInfo {
            pointer_id: Some(PointerId::PRIMARY),
            persistent_device_id: None,
            pointer_type: PointerType::Mouse,
        };

        self.check_time_monotonic_and_set(time);
        self.primary_state.scale_factor = scale_factor;
        self.primary_state.modifiers = keyboard::current_modifiers();

        match msg {
            WM_KEYDOWN | WM_SYSKEYDOWN => vec![Event::Keyboard(keyboard::from_win32(
                wparam,
                lparam,
                KeyState::Down,
            ))],

            WM_KEYUP | WM_SYSKEYUP => vec![Event::Keyboard(keyboard::from_win32(
                wparam,
                lparam,
                KeyState::Up,
            ))],
            WM_IME_STARTCOMPOSITION => Vec::new(),
            WM_IME_ENDCOMPOSITION => self.end_ime_composition().into_iter().collect(),
            WM_IME_COMPOSITION => {
                // SAFETY: `hwnd` is the window that received this message.
                let events = unsafe { text::from_imm(hwnd, lparam) };
                events.map_or_else(Vec::new, |mut events| {
                    let is_commit = matches!(events.first(), Some(TextInputEvent::Insert(_)));
                    let was_composing = mem::replace(
                        &mut self.ime_composing,
                        matches!(events.first(), Some(TextInputEvent::CompositionUpdate(_))),
                    );
                    if was_composing && is_commit {
                        events.insert(0, TextInputEvent::CompositionEnd);
                    }
                    vec![Event::Text(events)]
                })
            }
            WM_MOUSEMOVE => {
                let mut out = Vec::with_capacity(2);

                if !mem::replace(&mut self.mouse_in_window, true) {
                    let mut options = TRACKMOUSEEVENT {
                        cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: hwnd,
                        dwHoverTime: HOVER_DEFAULT,
                    };
                    // SAFETY: `options` is fully initialized and `hwnd` is valid.
                    unsafe { TrackMouseEvent(&mut options) };

                    out.push(Event::Pointer(PointerEvent::Enter(PRIMARY_MOUSE)));
                }

                self.primary_state.position = PhysicalPosition::new(
                    (lparam & 0xffff) as i16 as _,
                    (lparam >> 16 & 0xffff) as i16 as _,
                );

                out.push(Event::Pointer(self.attach_count(PointerEvent::Move(
                    PointerUpdate {
                        pointer: PRIMARY_MOUSE,
                        current: self.primary_state.clone(),
                        coalesced: Vec::new(),
                        predicted: Vec::new(),
                    },
                ))));

                out
            }
            WM_MOUSELEAVE => {
                self.mouse_in_window = false;
                vec![Event::Pointer(
                    self.attach_count(PointerEvent::Leave(PRIMARY_MOUSE)),
                )]
            }
            WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_XBUTTONDOWN => {
                // SAFETY: `hwnd` is valid.
                unsafe { SetCapture(hwnd) };

                let button = pointer::button_from_win32(msg, wparam);
                if let Some(button) = button {
                    self.primary_state.buttons.insert(button);
                }

                vec![Event::Pointer(self.attach_count(PointerEvent::Down(
                    PointerButtonEvent {
                        pointer: PRIMARY_MOUSE,
                        button,
                        state: self.primary_state.clone(),
                    },
                )))]
            }
            WM_LBUTTONUP | WM_RBUTTONUP | WM_MBUTTONUP | WM_XBUTTONUP => {
                let button = pointer::button_from_win32(msg, wparam);
                if let Some(button) = button {
                    self.primary_state.buttons.remove(button);
                }

                // This releases capture when all buttons of the primary mouse are released.
                if self.primary_state.buttons.is_empty() {
                    // SAFETY: `ReleaseCapture` is always safe to call, even without an active capture.
                    unsafe { ReleaseCapture() };
                }

                vec![Event::Pointer(self.attach_count(PointerEvent::Up(
                    PointerButtonEvent {
                        pointer: PRIMARY_MOUSE,
                        button,
                        state: self.primary_state.clone(),
                    },
                )))]
            }
            WM_MOUSEWHEEL => {
                let notches = ((wparam >> 16) as i16) as f32 / WHEEL_DELTA as f32;
                let lines = notches * scroll_multiplier(SPI_GETWHEELSCROLLLINES);
                vec![Event::Pointer(PointerEvent::Scroll(PointerScrollEvent {
                    pointer: PRIMARY_MOUSE,
                    delta: ScrollDelta::LineDelta(0.0, lines),
                    state: self.primary_state.clone(),
                }))]
            }
            WM_MOUSEHWHEEL => {
                let notches = ((wparam >> 16) as i16) as f32 / WHEEL_DELTA as f32;
                let characters = notches * scroll_multiplier(SPI_GETWHEELSCROLLCHARS);
                vec![Event::Pointer(PointerEvent::Scroll(PointerScrollEvent {
                    pointer: PRIMARY_MOUSE,
                    // NOTE: inverted, MSDN says positive means rightward rotation
                    //       which means leftward scroll in Windows convention.
                    delta: ScrollDelta::LineDelta(-characters, 0.0),
                    state: self.primary_state.clone(),
                }))]
            }
            WM_TOUCH => self.handle_touch(hwnd, wparam, lparam, time),
            _ => Vec::new(),
        }
    }

    /// Convert a `WM_TOUCH` message into zero or more pointer events,
    /// one per simultaneous touch point reported by the message.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "Bitmasked and constant value, no data loss."
    )]
    fn handle_touch(
        &mut self,
        hwnd: HWND,
        wparam: WPARAM,
        lparam: LPARAM,
        time: u64,
    ) -> Vec<Event> {
        let touch_count = wparam & 0xffff;
        let htouch = lparam as HTOUCHINPUT;
        let mut events = vec![TOUCHINPUT::default(); touch_count];

        // SAFETY: `inputs` has exactly `touch_count` elements, matching
        // `cInputs`, and `htouch` came directly from this message's `lparam`.
        if (unsafe {
            GetTouchInputInfo(
                htouch,
                touch_count as u32,
                events.as_mut_ptr(),
                size_of::<TOUCHINPUT>() as i32,
            )
        }) == 0
        {
            // SAFETY: `htouch` came from this message's `lparam` and has not yet been closed.
            unsafe { CloseTouchInputHandle(htouch) };
            return Vec::new();
        }

        let events = events
            .into_iter()
            .map(|event| {
                let mut point = POINT {
                    x: event.x / 100,
                    y: event.y / 100,
                };
                // SAFETY: `hwnd` is valid and `point` is a valid `POINT`.
                unsafe { ScreenToClient(hwnd, &mut point) };

                let pointer = PointerInfo {
                    pointer_id: PointerId::new((event.dwID as u64).saturating_add(1)),
                    pointer_type: PointerType::Touch,
                    persistent_device_id: None,
                };

                let flags = event.dwFlags;
                let state = PointerState {
                    time,
                    position: PhysicalPosition::new(point.x as f64, point.y as f64),
                    modifiers: self.primary_state.modifiers,
                    pressure: if flags & TOUCHEVENTF_UP != 0 { 0. } else { 0.5 },
                    scale_factor: self.primary_state.scale_factor,
                    ..Default::default()
                };

                let event = if flags & TOUCHEVENTF_DOWN != 0 {
                    PointerEvent::Down(PointerButtonEvent {
                        pointer,
                        button: None,
                        state,
                    })
                } else if flags & TOUCHEVENTF_UP != 0 {
                    PointerEvent::Up(PointerButtonEvent {
                        pointer,
                        button: None,
                        state,
                    })
                } else {
                    PointerEvent::Move(PointerUpdate {
                        pointer,
                        current: state,
                        coalesced: Vec::new(),
                        predicted: Vec::new(),
                    })
                };

                Event::Pointer(self.attach_count(event))
            })
            .collect();

        // SAFETY: `htouch` came from this message's `lparam` and has not yet been closed.
        unsafe { CloseTouchInputHandle(htouch) };

        events
    }

    fn end_ime_composition(&mut self) -> Option<Event> {
        mem::take(&mut self.ime_composing)
            .then(|| Event::Text(vec![TextInputEvent::CompositionEnd]))
    }

    fn check_time_monotonic_and_set(&mut self, time: u64) {
        let previous = mem::replace(&mut self.primary_state.time, time);
        debug_assert!(
            time >= previous,
            "EventReducer::reduce timestamps must be monotonic nanoseconds"
        );
    }

    /// Enhance a [`PointerEvent`] with a `count`.
    fn attach_count(&mut self, mut event: PointerEvent) -> PointerEvent {
        match &mut event {
            PointerEvent::Down(event) => {
                let pointer_id = event.pointer.pointer_id;
                let position = event.state.position;
                let time = event.state.time;

                let slop = match event.pointer.pointer_type {
                    // This is on the low side of double tap slop, validated experimentally
                    // to work on a few touchscreen laptops.
                    PointerType::Touch => 12.0,
                    PointerType::Pen => 6.0,
                    // This is slightly more forgiving than the default on Windows for mice.
                    // In order to make the slop calculation more similar between devices,
                    // this uses a slightly different method than Windows, which tests if the
                    // tap is in a box, rather than in a circle, centered on the anchor point.
                    _ => 2.0,
                } * std::f64::consts::SQRT_2
                    * self.primary_state.scale_factor;

                let make = |count| pointer::TapState {
                    pointer_id,
                    down_time: time,
                    up_time: time,
                    count,
                    x: position.x,
                    y: position.y,
                };

                if let Some(tap) = self
                    .counter
                    .iter_mut()
                    .find(|tap| tap.is_in_range(position, slop) && tap.is_valid_for(time))
                {
                    *tap = make(tap.count + 1);
                    event.state.count = tap.count;
                } else {
                    let state = make(1);
                    if let Some(tap) = self
                        .counter
                        .iter_mut()
                        .find(|tap| tap.pointer_is(pointer_id))
                    {
                        *tap = state;
                    } else {
                        self.counter.push(state);
                    }
                    event.state.count = 1;
                };
                self.clear_expired_taps(time);
            }
            PointerEvent::Up(event) => {
                if let Some(tap) = self
                    .counter
                    .iter_mut()
                    .find(|tap| tap.pointer_is(event.pointer.pointer_id))
                {
                    tap.up_time = event.state.time;
                    event.state.count = tap.count;
                }
            }
            PointerEvent::Move(PointerUpdate {
                pointer,
                current,
                coalesced,
                predicted,
            }) => {
                if let Some(count) = self.counter.iter().find_map(|tap| {
                    (tap.pointer_is(pointer.pointer_id) && tap.is_down()).then_some(tap.count)
                }) {
                    for event in coalesced
                        .iter_mut()
                        .chain(predicted.iter_mut())
                        .chain(Some(current))
                    {
                        event.count = count;
                    }
                }
            }
            PointerEvent::Cancel(p) | PointerEvent::Leave(p) => {
                self.counter.retain(|tap| tap.pointer_is(p.pointer_id));
            }
            _ => {}
        }

        event
    }

    /// Clear expired taps.
    ///
    /// `time` is the time of the last received event.
    fn clear_expired_taps(&mut self, time: u64) {
        self.counter
            .retain(|tap| tap.is_down() || tap.is_valid_for(time));
    }
}

/// Result of [`EventReducer::reduce`].
#[derive(Debug)]
pub enum Event {
    /// Resulting [`KeyboardEvent`].
    Keyboard(KeyboardEvent),
    /// Resulting [`PointerEvent`].
    Pointer(PointerEvent),
    /// Resulting [`TextInputEvent`] values.
    ///
    /// This is a batch because one platform event can map to more than one normalized text event.
    /// For example, committing an active IME composition emits [`TextInputEvent::CompositionEnd`]
    /// followed by the committed [`TextInputEvent::Insert`].
    Text(Vec<TextInputEvent>),
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "System provided value, should be no data loss."
)]
fn scroll_multiplier(param: u32) -> f32 {
    /// The default number of lines/characters scrolled per notch of a vertical
    /// or horizontal mouse wheel.
    const DEFAULT: isize = 3;

    let mut multiplier = DEFAULT;
    unsafe { SystemParametersInfoW(param, 0, std::ptr::from_mut(&mut multiplier).cast(), 0) };
    if multiplier as u32 == u32::MAX {
        // TODO: figure out how to handle page scrolls
        multiplier = DEFAULT;
    }
    multiplier as _
}
