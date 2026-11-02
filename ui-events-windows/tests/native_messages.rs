// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Exercise the reducer through a real, hidden Win32 window procedure.

#![cfg(windows)]
#![expect(
    unsafe_code,
    reason = "Tests own hidden Win32 windows and dispatch native messages"
)]

use std::{cell::RefCell, ptr::null_mut, sync::Once, time::Instant};

use ui_events::{pointer::PointerEvent, text::TextInputEvent};
use ui_events_windows::{InputEvent, MessageResponse, WindowMessageReducer};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::Ime::{ISC_SHOWUICANDIDATEWINDOW, ISC_SHOWUICOMPOSITIONWINDOW};
use windows_sys::Win32::UI::Input::Touch::{RegisterTouchWindow, UnregisterTouchWindow};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GWLP_USERDATA, GetWindowLongPtrW,
    RegisterClassW, SendMessageW, SetMessageExtraInfo, SetWindowLongPtrW, WM_CAPTURECHANGED,
    WM_CHAR, WM_IME_COMPOSITION, WM_IME_SETCONTEXT, WM_LBUTTONDOWN, WM_MOUSEMOVE, WNDCLASSW,
    WS_OVERLAPPEDWINDOW,
};

struct State {
    reducer: Option<WindowMessageReducer>,
    start: Instant,
    events: Vec<InputEvent>,
    response: Option<MessageResponse>,
}

struct TestWindow {
    hwnd: HWND,
    state: Box<RefCell<State>>,
}

impl TestWindow {
    fn new() -> Self {
        let class: Vec<u16> = "UiEventsNativeTests\0".encode_utf16().collect();
        // SAFETY: A null name queries this process's module.
        let instance = unsafe { GetModuleHandleW(core::ptr::null()) };
        assert!(
            !instance.is_null(),
            "the test process must have a module handle"
        );
        static REGISTER: Once = Once::new();
        REGISTER.call_once(|| {
            let descriptor = WNDCLASSW {
                lpfnWndProc: Some(wndproc),
                hInstance: instance,
                lpszClassName: class.as_ptr(),
                ..Default::default()
            };
            // SAFETY: The descriptor and class name live through registration.
            assert_ne!(
                unsafe { RegisterClassW(&descriptor) },
                0,
                "the native test window class must register"
            );
        });
        // SAFETY: The registered class uses our procedure; the window stays hidden.
        let hwnd = unsafe {
            CreateWindowExW(
                0,
                class.as_ptr(),
                core::ptr::null(),
                WS_OVERLAPPEDWINDOW,
                0,
                0,
                100,
                100,
                null_mut(),
                null_mut(),
                instance,
                core::ptr::null(),
            )
        };
        assert!(
            !hwnd.is_null(),
            "the hidden native test window must be created"
        );
        let state = Box::new(RefCell::new(State {
            // SAFETY: This test owns the live window and runs on its owning thread.
            reducer: Some(unsafe { WindowMessageReducer::new(hwnd, 1.0) }),
            start: Instant::now(),
            events: Vec::new(),
            response: None,
        }));
        // SAFETY: The boxed state stays stable until Drop clears the slot.
        unsafe {
            SetWindowLongPtrW(
                hwnd,
                GWLP_USERDATA,
                (&*state as *const RefCell<State>) as isize,
            )
        };
        Self { hwnd, state }
    }

    fn send(&self, msg: u32, wparam: WPARAM, lparam: LPARAM) {
        // SAFETY: This thread owns the window and supplies the message's parameters.
        unsafe { SendMessageW(self.hwnd, msg, wparam, lparam) };
    }

    fn drain(&self) -> Vec<InputEvent> {
        core::mem::take(&mut self.state.borrow_mut().events)
    }
}

impl Drop for TestWindow {
    fn drop(&mut self) {
        // SAFETY: Clear the borrowed state before destroying the window on its owning thread.
        unsafe {
            SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, 0);
            self.state.borrow_mut().reducer.take();
            DestroyWindow(self.hwnd);
        }
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // SAFETY: Only TestWindow installs this procedure and its boxed state pointer.
    let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const RefCell<State>;
    if ptr.is_null() || !WindowMessageReducer::handles_message(msg) {
        // SAFETY: Forward the window procedure's original parameters.
        return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
    }
    // SAFETY: TestWindow clears this pointer before freeing the state.
    let state = unsafe { &*ptr };
    let reducer = state.borrow_mut().reducer.take();
    let Some(mut reducer) = reducer else {
        // SAFETY: Nested calls while the reducer is borrowed use native defaults.
        return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
    };
    let time = u64::try_from(state.borrow().start.elapsed().as_nanos()).unwrap_or(u64::MAX);
    // SAFETY: These parameters came directly from this window's procedure, on its thread.
    let reduction = unsafe { reducer.reduce(msg, wparam, lparam, time) };
    let response = reduction.response;
    {
        let mut state = state.borrow_mut();
        state.events.extend(reduction.events);
        state.response = Some(response);
        state.reducer = Some(reducer);
    }
    match response {
        // SAFETY: Forward native parameters after releasing every state borrow.
        MessageResponse::Forward => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
        MessageResponse::ForwardWithParameters { wparam, lparam } => unsafe {
            DefWindowProcW(hwnd, msg, wparam, lparam)
        },
        MessageResponse::Consume(result) | MessageResponse::ConsumeIfHandled(result) => result,
    }
}

#[test]
fn mouse_capture_loss_reaches_the_normalized_event_stream() {
    let window = TestWindow::new();
    window.send(WM_LBUTTONDOWN, 1, 20 << 16 | 10);
    window.send(WM_CAPTURECHANGED, 0, 0);
    let events = window.drain();
    assert!(
        matches!(events.as_slice(), [InputEvent::Pointer(PointerEvent::Down(down)), InputEvent::Pointer(PointerEvent::Cancel(_))]
        if down.state.position.x == 10.0 && down.state.position.y == 20.0)
    );
}

#[test]
fn unicode_and_ime_messages_follow_the_native_response_contract() {
    let window = TestWindow::new();
    window.send(WM_CHAR, 0xd83d, 0);
    window.send(WM_CHAR, 0xde42, 0);
    let events = window.drain();
    assert!(matches!(events.as_slice(), [InputEvent::Text(text)]
        if matches!(text.as_slice(), [TextInputEvent::Insert(insert)] if insert.text == "🙂")));
    let flags = ISC_SHOWUICOMPOSITIONWINDOW | ISC_SHOWUICANDIDATEWINDOW;
    window.send(WM_IME_SETCONTEXT, 0, flags as LPARAM);
    assert_eq!(
        window.state.borrow().response,
        Some(MessageResponse::ForwardWithParameters {
            wparam: 0,
            lparam: ISC_SHOWUICANDIDATEWINDOW as LPARAM,
        })
    );
    window.send(WM_IME_COMPOSITION, 0, 0);
    assert!(
        matches!(window.drain().as_slice(), [InputEvent::Text(events)]
        if events == &[TextInputEvent::CompositionEnd])
    );
    let mut state = window.state.borrow_mut();
    let reducer = state.reducer.as_mut().unwrap();
    assert_eq!(reducer.hwnd(), window.hwnd);
    reducer.set_scale_factor(2.0);
    assert_eq!(reducer.scale_factor(), 2.0);
    assert!(reducer.set_ime_allowed(false).is_ok());
    assert!(!reducer.is_ime_allowed());
    assert!(reducer.set_ime_allowed(true).is_ok());
}

#[test]
fn promoted_touch_retains_mouse_fallback_until_the_host_registers_touch() {
    let window = TestWindow::new();
    // SAFETY: This changes only the current test thread's message metadata.
    let previous = unsafe { SetMessageExtraInfo(0xff51_5780_u32 as LPARAM) };
    window.send(WM_MOUSEMOVE, 0, 0);
    let fallback = window.drain();
    // SAFETY: This test owns the live window and its touch registration.
    let registered = unsafe { RegisterTouchWindow(window.hwnd, 0) };
    window.send(WM_MOUSEMOVE, 0, 0);
    let duplicate = window.drain();
    // SAFETY: Restore registration and metadata before assertions can unwind.
    unsafe {
        UnregisterTouchWindow(window.hwnd);
        SetMessageExtraInfo(previous);
    }
    assert_ne!(registered, 0);
    assert!(matches!(
        fallback.last(),
        Some(InputEvent::Pointer(PointerEvent::Move(_)))
    ));
    assert!(duplicate.is_empty());
}
