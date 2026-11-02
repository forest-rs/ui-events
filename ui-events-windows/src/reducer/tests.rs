// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use super::message::ime_context_lparam;
use super::*;
use dpi::{LogicalPosition, LogicalSize, Position, Size};
use ui_events::ScrollDelta;
use ui_events::pointer::PointerButton;
use ui_events::text::CompositionState;
use windows_sys::Win32::UI::Input::Ime::{ISC_SHOWUICANDIDATEWINDOW, ISC_SHOWUICOMPOSITIONWINDOW};
use windows_sys::Win32::UI::WindowsAndMessaging::WM_APP;

fn test_reducer() -> WindowMessageReducer {
    WindowMessageReducer {
        hwnd: core::ptr::null_mut(),
        scale_factor: 1.0,
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

fn button_event(
    pointer: PointerInfo,
    button: Option<PointerButton>,
    time: u64,
) -> PointerButtonEvent {
    PointerButtonEvent {
        pointer,
        button,
        state: PointerState {
            time,
            ..Default::default()
        },
    }
}

#[test]
fn simultaneous_contacts_do_not_steal_each_others_tap_state() {
    let mut reducer = test_reducer();
    let first = PointerInfo {
        pointer_id: pointer::touch_pointer_id(0),
        pointer_type: PointerType::Touch,
        persistent_device_id: None,
    };
    let second = PointerInfo {
        pointer_id: pointer::touch_pointer_id(1),
        ..first
    };
    for contact in [first, second] {
        let down = reducer.attach_count(PointerEvent::Down(button_event(contact, None, 0)));
        assert!(matches!(down, PointerEvent::Down(event) if event.state.count == 1));
    }
    assert_eq!(reducer.counter.len(), 2);
    for contact in [first, second] {
        let up = reducer.attach_count(PointerEvent::Up(button_event(contact, None, 1)));
        assert!(matches!(up, PointerEvent::Up(event) if event.state.count == 1));
    }
}

#[test]
fn zero_duration_clicks_repeat_and_saturate_without_overflow() {
    let mut reducer = test_reducer();
    for i in 0..300_u64 {
        let down = reducer.attach_count(PointerEvent::Down(button_event(
            PRIMARY_MOUSE,
            Some(PointerButton::Primary),
            i,
        )));
        let expected = u8::try_from(i + 1).unwrap_or(u8::MAX);
        assert!(matches!(down, PointerEvent::Down(event) if event.state.count == expected));
        reducer.attach_count(PointerEvent::Up(button_event(
            PRIMARY_MOUSE,
            Some(PointerButton::Primary),
            i,
        )));
    }
}

#[test]
fn repetition_does_not_cross_mouse_buttons_or_device_kinds() {
    let mut reducer = test_reducer();
    for button in [PointerButton::Primary, PointerButton::Secondary] {
        let down = reducer.attach_count(PointerEvent::Down(button_event(
            PRIMARY_MOUSE,
            Some(button),
            0,
        )));
        assert!(matches!(down, PointerEvent::Down(event) if event.state.count == 1));
        reducer.attach_count(PointerEvent::Up(button_event(
            PRIMARY_MOUSE,
            Some(button),
            1,
        )));
    }
    let touch = PointerInfo {
        pointer_id: pointer::touch_pointer_id(0),
        pointer_type: PointerType::Touch,
        persistent_device_id: None,
    };
    let down = reducer.attach_count(PointerEvent::Down(button_event(touch, None, 2)));
    assert!(matches!(down, PointerEvent::Down(event) if event.state.count == 1));
}

#[test]
fn message_disposition_preserves_win32_return_contracts() {
    let response = |msg| response_for_message(msg, 0, 0, true);
    assert_eq!(response(WM_TOUCH), MessageResponse::Consume(0));
    assert_eq!(
        response(WM_XBUTTONDOWN),
        MessageResponse::ConsumeIfHandled(1)
    );
    assert_eq!(response(WM_XBUTTONUP), MessageResponse::ConsumeIfHandled(1));
    assert_eq!(response(WM_MOUSEMOVE), MessageResponse::ConsumeIfHandled(0));
    assert_eq!(response(WM_APP), MessageResponse::Forward);
}

#[test]
fn recognized_message_table_matches_disposition_table() {
    assert!(WindowMessageReducer::handles_message(WM_TOUCH));
    assert!(WindowMessageReducer::handles_message(WM_SYSKEYDOWN));
    assert!(WindowMessageReducer::handles_message(WM_CHAR));
    assert!(WindowMessageReducer::handles_message(WM_IME_SETCONTEXT));
    assert!(WindowMessageReducer::handles_message(WM_MOUSELEAVE));
    assert!(WindowMessageReducer::handles_message(WM_CAPTURECHANGED));
    assert!(!WindowMessageReducer::handles_message(WM_APP));
}

#[test]
fn ime_context_forwarding_hides_only_application_rendered_preedit() {
    let flags = ISC_SHOWUICOMPOSITIONWINDOW | ISC_SHOWUICANDIDATEWINDOW;
    let filtered = ISC_SHOWUICANDIDATEWINDOW as LPARAM;

    assert_eq!(ime_context_lparam(flags as LPARAM), filtered);
    assert_eq!(
        response_for_message(WM_IME_SETCONTEXT, 1, flags as LPARAM, true),
        MessageResponse::ForwardWithParameters {
            wparam: 1,
            lparam: filtered,
        }
    );
    assert_eq!(
        response_for_message(WM_IME_SETCONTEXT, 1, flags as LPARAM, false),
        MessageResponse::Forward
    );
}

#[test]
fn logical_ime_cursor_area_scales_to_client_pixels() {
    let (composition, candidate) = text::ime_forms(
        Position::from(LogicalPosition::new(10.0, 20.0)),
        Size::from(LogicalSize::new(2.0, 12.0)),
        2.0,
    );

    assert_eq!(
        (composition.ptCurrentPos.x, composition.ptCurrentPos.y),
        (20, 64)
    );
    assert_eq!(
        (candidate.ptCurrentPos.x, candidate.ptCurrentPos.y),
        (20, 40)
    );
    assert_eq!(
        (
            candidate.rcArea.left,
            candidate.rcArea.top,
            candidate.rcArea.right,
            candidate.rcArea.bottom,
        ),
        (20, 40, 24, 64)
    );
}

#[test]
fn cancel_removes_only_the_cancelled_pointers_tap_state() {
    let mut reducer = test_reducer();
    let other_pointer = pointer::touch_pointer_id(0);
    reducer.counter = vec![
        pointer::TapState {
            pointer_id: PRIMARY_MOUSE.pointer_id,
            pointer_type: PointerType::Mouse,
            button: Some(PointerButton::Primary),
            up_time: None,
            count: 1,
            x: 0.0,
            y: 0.0,
        },
        pointer::TapState {
            pointer_id: other_pointer,
            pointer_type: PointerType::Touch,
            button: None,
            up_time: None,
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

#[test]
fn wheel_deltas_use_navigation_direction() {
    assert_eq!(
        wheel_delta(WheelAxis::Vertical, 1.0, 3),
        Some(ScrollDelta::LineDelta(0.0, -3.0))
    );
    assert_eq!(
        wheel_delta(WheelAxis::Horizontal, 1.0, 3),
        Some(ScrollDelta::LineDelta(3.0, 0.0))
    );
}

#[test]
fn page_scroll_preference_stays_a_page_delta() {
    assert_eq!(
        wheel_delta(WheelAxis::Vertical, -1.0, u32::MAX),
        Some(ScrollDelta::PageDelta(0.0, 1.0))
    );
}

#[test]
fn disabled_wheel_preference_produces_no_scroll() {
    assert_eq!(wheel_delta(WheelAxis::Vertical, 1.0, 0), None);
}

#[test]
fn combined_ime_commit_and_update_preserve_both_sides() {
    let mut reducer = test_reducer();
    reducer.ime_active = true;
    reducer.ime_preediting = true;
    let events = reducer.normalize_ime_events(vec![
        TextInputEvent::Insert(TextInsertEvent::new("に")),
        TextInputEvent::CompositionUpdate(CompositionState::new("ほ")),
    ]);

    assert!(matches!(
        events.as_slice(),
        [
            TextInputEvent::CompositionEnd,
            TextInputEvent::Insert(insert),
            TextInputEvent::CompositionUpdate(update),
        ] if insert.text == "に" && update.text == "ほ"
    ));
    assert!(reducer.ime_active);
    assert!(reducer.ime_preediting);
}

#[test]
fn ime_commit_ends_preedit_before_inserting_text() {
    let mut reducer = test_reducer();
    reducer.ime_active = true;
    reducer.ime_preediting = true;

    let events =
        reducer.normalize_ime_events(vec![TextInputEvent::Insert(TextInsertEvent::new("日"))]);

    assert!(matches!(
        events.as_slice(),
        [
            TextInputEvent::CompositionEnd,
            TextInputEvent::Insert(insert),
        ] if insert.text == "日"
    ));
    assert!(!reducer.ime_active);
    assert!(!reducer.ime_preediting);
}

#[test]
fn ime_end_preserves_a_late_hangul_commit() {
    let mut reducer = test_reducer();
    reducer.ime_active = true;
    reducer.ime_preediting = true;

    let event = reducer.finish_ime_composition(Some("한".to_owned()));

    assert!(matches!(
        event,
        Some(InputEvent::Text(events))
            if matches!(
                events.as_slice(),
                [
                    TextInputEvent::CompositionEnd,
                    TextInputEvent::Insert(insert),
                ] if insert.text == "한"
            )
    ));
    assert!(!reducer.ime_active);
    assert!(!reducer.ime_preediting);
}

#[test]
fn ime_session_without_preedit_does_not_emit_composition_end() {
    let mut reducer = test_reducer();
    reducer.ime_active = true;

    assert!(reducer.finish_ime_composition(None).is_none());
    assert!(!reducer.ime_active);
}
