// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Reusable `AppKit` responder for pointer, scroll, gesture, tablet, and
//! keyboard input.

use alloc::boxed::Box;

use objc2::rc::Retained;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{NSEvent, NSResponder};
use ui_events::keyboard::KeyboardEvent;
use ui_events::pointer::PointerEvent;
use ui_events_apple_common::EventDisposition;

use crate::{keyboard_event_from_nsevent, pointer_event_from_nsevent_at_position};

/// Host-side event sink for [`AppKitInputResponder`].
pub trait AppKitInputResponderHost {
    /// Handle a translated `AppKit` keyboard event and report whether native
    /// responder routing should stop.
    fn handle_keyboard_event(&self, event: KeyboardEvent) -> EventDisposition;

    /// Handle a translated `AppKit` pointer, scroll, gesture, or tablet event
    /// and report whether native responder routing should stop.
    fn handle_pointer_event(&self, event: PointerEvent) -> EventDisposition;

    /// Return the pointer position for `event` in the host coordinate space,
    /// measured in logical AppKit points.
    ///
    /// `NSEvent::locationInWindow` is window-relative. Hosts backed by an
    /// `NSView` should usually convert that point with
    /// `view.convertPoint_fromView(event.locationInWindow(), None)` before
    /// returning it here.
    fn pointer_position(&self, event: &NSEvent) -> (f64, f64);

    /// Return the scale factor used when converting `AppKit` pointer positions
    /// from logical points into physical pixels.
    fn pointer_scale_factor(&self) -> f64;
}

#[doc(hidden)]
pub struct InputResponderState {
    host: Box<dyn AppKitInputResponderHost>,
}

impl core::fmt::Debug for InputResponderState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("InputResponderState")
            .field("host", &"<dyn AppKitInputResponderHost>")
            .finish()
    }
}

define_class!(
    #[unsafe(super = NSResponder)]
    #[thread_kind = objc2::MainThreadOnly]
    #[name = "UIEventsAppKitInputResponder"]
    #[ivars = InputResponderState]
    #[doc = "Reusable `AppKit` responder for pointer, scroll, gesture, tablet, and keyboard input."]
    pub struct AppKitInputResponder;

    impl AppKitInputResponder {
        #[unsafe(method(acceptsFirstResponder))]
        fn accepts_first_responder(&self) -> bool {
            true
        }

        #[unsafe(method(keyDown:))]
        fn key_down(&self, event: &NSEvent) {
            if !self.handle_keyboard_nsevent(event).is_handled() {
                // SAFETY: This invokes the overridden method on `NSResponder`.
                unsafe { msg_send![super(self), keyDown: event] }
            }
        }

        #[unsafe(method(keyUp:))]
        fn key_up(&self, event: &NSEvent) {
            if !self.handle_keyboard_nsevent(event).is_handled() {
                // SAFETY: This invokes the overridden method on `NSResponder`.
                unsafe { msg_send![super(self), keyUp: event] }
            }
        }

        #[unsafe(method(flagsChanged:))]
        fn flags_changed(&self, event: &NSEvent) {
            if !self.handle_keyboard_nsevent(event).is_handled() {
                // SAFETY: This invokes the overridden method on `NSResponder`.
                unsafe { msg_send![super(self), flagsChanged: event] }
            }
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            if !self.handle_pointer_nsevent(event).is_handled() {
                // SAFETY: This invokes the overridden method on `NSResponder`.
                unsafe { msg_send![super(self), mouseDown: event] }
            }
        }

        #[unsafe(method(mouseDragged:))]
        fn mouse_dragged(&self, event: &NSEvent) {
            if !self.handle_pointer_nsevent(event).is_handled() {
                // SAFETY: This invokes the overridden method on `NSResponder`.
                unsafe { msg_send![super(self), mouseDragged: event] }
            }
        }

        #[unsafe(method(mouseMoved:))]
        fn mouse_moved(&self, event: &NSEvent) {
            if !self.handle_pointer_nsevent(event).is_handled() {
                // SAFETY: This invokes the overridden method on `NSResponder`.
                unsafe { msg_send![super(self), mouseMoved: event] }
            }
        }

        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, event: &NSEvent) {
            if !self.handle_pointer_nsevent(event).is_handled() {
                // SAFETY: This invokes the overridden method on `NSResponder`.
                unsafe { msg_send![super(self), mouseUp: event] }
            }
        }

        #[unsafe(method(rightMouseDown:))]
        fn right_mouse_down(&self, event: &NSEvent) {
            if !self.handle_pointer_nsevent(event).is_handled() {
                // SAFETY: This invokes the overridden method on `NSResponder`.
                unsafe { msg_send![super(self), rightMouseDown: event] }
            }
        }

        #[unsafe(method(rightMouseDragged:))]
        fn right_mouse_dragged(&self, event: &NSEvent) {
            if !self.handle_pointer_nsevent(event).is_handled() {
                // SAFETY: This invokes the overridden method on `NSResponder`.
                unsafe { msg_send![super(self), rightMouseDragged: event] }
            }
        }

        #[unsafe(method(rightMouseUp:))]
        fn right_mouse_up(&self, event: &NSEvent) {
            if !self.handle_pointer_nsevent(event).is_handled() {
                // SAFETY: This invokes the overridden method on `NSResponder`.
                unsafe { msg_send![super(self), rightMouseUp: event] }
            }
        }

        #[unsafe(method(otherMouseDown:))]
        fn other_mouse_down(&self, event: &NSEvent) {
            if !self.handle_pointer_nsevent(event).is_handled() {
                // SAFETY: This invokes the overridden method on `NSResponder`.
                unsafe { msg_send![super(self), otherMouseDown: event] }
            }
        }

        #[unsafe(method(otherMouseDragged:))]
        fn other_mouse_dragged(&self, event: &NSEvent) {
            if !self.handle_pointer_nsevent(event).is_handled() {
                // SAFETY: This invokes the overridden method on `NSResponder`.
                unsafe { msg_send![super(self), otherMouseDragged: event] }
            }
        }

        #[unsafe(method(otherMouseUp:))]
        fn other_mouse_up(&self, event: &NSEvent) {
            if !self.handle_pointer_nsevent(event).is_handled() {
                // SAFETY: This invokes the overridden method on `NSResponder`.
                unsafe { msg_send![super(self), otherMouseUp: event] }
            }
        }

        #[unsafe(method(scrollWheel:))]
        fn scroll_wheel(&self, event: &NSEvent) {
            if !self.handle_pointer_nsevent(event).is_handled() {
                // SAFETY: This invokes the overridden method on `NSResponder`.
                unsafe { msg_send![super(self), scrollWheel: event] }
            }
        }

        #[unsafe(method(magnifyWithEvent:))]
        fn magnify_with_event(&self, event: &NSEvent) {
            if !self.handle_pointer_nsevent(event).is_handled() {
                // SAFETY: This invokes the overridden method on `NSResponder`.
                unsafe { msg_send![super(self), magnifyWithEvent: event] }
            }
        }

        #[unsafe(method(rotateWithEvent:))]
        fn rotate_with_event(&self, event: &NSEvent) {
            if !self.handle_pointer_nsevent(event).is_handled() {
                // SAFETY: This invokes the overridden method on `NSResponder`.
                unsafe { msg_send![super(self), rotateWithEvent: event] }
            }
        }

        #[unsafe(method(tabletPoint:))]
        fn tablet_point(&self, event: &NSEvent) {
            if !self.handle_pointer_nsevent(event).is_handled() {
                // SAFETY: This invokes the overridden method on `NSResponder`.
                unsafe { msg_send![super(self), tabletPoint: event] }
            }
        }

        #[unsafe(method(tabletProximity:))]
        fn tablet_proximity(&self, event: &NSEvent) {
            if !self.handle_pointer_nsevent(event).is_handled() {
                // SAFETY: This invokes the overridden method on `NSResponder`.
                unsafe { msg_send![super(self), tabletProximity: event] }
            }
        }

        #[unsafe(method(mouseEntered:))]
        fn mouse_entered(&self, event: &NSEvent) {
            if !self.handle_pointer_nsevent(event).is_handled() {
                // SAFETY: This invokes the overridden method on `NSResponder`.
                unsafe { msg_send![super(self), mouseEntered: event] }
            }
        }

        #[unsafe(method(mouseExited:))]
        fn mouse_exited(&self, event: &NSEvent) {
            if !self.handle_pointer_nsevent(event).is_handled() {
                // SAFETY: This invokes the overridden method on `NSResponder`.
                unsafe { msg_send![super(self), mouseExited: event] }
            }
        }
    }
);

impl AppKitInputResponder {
    /// Create a reusable `AppKit` responder for pointer, scroll, gesture,
    /// tablet, and keyboard input.
    pub fn new(mtm: MainThreadMarker, host: Box<dyn AppKitInputResponderHost>) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(InputResponderState { host });
        // SAFETY: `NSResponder` has no additional initialization requirements.
        unsafe { msg_send![super(this), init] }
    }

    /// Translate an `AppKit` keyboard `NSEvent`, forward it to the host, and
    /// return the host's disposition.
    ///
    /// Events this adapter cannot translate are [`EventDisposition::Unhandled`].
    pub fn handle_keyboard_nsevent(&self, event: &NSEvent) -> EventDisposition {
        if let Some(event) = keyboard_event_from_nsevent(event) {
            self.ivars().host.handle_keyboard_event(event)
        } else {
            EventDisposition::Unhandled
        }
    }

    /// Translate an `AppKit` pointer-related `NSEvent`, forward it to the host,
    /// and return the host's disposition.
    ///
    /// Events this adapter cannot translate are [`EventDisposition::Unhandled`].
    pub fn handle_pointer_nsevent(&self, event: &NSEvent) -> EventDisposition {
        let (x, y) = self.ivars().host.pointer_position(event);
        if let Some(event) = pointer_event_from_nsevent_at_position(
            event,
            self.ivars().host.pointer_scale_factor(),
            x,
            y,
        ) {
            self.ivars().host.handle_pointer_event(event)
        } else {
            EventDisposition::Unhandled
        }
    }

    /// Set the responder that should receive events this responder does not handle.
    ///
    /// # Safety
    ///
    /// AppKit stores `next_responder` unretained. The caller must ensure that
    /// the responder remains alive while AppKit can traverse this responder
    /// chain.
    pub unsafe fn set_following_responder(&self, next_responder: Option<&NSResponder>) {
        unsafe { self.setNextResponder(next_responder) };
    }
}
