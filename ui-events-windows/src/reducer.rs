// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! [`WindowEventReducer`], the Win32 analogue of `ui-events-winit`'s reducer
//! of the same name. See the crate-level documentation for an overview.

use std::mem::size_of;

use dpi::PhysicalPosition;
use ui_events::{
    ScrollDelta,
    keyboard::KeyState,
    pointer::{
        PointerButtonEvent, PointerEvent, PointerId, PointerInfo, PointerScrollEvent,
        PointerState, PointerType, PointerUpdate,
    },
    text::TextInputEvent,
};
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
    WHEEL_DELTA, WM_IME_COMPOSITION, WM_IME_ENDCOMPOSITION, WM_IME_STARTCOMPOSITION, WM_KEYDOWN,
    WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEHWHEEL,
    WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
    WM_TOUCH, WM_XBUTTONDOWN, WM_XBUTTONUP,
};

use crate::{keyboard, pointer, text};

/// The default number of lines scrolled per notch of a vertical or
/// horizontal mouse wheel, used as a fallback in place of querying
/// `SystemParametersInfoW(SPI_GETWHEELSCROLLLINES | SPI_GETWHEELSCROLLCHARS)`.
///
/// Inlined from `winit-win32`'s `DEFAULT_SCROLL_LINES_PER_WHEEL_DELTA`/
/// `DEFAULT_SCROLL_CHARACTERS_PER_WHEEL_DELTA` (both `3` upstream); this
/// crate always uses the default rather than respecting the user's "Mouse
/// Properties" wheel-speed setting, which is a simplification relative to
/// upstream `winit`.
const DEFAULT_SCROLL_LINES_PER_WHEEL_DELTA: f32 = 3.0;

/// Manages stateful transformations of raw Win32 window messages.
///
/// Store a single instance of this per window, then call
/// [`WindowEventReducer::reduce`] on each relevant `WM_*` message for that
/// window's `WNDPROC`.
/// Use the [`WindowEventTranslation`] values to receive [`PointerEvent`],
/// [`KeyboardEvent`], and text-input event batches.
///
/// This handles:
///  - `WM_KEYDOWN`/`WM_KEYUP`/`WM_SYSKEYDOWN`/`WM_SYSKEYUP`
///  - `WM_IME_STARTCOMPOSITION`/`WM_IME_COMPOSITION`/`WM_IME_ENDCOMPOSITION`
///  - `WM_TOUCH`
///  - `WM_LBUTTONDOWN`/`WM_LBUTTONUP`/`WM_RBUTTONDOWN`/`WM_RBUTTONUP`/
///    `WM_MBUTTONDOWN`/`WM_MBUTTONUP`/`WM_XBUTTONDOWN`/`WM_XBUTTONUP`
///  - `WM_MOUSEWHEEL`/`WM_MOUSEHWHEEL`
///  - `WM_MOUSEMOVE`/`WM_MOUSELEAVE`
///
/// [`KeyboardEvent`]: ui_events::keyboard::KeyboardEvent
#[derive(Debug, Default)]
pub struct WindowEventReducer {
    /// State of the primary mouse pointer.
    primary_state: PointerState,
    /// Click and tap counter.
    counter: TapCounter,
    /// Whether the window currently has a non-empty IME composition.
    ime_composing: bool,
    /// Whether the cursor is currently known to be inside the window's
    /// client area, used to synthesize [`PointerEvent::Enter`] and to know
    /// when to re-arm `TrackMouseEvent`.
    mouse_in_window: bool,
    /// Last caller-provided timestamp seen by the reducer.
    last_seen_time: Option<u64>,
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "There is no alternative to truncation here."
)]
impl WindowEventReducer {
    /// Process a raw Win32 window message.
    ///
    /// `hwnd`, `msg`, `wparam`, and `lparam` are exactly the parameters a
    /// `WNDPROC` receives for the window this reducer is tracking. Messages
    /// this reducer does not recognize produce an empty `Vec`; the caller
    /// should otherwise continue its normal default processing (e.g. calling
    /// `DefWindowProcW`) regardless of what this returns.
    ///
    /// `time` is monotonic nanoseconds in the consumer's event-stream clock
    /// domain. Every [`PointerState::time`] produced by this call uses this
    /// value. Passing the host/frame clock here lets a host keep input events,
    /// timers, frame samples, submission timestamps, and diagnostics on one
    /// timeline.
    ///
    /// The reducer does not interpret `time` as wall-clock or epoch time; it
    /// only preserves ordering and relative deltas within the caller's chosen
    /// clock domain. Tap detection depends on `time` being real monotonic
    /// nanoseconds because its timeout is measured in nanoseconds.
    pub fn reduce(
        &mut self,
        scale_factor: f64,
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        time: u64,
    ) -> Vec<WindowEventTranslation> {
        const PRIMARY_MOUSE: PointerInfo = PointerInfo {
            pointer_id: Some(PointerId::PRIMARY),
            persistent_device_id: None,
            pointer_type: PointerType::Mouse,
        };

        self.check_time_monotonic(time);
        self.primary_state.time = time;
        self.primary_state.scale_factor = scale_factor;
        self.primary_state.modifiers = keyboard::current_modifiers();

        match msg {
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                vec![WindowEventTranslation::Keyboard(
                    keyboard::from_win32_keyboard_event(wparam, lparam, KeyState::Down),
                )]
            }
            WM_KEYUP | WM_SYSKEYUP => {
                vec![WindowEventTranslation::Keyboard(
                    keyboard::from_win32_keyboard_event(wparam, lparam, KeyState::Up),
                )]
            }
            WM_IME_STARTCOMPOSITION => Vec::new(),
            WM_IME_ENDCOMPOSITION => self.end_ime_composition().into_iter().collect(),
            WM_IME_COMPOSITION => {
                let was_composing = self.ime_composing;
                // SAFETY: `hwnd` is the window that received this message,
                // per this function's contract.
                let events = unsafe { text::from_win32_ime_composition(hwnd, lparam) };
                match events {
                    Some(mut events) => {
                        let is_commit =
                            matches!(events.first(), Some(TextInputEvent::Insert(_)));
                        self.ime_composing = matches!(
                            events.first(),
                            Some(TextInputEvent::CompositionUpdate(_))
                        );
                        if was_composing && is_commit {
                            events.insert(0, TextInputEvent::CompositionEnd);
                        }
                        vec![WindowEventTranslation::Text(events)]
                    }
                    None => Vec::new(),
                }
            }
            WM_MOUSEMOVE => {
                let mut out = Vec::with_capacity(2);

                if !self.mouse_in_window {
                    self.mouse_in_window = true;

                    let mut tme = TRACKMOUSEEVENT {
                        cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: hwnd,
                        dwHoverTime: HOVER_DEFAULT,
                    };
                    // SAFETY: `tme` is fully initialized and `hwnd` is valid.
                    unsafe { TrackMouseEvent(&mut tme) };

                    out.push(WindowEventTranslation::Pointer(PointerEvent::Enter(
                        PRIMARY_MOUSE,
                    )));
                }

                self.primary_state.position =
                    PhysicalPosition::new(get_x_lparam(lparam) as f64, get_y_lparam(lparam) as f64);

                out.push(WindowEventTranslation::Pointer(self.counter.attach_count(
                    scale_factor,
                    PointerEvent::Move(PointerUpdate {
                        pointer: PRIMARY_MOUSE,
                        current: self.primary_state.clone(),
                        coalesced: Vec::new(),
                        predicted: Vec::new(),
                    }),
                )));

                out
            }
            WM_MOUSELEAVE => {
                self.mouse_in_window = false;
                vec![WindowEventTranslation::Pointer(self.counter.attach_count(
                    scale_factor,
                    PointerEvent::Leave(PRIMARY_MOUSE),
                ))]
            }
            WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_XBUTTONDOWN => {
                // SAFETY: `hwnd` is valid, per this function's contract.
                unsafe { SetCapture(hwnd) };

                let button = pointer::try_from_win32_button(msg, wparam);
                if let Some(button) = button {
                    self.primary_state.buttons.insert(button);
                }

                vec![WindowEventTranslation::Pointer(self.counter.attach_count(
                    scale_factor,
                    PointerEvent::Down(PointerButtonEvent {
                        pointer: PRIMARY_MOUSE,
                        button,
                        state: self.primary_state.clone(),
                    }),
                ))]
            }
            WM_LBUTTONUP | WM_RBUTTONUP | WM_MBUTTONUP | WM_XBUTTONUP => {
                let button = pointer::try_from_win32_button(msg, wparam);
                if let Some(button) = button {
                    self.primary_state.buttons.remove(button);
                }

                // This releases capture unconditionally; if more than one
                // button is held, this drops capture as soon as any one of
                // them is released, which is a simplification relative to
                // `winit-win32`'s reference-counted `capture_count`.
                if self.primary_state.buttons.is_empty() {
                    // SAFETY: `ReleaseCapture` takes no arguments and is
                    // always safe to call, even without an active capture.
                    unsafe { ReleaseCapture() };
                }

                vec![WindowEventTranslation::Pointer(self.counter.attach_count(
                    scale_factor,
                    PointerEvent::Up(PointerButtonEvent {
                        pointer: PRIMARY_MOUSE,
                        button,
                        state: self.primary_state.clone(),
                    }),
                ))]
            }
            WM_MOUSEWHEEL => {
                let notches = ((wparam >> 16) as i16) as f32 / WHEEL_DELTA as f32;
                let lines = notches * DEFAULT_SCROLL_LINES_PER_WHEEL_DELTA;
                vec![WindowEventTranslation::Pointer(PointerEvent::Scroll(
                    PointerScrollEvent {
                        pointer: PRIMARY_MOUSE,
                        delta: ScrollDelta::LineDelta(0.0, lines),
                        state: self.primary_state.clone(),
                    },
                ))]
            }
            WM_MOUSEHWHEEL => {
                // NOTE: inverted, matching `winit-win32`'s `WM_MOUSEHWHEEL` handling.
                // See https://github.com/rust-windowing/winit/pull/2105/
                let notches = -(((wparam >> 16) as i16) as f32) / WHEEL_DELTA as f32;
                let characters = notches * DEFAULT_SCROLL_LINES_PER_WHEEL_DELTA;
                vec![WindowEventTranslation::Pointer(PointerEvent::Scroll(
                    PointerScrollEvent {
                        pointer: PRIMARY_MOUSE,
                        delta: ScrollDelta::LineDelta(characters, 0.0),
                        state: self.primary_state.clone(),
                    },
                ))]
            }
            WM_TOUCH => self.handle_touch(scale_factor, hwnd, wparam, lparam, time),
            _ => Vec::new(),
        }
    }

    /// Convert a `WM_TOUCH` message into zero or more pointer events, one
    /// per simultaneous touch point reported by the message.
    ///
    /// Inlined from `winit-win32`'s `WM_TOUCH` handling in `event_loop.rs`.
    fn handle_touch(
        &mut self,
        scale_factor: f64,
        hwnd: HWND,
        wparam: WPARAM,
        lparam: LPARAM,
        time: u64,
    ) -> Vec<WindowEventTranslation> {
        let touch_count = wparam & 0xffff;
        let htouch = lparam as HTOUCHINPUT;
        let mut inputs = vec![
            TOUCHINPUT {
                x: 0,
                y: 0,
                hSource: std::ptr::null_mut(),
                dwID: 0,
                dwFlags: 0,
                dwMask: 0,
                dwTime: 0,
                dwExtraInfo: 0,
                cxContact: 0,
                cyContact: 0,
            };
            touch_count
        ];

        // SAFETY: `inputs` has exactly `touch_count` elements, matching
        // `cInputs`, and `htouch` came directly from this message's `lParam`.
        let ok = unsafe {
            GetTouchInputInfo(
                htouch,
                touch_count as u32,
                inputs.as_mut_ptr(),
                size_of::<TOUCHINPUT>() as i32,
            )
        };

        let mut out = Vec::with_capacity(touch_count);
        if ok != 0 {
            for input in &inputs {
                let mut point = POINT {
                    x: input.x / 100,
                    y: input.y / 100,
                };
                // SAFETY: `hwnd` is valid and `point` is a valid `POINT`.
                unsafe { ScreenToClient(hwnd, &mut point) };

                let pointer = PointerInfo {
                    pointer_id: PointerId::new((input.dwID as u64).saturating_add(1)),
                    pointer_type: PointerType::Touch,
                    persistent_device_id: None,
                };

                let flags = input.dwFlags;
                let state = PointerState {
                    time,
                    position: PhysicalPosition::new(point.x as f64, point.y as f64),
                    modifiers: self.primary_state.modifiers,
                    pressure: if flags & TOUCHEVENTF_UP != 0 {
                        0.0
                    } else {
                        0.5
                    },
                    scale_factor,
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

                out.push(WindowEventTranslation::Pointer(
                    self.counter.attach_count(scale_factor, event),
                ));
            }
        }

        // SAFETY: `htouch` came directly from this message's `lParam` and
        // has not yet been closed.
        unsafe { CloseTouchInputHandle(htouch) };

        out
    }

    fn end_ime_composition(&mut self) -> Option<WindowEventTranslation> {
        if self.ime_composing {
            self.ime_composing = false;
            Some(WindowEventTranslation::Text(vec![
                TextInputEvent::CompositionEnd,
            ]))
        } else {
            None
        }
    }

    fn check_time_monotonic(&mut self, time: u64) {
        if let Some(previous) = self.last_seen_time {
            debug_assert!(
                time >= previous,
                "WindowEventReducer::reduce timestamps must be monotonic nanoseconds"
            );
        }
        self.last_seen_time = Some(time);
    }
}

/// Extract the low-order 16 bits of `lparam`, sign-extended as the X coordinate.
///
/// Inlined from `winit-win32`'s `util::get_x_lparam` (`GET_X_LPARAM`).
fn get_x_lparam(lparam: LPARAM) -> i16 {
    (lparam as u32 & 0xffff) as i16
}

/// Extract the high-order 16 bits of `lparam`, sign-extended as the Y coordinate.
///
/// Inlined from `winit-win32`'s `util::get_y_lparam` (`GET_Y_LPARAM`).
fn get_y_lparam(lparam: LPARAM) -> i16 {
    ((lparam as u32 >> 16) & 0xffff) as i16
}

/// Result of [`WindowEventReducer::reduce`].
#[derive(Debug)]
pub enum WindowEventTranslation {
    /// Resulting [`KeyboardEvent`].
    ///
    /// [`KeyboardEvent`]: ui_events::keyboard::KeyboardEvent
    Keyboard(ui_events::keyboard::KeyboardEvent),
    /// Resulting [`PointerEvent`].
    Pointer(PointerEvent),
    /// Resulting [`TextInputEvent`] values.
    ///
    /// This is a batch because one platform event can map to more than one
    /// normalized text event. For example, committing an active IME
    /// composition emits [`TextInputEvent::CompositionEnd`] followed by the
    /// committed [`TextInputEvent::Insert`].
    Text(Vec<TextInputEvent>),
}

#[derive(Clone, Debug)]
struct TapState {
    /// Pointer ID used to attach tap counts to [`PointerEvent::Move`].
    pointer_id: Option<PointerId>,
    /// Nanosecond timestamp when the tap went Down.
    down_time: u64,
    /// Nanosecond timestamp when the tap went Up.
    ///
    /// Resets to `down_time` when tap goes Down.
    up_time: u64,
    /// The local tap count as of the last Down phase.
    count: u8,
    /// x coordinate.
    x: f64,
    /// y coordinate.
    y: f64,
}

#[derive(Debug, Default)]
struct TapCounter {
    taps: Vec<TapState>,
}

impl TapCounter {
    /// Enhance a [`PointerEvent`] with a `count`.
    fn attach_count(&mut self, scale_factor: f64, e: PointerEvent) -> PointerEvent {
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
    /// `t` is the time of the last received event.
    fn clear_expired(&mut self, t: u64) {
        self.taps.retain(
            |TapState {
                 down_time, up_time, ..
             }| { down_time == up_time || (up_time + 500_000_000) > t },
        );
    }
}
