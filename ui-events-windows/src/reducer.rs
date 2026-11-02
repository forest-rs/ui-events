// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! [`WindowMessageReducer`]. See the crate-level documentation for an overview.

use std::{error::Error, fmt, mem};

use dpi::{PhysicalPosition, Position, Size};
use ui_events::keyboard::KeyState;
use ui_events::pointer::{
    PointerButtonEvent, PointerEvent, PointerId, PointerInfo, PointerScrollEvent, PointerState,
    PointerType, PointerUpdate,
};
use ui_events::text::{TextInputEvent, TextInsertEvent};
use windows_sys::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::ScreenToClient;
use windows_sys::Win32::UI::Controls::{HOVER_DEFAULT, WM_MOUSELEAVE};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    ReleaseCapture, SetCapture, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
};
use windows_sys::Win32::UI::Input::Touch::{
    CloseTouchInputHandle, GetTouchInputInfo, HTOUCHINPUT, IsTouchWindow, TOUCHEVENTF_DOWN,
    TOUCHEVENTF_UP, TOUCHINPUT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetMessageExtraInfo, SPI_GETWHEELSCROLLCHARS, SPI_GETWHEELSCROLLLINES, WHEEL_DELTA,
    WM_CAPTURECHANGED, WM_CHAR, WM_IME_COMPOSITION, WM_IME_ENDCOMPOSITION, WM_IME_SETCONTEXT,
    WM_IME_STARTCOMPOSITION, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN,
    WM_MBUTTONUP, WM_MOUSEHWHEEL, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_RBUTTONDOWN, WM_RBUTTONUP,
    WM_SYSKEYDOWN, WM_SYSKEYUP, WM_TOUCH, WM_XBUTTONDOWN, WM_XBUTTONUP,
};

use crate::{keyboard, pointer, text};

mod message;
mod wheel;

use message::response_for_message;
pub use message::{InputEvent, MessageResponse, Reduction};
use wheel::{WheelAxis, wheel_delta, wheel_preference};

const PRIMARY_MOUSE: PointerInfo = PointerInfo {
    pointer_id: Some(PointerId::PRIMARY),
    persistent_device_id: None,
    pointer_type: PointerType::Mouse,
};

/// Manages stateful transformations of raw Win32 window messages for one window.
///
/// Store a single instance of this per window, then call [`WindowMessageReducer::reduce`] on each
/// relevant `WM_*` message for that window's `WNDPROC`.
/// Use the [`InputEvent`] values to receive [`PointerEvent`],
/// [`KeyboardEvent`](ui_events::keyboard::KeyboardEvent),
/// and text-input event batches.
///
/// This handles:
///  - `WM_KEYDOWN`/`WM_KEYUP`/`WM_SYSKEYDOWN`/`WM_SYSKEYUP`
///  - `WM_CHAR`
///  - `WM_IME_SETCONTEXT`/`WM_IME_STARTCOMPOSITION`/`WM_IME_COMPOSITION`/
///    `WM_IME_ENDCOMPOSITION`
///  - `WM_TOUCH`
///  - `WM_LBUTTONDOWN`/`WM_LBUTTONUP`/`WM_RBUTTONDOWN`/`WM_RBUTTONUP`/
///    `WM_MBUTTONDOWN`/`WM_MBUTTONUP`/`WM_XBUTTONDOWN`/`WM_XBUTTONUP`
///  - `WM_MOUSEWHEEL`/`WM_MOUSEHWHEEL`
///  - `WM_MOUSEMOVE`/`WM_MOUSELEAVE`/`WM_CAPTURECHANGED`
#[derive(Debug)]
pub struct WindowMessageReducer {
    /// Window whose messages this reducer processes.
    hwnd: HWND,
    /// Physical pixels per logical pixel for this window.
    scale_factor: f64,
    /// State of the primary mouse pointer.
    primary_state: PointerState,
    /// Whether the window currently has an active IME composition session.
    ime_active: bool,
    /// Whether a composition update has installed preedit text in the editor.
    ime_preediting: bool,
    /// Whether IMM32 text input is enabled for this window.
    ime_allowed: bool,
    /// Last requested client-space area for IMM32 composition and candidate UI.
    ime_cursor_area: Option<(Position, Size)>,
    /// Decoder state for UTF-16 surrogate pairs split across `WM_CHAR` messages.
    wm_char_decoder: text::WmCharDecoder,
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
    /// The reducer must be used on the thread that owns that window.
    pub unsafe fn new(hwnd: HWND, scale_factor: f64) -> Self {
        Self {
            hwnd,
            scale_factor,
            primary_state: PointerState::default(),
            ime_active: false,
            ime_preediting: false,
            ime_allowed: true,
            ime_cursor_area: None,
            wm_char_decoder: text::WmCharDecoder::default(),
            mouse_in_window: false,
            counter: Vec::new(),
        }
    }

    /// Returns the native window whose messages this reducer processes.
    pub const fn hwnd(&self) -> HWND {
        self.hwnd
    }

    /// Returns the physical pixels per logical pixel used for this window.
    pub const fn scale_factor(&self) -> f64 {
        self.scale_factor
    }

    /// Update the physical-pixels-per-logical-pixel scale for this window.
    pub fn set_scale_factor(&mut self, scale_factor: f64) {
        self.scale_factor = scale_factor;
        self.apply_ime_cursor_area();
    }

    /// Enable or disable IMM32 text input for this window.
    ///
    /// This restores the window's default input context when `allowed` is `true` and disassociates
    /// it when `allowed` is `false`. Call this when a text-editing control gains or loses focus.
    ///
    /// Disabling an active composition returns a [`TextInputEvent::CompositionEnd`] wrapped as an
    /// [`InputEvent`]. Dispatch it through the same path as events returned by [`Self::reduce`].
    ///
    /// The reducer initially accepts IME messages without changing the window's native input
    /// context. Call this method at text-focus transitions to make the association explicit.
    ///
    /// # Errors
    ///
    /// Returns [`ImeAssociationError`] when Windows cannot update the input-context association.
    pub fn set_ime_allowed(
        &mut self,
        allowed: bool,
    ) -> Result<Option<InputEvent>, ImeAssociationError> {
        // SAFETY: `self.hwnd` remains valid by the invariant of `Self::new`.
        if !unsafe { text::set_ime_allowed(self.hwnd, allowed) } {
            return Err(ImeAssociationError);
        }

        self.ime_allowed = allowed;
        if allowed {
            self.apply_ime_cursor_area();
            Ok(None)
        } else {
            self.wm_char_decoder = text::WmCharDecoder::default();
            Ok(self.finish_ime_composition(None))
        }
    }

    /// Return whether this reducer accepts IMM32 text-input messages.
    pub const fn is_ime_allowed(&self) -> bool {
        self.ime_allowed
    }

    /// Set the client-space text-cursor area used to position IMM32 UI.
    ///
    /// `position` is the upper-left corner of the text cursor and `size` is its extent. Both may
    /// use physical or logical pixels; logical values are converted with the reducer's current
    /// scale factor. The area is retained and reapplied after scale-factor changes or when IME
    /// input is enabled.
    ///
    /// Call this whenever the active editor's insertion point moves. The application continues to
    /// render composition text; Windows renders candidate and other IME-owned UI near this area.
    pub fn set_ime_cursor_area(&mut self, position: impl Into<Position>, size: impl Into<Size>) {
        self.ime_cursor_area = Some((position.into(), size.into()));
        self.apply_ime_cursor_area();
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
                | WM_IME_SETCONTEXT
                | WM_CHAR
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
            // A host that has not opted into WM_TOUCH still needs Windows'
            // promoted mouse events as its touch fallback.
            if pointer::is_promoted_touch(extra_info)
                // SAFETY: `self.hwnd` is this reducer's live window; flags are optional.
                && unsafe { IsTouchWindow(self.hwnd, core::ptr::null_mut()) } != 0
            {
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
                self.ime_active,
            ))],

            WM_KEYUP | WM_SYSKEYUP => vec![InputEvent::Keyboard(keyboard::from_win32(
                wparam,
                lparam,
                KeyState::Up,
                self.ime_active,
            ))],
            WM_CHAR => text::from_wm_char(&mut self.wm_char_decoder, wparam as u16)
                .map(|event| InputEvent::Text(vec![event]))
                .into_iter()
                .collect(),
            WM_IME_SETCONTEXT => Vec::new(),
            WM_IME_STARTCOMPOSITION if self.ime_allowed => {
                self.ime_active = true;
                self.apply_ime_cursor_area();
                Vec::new()
            }
            WM_IME_ENDCOMPOSITION => {
                let late_commit = if self.ime_preediting {
                    // SAFETY: `self.hwnd` is the window that received this message.
                    unsafe { text::composed_text(self.hwnd) }.filter(|text| !text.is_empty())
                } else {
                    None
                };
                self.finish_ime_composition(late_commit)
                    .into_iter()
                    .collect()
            }
            WM_IME_COMPOSITION if self.ime_allowed => {
                // SAFETY: `hwnd` is the window that received this message.
                let events = unsafe { text::from_imm(self.hwnd, lparam) };
                events
                    .map(|events| InputEvent::Text(self.normalize_ime_events(events)))
                    .into_iter()
                    .collect()
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
                wheel_delta(
                    WheelAxis::Vertical,
                    notches,
                    wheel_preference(SPI_GETWHEELSCROLLLINES),
                )
                .map(|delta| {
                    InputEvent::Pointer(PointerEvent::Scroll(PointerScrollEvent {
                        pointer: PRIMARY_MOUSE,
                        delta,
                        state: self.primary_state.clone(),
                    }))
                })
                .into_iter()
                .collect()
            }
            WM_MOUSEHWHEEL => {
                self.update_wheel_position(lparam);
                let notches = ((wparam >> 16) as i16) as f32 / WHEEL_DELTA as f32;
                wheel_delta(
                    WheelAxis::Horizontal,
                    notches,
                    wheel_preference(SPI_GETWHEELSCROLLCHARS),
                )
                .map(|delta| {
                    InputEvent::Pointer(PointerEvent::Scroll(PointerScrollEvent {
                        pointer: PRIMARY_MOUSE,
                        delta,
                        state: self.primary_state.clone(),
                    }))
                })
                .into_iter()
                .collect()
            }
            WM_CAPTURECHANGED => self.handle_capture_changed().into_iter().collect(),
            WM_TOUCH => self.handle_touch(wparam, lparam, time),
            _ => Vec::new(),
        };

        Reduction {
            events,
            response: response_for_message(msg, wparam, lparam, self.ime_allowed),
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

    fn apply_ime_cursor_area(&self) {
        if !self.ime_allowed {
            return;
        }
        let Some((position, size)) = self.ime_cursor_area else {
            return;
        };
        // SAFETY: `self.hwnd` remains valid by the invariant of `Self::new`.
        unsafe { text::set_ime_cursor_area(self.hwnd, position, size, self.scale_factor) };
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

    fn finish_ime_composition(&mut self, late_commit: Option<String>) -> Option<InputEvent> {
        self.ime_active = false;
        let had_preedit = mem::take(&mut self.ime_preediting);
        let mut events =
            Vec::with_capacity(usize::from(had_preedit) + usize::from(late_commit.is_some()));
        if had_preedit {
            events.push(TextInputEvent::CompositionEnd);
        }
        if let Some(text) = late_commit {
            events.push(TextInputEvent::Insert(TextInsertEvent::new(text)));
        }
        (!events.is_empty()).then_some(InputEvent::Text(events))
    }

    fn normalize_ime_events(&mut self, events: Vec<TextInputEvent>) -> Vec<TextInputEvent> {
        let has_insert = events
            .iter()
            .any(|event| matches!(event, TextInputEvent::Insert(_)));
        let has_update = events
            .iter()
            .any(|event| matches!(event, TextInputEvent::CompositionUpdate(_)));
        let has_explicit_end = events
            .iter()
            .any(|event| matches!(event, TextInputEvent::CompositionEnd));
        let should_end_before_commit = (self.ime_preediting && has_insert) || has_explicit_end;

        let mut normalized =
            Vec::with_capacity(events.len() + usize::from(should_end_before_commit));
        if should_end_before_commit {
            normalized.push(TextInputEvent::CompositionEnd);
        }
        normalized.extend(
            events
                .into_iter()
                .filter(|event| !matches!(event, TextInputEvent::CompositionEnd)),
        );
        if has_update {
            self.ime_active = true;
            self.ime_preediting = true;
        } else if has_insert || has_explicit_end {
            self.ime_active = false;
            self.ime_preediting = false;
        }
        normalized
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
                    pointer_type: event.pointer.pointer_type,
                    button: event.button,
                    up_time: None,
                    count,
                    x: position.x,
                    y: position.y,
                };

                if let Some(tap) = self.counter.iter_mut().find(|tap| {
                    tap.pointer_type == event.pointer.pointer_type
                        && tap.button == event.button
                        && tap.is_in_range(position, slop)
                        && tap.is_valid_for(time)
                }) {
                    *tap = make(tap.count.saturating_add(1));
                    event.state.count = tap.count;
                } else {
                    let state = make(1);
                    if let Some(tap) = self
                        .counter
                        .iter_mut()
                        .find(|tap| tap.pointer_is(pointer_id) && tap.button == event.button)
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
                if let Some(tap) = self.counter.iter_mut().find(|tap| {
                    tap.pointer_is(event.pointer.pointer_id) && tap.button == event.button
                }) {
                    tap.up_time = Some(event.state.time);
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

/// Failure to update a window's IMM32 input-context association.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImeAssociationError;

impl fmt::Display for ImeAssociationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Windows could not update the IMM32 input-context association")
    }
}

impl Error for ImeAssociationError {}

#[cfg(test)]
mod tests;
