// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! [`WindowMessageReducer`]. See the crate-level documentation for an overview.

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
    GetMessageExtraInfo, SPI_GETWHEELSCROLLCHARS, SPI_GETWHEELSCROLLLINES, SystemParametersInfoW,
    WHEEL_DELTA, WM_CAPTURECHANGED, WM_IME_COMPOSITION, WM_IME_ENDCOMPOSITION,
    WM_IME_STARTCOMPOSITION, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN,
    WM_MBUTTONUP, WM_MOUSEHWHEEL, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_RBUTTONDOWN, WM_RBUTTONUP,
    WM_SYSKEYDOWN, WM_SYSKEYUP, WM_TOUCH, WM_XBUTTONDOWN, WM_XBUTTONUP,
};

use crate::{keyboard, pointer, text};

const PRIMARY_MOUSE: PointerInfo = PointerInfo {
    pointer_id: Some(PointerId::PRIMARY),
    persistent_device_id: None,
    pointer_type: PointerType::Mouse,
};

/// Manages stateful transformations of raw Win32 window messages for one window.
///
/// Store a single instance of this per window, then call [`WindowMessageReducer::reduce`] on each
/// relevant `WM_*` message for that window's `WNDPROC`.
/// Use the [`InputEvent`] values to receive [`PointerEvent`], [`KeyboardEvent`],
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
#[derive(Debug)]
pub struct WindowMessageReducer {
    /// Window whose messages this reducer processes.
    hwnd: HWND,
    /// Physical pixels per logical pixel for this window.
    scale_factor: f64,
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

impl WindowMessageReducer {
    /// Create a reducer for one Win32 window.
    ///
    /// Call [`Self::set_scale_factor`] when the window's scale factor changes.
    ///
    /// # Safety
    ///
    /// `hwnd` must remain a valid window handle until the reducer is dropped or no longer used.
    pub unsafe fn new(hwnd: HWND, scale_factor: f64) -> Self {
        Self {
            hwnd,
            scale_factor,
            primary_state: PointerState::default(),
            ime_composing: false,
            mouse_in_window: false,
            counter: Vec::new(),
        }
    }

    /// Update the physical-pixels-per-logical-pixel scale for this window.
    pub fn set_scale_factor(&mut self, scale_factor: f64) {
        self.scale_factor = scale_factor;
    }

    /// Return whether this reducer recognizes `msg`.
    pub const fn handles_message(msg: u32) -> bool {
        matches!(
            msg,
            WM_KEYDOWN
                | WM_SYSKEYDOWN
                | WM_KEYUP
                | WM_SYSKEYUP
                | WM_IME_STARTCOMPOSITION
                | WM_IME_ENDCOMPOSITION
                | WM_IME_COMPOSITION
                | WM_MOUSEMOVE
                | WM_MOUSELEAVE
                | WM_LBUTTONDOWN
                | WM_RBUTTONDOWN
                | WM_MBUTTONDOWN
                | WM_XBUTTONDOWN
                | WM_LBUTTONUP
                | WM_RBUTTONUP
                | WM_MBUTTONUP
                | WM_XBUTTONUP
                | WM_MOUSEWHEEL
                | WM_MOUSEHWHEEL
                | WM_CAPTURECHANGED
                | WM_TOUCH
        )
    }

    /// Process a raw Win32 window message.
    ///
    /// `msg`, `wparam`, and `lparam` are exactly the parameters a `WNDPROC` receives for this
    /// reducer's window. Apply [`Reduction::response`] after dispatching the translated events
    /// instead of maintaining a second message table in the window procedure.
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
    ///   - `msg`, `wparam` and `lparam` must come from an invocation of the window procedure
    ///     attached to the `hwnd` passed to [`Self::new`].
    ///   - `time` must be nanoseconds timestamp that increases monotonically each call
    #[expect(
        clippy::cast_possible_truncation,
        reason = "Bitmasked and constant value, no data loss."
    )]
    pub unsafe fn reduce(
        &mut self,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        time: u64,
    ) -> Reduction {
        self.check_time_monotonic_and_set(time);
        self.primary_state.scale_factor = self.scale_factor;
        self.primary_state.modifiers = keyboard::current_modifiers();

        if is_legacy_mouse_message(msg) {
            // SAFETY: `GetMessageExtraInfo` only reads metadata associated with the current
            // thread's message.
            let extra_info = unsafe { GetMessageExtraInfo() };
            if pointer::is_promoted_touch(extra_info) {
                return Reduction {
                    events: Vec::new(),
                    response: MessageResponse::Consume(0),
                };
            }
        }

        let events = match msg {
            WM_KEYDOWN | WM_SYSKEYDOWN => vec![InputEvent::Keyboard(keyboard::from_win32(
                wparam,
                lparam,
                KeyState::Down,
            ))],

            WM_KEYUP | WM_SYSKEYUP => vec![InputEvent::Keyboard(keyboard::from_win32(
                wparam,
                lparam,
                KeyState::Up,
            ))],
            WM_IME_STARTCOMPOSITION => Vec::new(),
            WM_IME_ENDCOMPOSITION => self.end_ime_composition().into_iter().collect(),
            WM_IME_COMPOSITION => {
                // SAFETY: `hwnd` is the window that received this message.
                let events = unsafe { text::from_imm(self.hwnd, lparam) };
                events.map_or_else(Vec::new, |mut events| {
                    let is_commit = matches!(events.first(), Some(TextInputEvent::Insert(_)));
                    let was_composing = mem::replace(
                        &mut self.ime_composing,
                        matches!(events.first(), Some(TextInputEvent::CompositionUpdate(_))),
                    );
                    if was_composing && is_commit {
                        events.insert(0, TextInputEvent::CompositionEnd);
                    }
                    vec![InputEvent::Text(events)]
                })
            }
            WM_MOUSEMOVE => {
                let mut out = Vec::with_capacity(2);

                if !mem::replace(&mut self.mouse_in_window, true) {
                    let mut options = TRACKMOUSEEVENT {
                        cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: self.hwnd,
                        dwHoverTime: HOVER_DEFAULT,
                    };
                    // SAFETY: `options` is fully initialized and `self.hwnd` is valid.
                    unsafe { TrackMouseEvent(&mut options) };

                    out.push(InputEvent::Pointer(PointerEvent::Enter(PRIMARY_MOUSE)));
                }

                self.primary_state.position = pointer::position_from_lparam(lparam);

                out.push(InputEvent::Pointer(self.attach_count(PointerEvent::Move(
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
                vec![InputEvent::Pointer(
                    self.attach_count(PointerEvent::Leave(PRIMARY_MOUSE)),
                )]
            }
            WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_XBUTTONDOWN => {
                self.primary_state.position = pointer::position_from_lparam(lparam);

                // SAFETY: `self.hwnd` is valid.
                unsafe { SetCapture(self.hwnd) };

                let button = pointer::button_from_win32(msg, wparam);
                if let Some(button) = button {
                    self.primary_state.buttons.insert(button);
                }

                vec![InputEvent::Pointer(self.attach_count(PointerEvent::Down(
                    PointerButtonEvent {
                        pointer: PRIMARY_MOUSE,
                        button,
                        state: self.primary_state.clone(),
                    },
                )))]
            }
            WM_LBUTTONUP | WM_RBUTTONUP | WM_MBUTTONUP | WM_XBUTTONUP => {
                self.primary_state.position = pointer::position_from_lparam(lparam);

                let button = pointer::button_from_win32(msg, wparam);
                if let Some(button) = button {
                    self.primary_state.buttons.remove(button);
                }

                // This releases capture when all buttons of the primary mouse are released.
                if self.primary_state.buttons.is_empty() {
                    // SAFETY: `ReleaseCapture` is always safe to call, even without an active capture.
                    unsafe { ReleaseCapture() };
                }

                vec![InputEvent::Pointer(self.attach_count(PointerEvent::Up(
                    PointerButtonEvent {
                        pointer: PRIMARY_MOUSE,
                        button,
                        state: self.primary_state.clone(),
                    },
                )))]
            }
            WM_MOUSEWHEEL => {
                self.update_wheel_position(lparam);
                let notches = ((wparam >> 16) as i16) as f32 / WHEEL_DELTA as f32;
                let lines = notches * scroll_multiplier(SPI_GETWHEELSCROLLLINES);
                vec![InputEvent::Pointer(PointerEvent::Scroll(
                    PointerScrollEvent {
                        pointer: PRIMARY_MOUSE,
                        delta: ScrollDelta::LineDelta(0.0, lines),
                        state: self.primary_state.clone(),
                    },
                ))]
            }
            WM_MOUSEHWHEEL => {
                self.update_wheel_position(lparam);
                let notches = ((wparam >> 16) as i16) as f32 / WHEEL_DELTA as f32;
                let characters = notches * scroll_multiplier(SPI_GETWHEELSCROLLCHARS);
                vec![InputEvent::Pointer(PointerEvent::Scroll(
                    PointerScrollEvent {
                        pointer: PRIMARY_MOUSE,
                        // NOTE: inverted, MSDN says positive means rightward rotation
                        //       which means leftward scroll in Windows convention.
                        delta: ScrollDelta::LineDelta(-characters, 0.0),
                        state: self.primary_state.clone(),
                    },
                ))]
            }
            WM_CAPTURECHANGED => self.handle_capture_changed().into_iter().collect(),
            WM_TOUCH => self.handle_touch(wparam, lparam, time),
            _ => Vec::new(),
        };

        Reduction {
            events,
            response: response_for_message(msg),
        }
    }

    /// Convert a `WM_TOUCH` message into zero or more pointer events,
    /// one per simultaneous touch point reported by the message.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "Bitmasked and constant value, no data loss."
    )]
    fn handle_touch(&mut self, wparam: WPARAM, lparam: LPARAM, time: u64) -> Vec<InputEvent> {
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
                // SAFETY: `self.hwnd` is valid and `point` is a valid `POINT`.
                unsafe { ScreenToClient(self.hwnd, &mut point) };

                let pointer = PointerInfo {
                    pointer_id: pointer::touch_pointer_id(event.dwID),
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

                InputEvent::Pointer(self.attach_count(event))
            })
            .collect();

        // SAFETY: `htouch` came from this message's `lparam` and has not yet been closed.
        unsafe { CloseTouchInputHandle(htouch) };

        events
    }

    fn update_wheel_position(&mut self, lparam: LPARAM) {
        let mut point = pointer::point_from_lparam(lparam);
        // SAFETY: `self.hwnd` is valid and `point` is a valid `POINT`.
        if unsafe { ScreenToClient(self.hwnd, &mut point) } != 0 {
            self.primary_state.position =
                PhysicalPosition::new(f64::from(point.x), f64::from(point.y));
        }
    }

    fn handle_capture_changed(&mut self) -> Option<InputEvent> {
        if self.primary_state.buttons.is_empty() {
            return None;
        }
        self.primary_state.buttons.clear();
        Some(InputEvent::Pointer(
            self.attach_count(PointerEvent::Cancel(PRIMARY_MOUSE)),
        ))
    }

    fn end_ime_composition(&mut self) -> Option<InputEvent> {
        mem::take(&mut self.ime_composing)
            .then(|| InputEvent::Text(vec![TextInputEvent::CompositionEnd]))
    }

    fn check_time_monotonic_and_set(&mut self, time: u64) {
        let previous = mem::replace(&mut self.primary_state.time, time);
        debug_assert!(
            time >= previous,
            "WindowMessageReducer::reduce timestamps must be monotonic nanoseconds"
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
                self.counter.retain(|tap| !tap.pointer_is(p.pointer_id));
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

const fn is_legacy_mouse_message(msg: u32) -> bool {
    matches!(
        msg,
        WM_MOUSEMOVE
            | WM_LBUTTONDOWN
            | WM_LBUTTONUP
            | WM_RBUTTONDOWN
            | WM_RBUTTONUP
            | WM_MBUTTONDOWN
            | WM_MBUTTONUP
            | WM_XBUTTONDOWN
            | WM_XBUTTONUP
            | WM_MOUSEWHEEL
            | WM_MOUSEHWHEEL
    )
}

/// A translated input event produced by [`WindowMessageReducer::reduce`].
#[derive(Debug)]
pub enum InputEvent {
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

/// Native window-procedure disposition for a reduced message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageResponse {
    /// Pass the message to `DefWindowProcW`.
    Forward,
    /// Return the contained `LRESULT` without calling `DefWindowProcW`.
    Consume(isize),
    /// Return the contained `LRESULT` if the application handled a translated event;
    /// otherwise pass the message to `DefWindowProcW`.
    ConsumeIfHandled(isize),
}

/// Result of [`WindowMessageReducer::reduce`].
#[derive(Debug)]
pub struct Reduction {
    /// Zero or more normalized input events.
    pub events: Vec<InputEvent>,
    /// Required native disposition after the application dispatches `events`.
    pub response: MessageResponse,
}

const fn response_for_message(msg: u32) -> MessageResponse {
    match msg {
        // The reducer closes the message's touch input handle, so forwarding it would pass an
        // invalid handle to DefWindowProcW.
        WM_TOUCH => MessageResponse::Consume(0),
        // Win32 requires TRUE when an application handles an X-button message.
        WM_XBUTTONDOWN | WM_XBUTTONUP => MessageResponse::ConsumeIfHandled(1),
        msg if WindowMessageReducer::handles_message(msg) => MessageResponse::ConsumeIfHandled(0),
        _ => MessageResponse::Forward,
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use ui_events::pointer::PointerButton;
    use windows_sys::Win32::UI::WindowsAndMessaging::WM_APP;

    fn test_reducer() -> WindowMessageReducer {
        WindowMessageReducer {
            hwnd: core::ptr::null_mut(),
            scale_factor: 1.0,
            primary_state: PointerState::default(),
            ime_composing: false,
            mouse_in_window: false,
            counter: Vec::new(),
        }
    }

    #[test]
    fn message_disposition_preserves_win32_return_contracts() {
        assert_eq!(response_for_message(WM_TOUCH), MessageResponse::Consume(0));
        assert_eq!(
            response_for_message(WM_XBUTTONDOWN),
            MessageResponse::ConsumeIfHandled(1)
        );
        assert_eq!(
            response_for_message(WM_XBUTTONUP),
            MessageResponse::ConsumeIfHandled(1)
        );
        assert_eq!(
            response_for_message(WM_MOUSEMOVE),
            MessageResponse::ConsumeIfHandled(0)
        );
        assert_eq!(response_for_message(WM_APP), MessageResponse::Forward);
    }

    #[test]
    fn recognized_message_table_matches_disposition_table() {
        assert!(WindowMessageReducer::handles_message(WM_TOUCH));
        assert!(WindowMessageReducer::handles_message(WM_SYSKEYDOWN));
        assert!(WindowMessageReducer::handles_message(WM_MOUSELEAVE));
        assert!(WindowMessageReducer::handles_message(WM_CAPTURECHANGED));
        assert!(!WindowMessageReducer::handles_message(WM_APP));
    }

    #[test]
    fn cancel_removes_only_the_cancelled_pointers_tap_state() {
        let mut reducer = test_reducer();
        let other_pointer = pointer::touch_pointer_id(0);
        reducer.counter = vec![
            pointer::TapState {
                pointer_id: PRIMARY_MOUSE.pointer_id,
                down_time: 1,
                up_time: 1,
                count: 1,
                x: 0.0,
                y: 0.0,
            },
            pointer::TapState {
                pointer_id: other_pointer,
                down_time: 1,
                up_time: 1,
                count: 1,
                x: 0.0,
                y: 0.0,
            },
        ];

        reducer.attach_count(PointerEvent::Cancel(PRIMARY_MOUSE));

        assert_eq!(reducer.counter.len(), 1);
        assert!(reducer.counter[0].pointer_is(other_pointer));
    }

    #[test]
    fn capture_loss_cancels_pressed_mouse_buttons() {
        let mut reducer = test_reducer();
        reducer.primary_state.buttons.insert(PointerButton::Primary);

        let event = reducer.handle_capture_changed();

        assert!(reducer.primary_state.buttons.is_empty());
        assert!(matches!(
            event,
            Some(InputEvent::Pointer(PointerEvent::Cancel(pointer)))
                if pointer == PRIMARY_MOUSE
        ));
    }
}
