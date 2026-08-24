// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Reusable AppKit responder for keyboard, IME, and edit-command callbacks.

use alloc::boxed::Box;
use core::cell::OnceCell;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject, Sel};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{NSEvent, NSResponder, NSTextInputClient, NSTextInputContext};
use objc2_foundation::{
    NSArray, NSAttributedString, NSAttributedStringKey, NSPoint, NSRange, NSRangePointer, NSRect,
};
use ui_events::edit::EditCommandEvent;
use ui_events::keyboard::KeyboardEvent;
use ui_events::text::TextInputEvent;
use ui_events_apple_common::EventDisposition;
use ui_text_input::{
    TextGeometryProvider, TextHitTestProvider, TextInputHost, TextInputRect, TextRangeConverter,
    TextRangeProvider,
};

use crate::appkit::{
    composition_end_event, composition_update_event_from_nsobject_and_ranges,
    edit_command_event_from_selector, insert_text_event_from_nsobject,
    insert_text_event_from_nsobject_and_replacement_range, keyboard_event_from_nsevent,
};
use crate::text_host::{
    attributed_substring_for_proposed_range_from_host, character_index_for_point_from_host,
    first_rect_for_character_range_from_host, has_marked_text_from_host, marked_range_from_host,
    selected_range_from_host,
};

/// Host-side contract for [`AppKitTextInputResponder`].
///
/// This extends the shared text-query traits with sinks for keyboard, text, and
/// edit-command events, plus AppKit-specific coordinate conversions for IME
/// geometry and hit-testing.
///
/// Text and edit callbacks are authoritative for mutations. Raw
/// [`KeyboardEvent`] values are still useful for key state and shortcuts, but
/// hosts should not also insert their produced text.
pub trait AppKitTextInputResponderHost:
    TextInputHost + TextRangeProvider + TextRangeConverter + TextGeometryProvider + TextHitTestProvider
{
    /// Handle a translated AppKit keyboard event and report whether native
    /// responder routing should stop.
    fn handle_keyboard_event(&self, event: KeyboardEvent) -> EventDisposition;

    /// Handle a translated AppKit text-input event.
    fn handle_text_input_event(&self, event: TextInputEvent);

    /// Handle a translated AppKit semantic edit command and report whether
    /// native responder routing should stop.
    fn handle_edit_command_event(&self, command: EditCommandEvent) -> EventDisposition;

    /// Convert a host-local IME rectangle into AppKit screen coordinates.
    fn text_input_rect_to_screen_rect(&self, rect: TextInputRect) -> NSRect;

    /// Convert an AppKit screen point into the host coordinate space used by
    /// [`TextHitTestProvider`].
    fn screen_point_to_host(&self, point: NSPoint) -> dpi::PhysicalPosition<f64>;
}

#[doc(hidden)]
pub struct TextInputResponderState {
    host: Box<dyn AppKitTextInputResponderHost>,
    input_context: OnceCell<Retained<NSTextInputContext>>,
}

impl core::fmt::Debug for TextInputResponderState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TextInputResponderState")
            .field("host", &"<dyn AppKitTextInputResponderHost>")
            .field("input_context", &self.input_context.get().is_some())
            .finish()
    }
}

define_class!(
    #[unsafe(super = NSResponder)]
    #[thread_kind = objc2::MainThreadOnly]
    #[name = "UIEventsAppKitTextInputResponder"]
    #[ivars = TextInputResponderState]
    #[doc = "Reusable AppKit responder for keyboard, IME, and edit-command callbacks."]
    pub struct AppKitTextInputResponder;

    impl AppKitTextInputResponder {
        #[unsafe(method(acceptsFirstResponder))]
        fn accepts_first_responder(&self) -> bool {
            true
        }

        #[unsafe(method(becomeFirstResponder))]
        fn become_first_responder(&self) -> bool {
            // SAFETY: This invokes the overridden method on `NSResponder`.
            let became: bool = unsafe { msg_send![super(self), becomeFirstResponder] };
            if became {
                self.with_input_context(|input_context| input_context.activate());
            }
            became
        }

        #[unsafe(method(resignFirstResponder))]
        fn resign_first_responder(&self) -> bool {
            // SAFETY: This invokes the overridden method on `NSResponder`.
            let resigned: bool = unsafe { msg_send![super(self), resignFirstResponder] };
            if resigned {
                self.with_input_context(|input_context| input_context.deactivate());
            }
            resigned
        }

        #[unsafe(method(keyDown:))]
        fn key_down(&self, event: &NSEvent) {
            let ime_handled = self
                .with_input_context(|input_context| input_context.handleEvent(event))
                .unwrap_or(false);
            let host_handled = self.handle_keyboard_nsevent(event).is_handled();
            if !ime_handled && !host_handled {
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

        #[unsafe(method(_wantsKeyDownForEvent:))]
        fn wants_key_down_for_event(&self, _event: &NSEvent) -> bool {
            true
        }

        #[unsafe(method(insertText:))]
        unsafe fn insert_text(&self, text: &AnyObject) {
            if let Some(event) = insert_text_event_from_nsobject(text) {
                self.handle_text_event(event);
            }
        }

        #[unsafe(method(setMarkedText:selectedRange:))]
        unsafe fn set_marked_text_selected_range(&self, text: &AnyObject, selected_range: NSRange) {
            let replacement_range = NSRange::new(usize::MAX, 0);
            if let Some(event) = composition_update_event_from_nsobject_and_ranges(
                text,
                selected_range,
                replacement_range,
            ) {
                self.handle_text_event(event);
            }
        }
    }

    #[allow(
        non_snake_case,
        reason = "objc2 protocol methods must match AppKit selector spellings"
    )]
    unsafe impl NSTextInputClient for AppKitTextInputResponder {
        #[unsafe(method(hasMarkedText))]
        fn hasMarkedText(&self) -> bool {
            has_marked_text_from_host(&*self.ivars().host)
        }

        #[unsafe(method(markedRange))]
        fn markedRange(&self) -> NSRange {
            marked_range_from_host(&*self.ivars().host)
        }

        #[unsafe(method(selectedRange))]
        fn selectedRange(&self) -> NSRange {
            selected_range_from_host(&*self.ivars().host)
        }

        #[unsafe(method(setMarkedText:selectedRange:replacementRange:))]
        unsafe fn setMarkedText_selectedRange_replacementRange(
            &self,
            text: &AnyObject,
            selected_range: NSRange,
            replacement_range: NSRange,
        ) {
            if let Some(event) = composition_update_event_from_nsobject_and_ranges(
                text,
                selected_range,
                replacement_range,
            ) {
                self.handle_text_event(event);
            }
        }

        #[unsafe(method(insertText:replacementRange:))]
        unsafe fn insertText_replacementRange(&self, text: &AnyObject, replacement_range: NSRange) {
            if let Some(event) =
                insert_text_event_from_nsobject_and_replacement_range(text, replacement_range)
            {
                self.handle_text_event(event);
            }
        }

        #[unsafe(method(unmarkText))]
        fn unmarkText(&self) {
            self.handle_text_event(composition_end_event());
        }

        #[unsafe(method(doCommandBySelector:))]
        unsafe fn doCommandBySelector(&self, command: Sel) {
            let handled = edit_command_event_from_selector(command)
                .map(|event| self.ivars().host.handle_edit_command_event(event))
                .is_some_and(EventDisposition::is_handled);
            if handled {
                self.notify_text_state_changed();
            } else {
                // SAFETY: `command` comes from AppKit, and this invokes the
                // overridden method on `NSResponder`.
                unsafe { msg_send![super(self), doCommandBySelector: command] }
            }
        }

        #[unsafe(method_id(validAttributesForMarkedText))]
        fn validAttributesForMarkedText(&self) -> Retained<NSArray<NSAttributedStringKey>> {
            NSArray::new()
        }

        #[unsafe(method_id(attributedSubstringForProposedRange:actualRange:))]
        unsafe fn attributedSubstringForProposedRange_actualRange(
            &self,
            range: NSRange,
            actual_range: NSRangePointer,
        ) -> Option<Retained<NSAttributedString>> {
            // SAFETY: `NSTextInputClient` guarantees a valid mutable pointer or
            // null for this out parameter for the duration of the callback.
            let actual_range = unsafe { actual_range.as_mut() };
            attributed_substring_for_proposed_range_from_host(
                &*self.ivars().host,
                range,
                actual_range,
            )
        }

        #[unsafe(method(firstRectForCharacterRange:actualRange:))]
        unsafe fn firstRectForCharacterRange_actualRange(
            &self,
            range: NSRange,
            actual_range: NSRangePointer,
        ) -> NSRect {
            // SAFETY: `NSTextInputClient` guarantees a valid mutable pointer or
            // null for this out parameter for the duration of the callback.
            let mut actual_range = unsafe { actual_range.as_mut() };
            first_rect_for_character_range_from_host(
                &*self.ivars().host,
                range,
                actual_range.as_deref_mut(),
                |rect| self.ivars().host.text_input_rect_to_screen_rect(rect),
            )
            .unwrap_or_else(|| {
                if let Some(actual_range) = actual_range {
                    *actual_range = selected_range_from_host(&*self.ivars().host);
                }
                self.ivars().host.caret_rect().map_or_else(
                    || {
                        self.ivars()
                            .host
                            .text_input_rect_to_screen_rect(TextInputRect::new(
                                dpi::PhysicalPosition::new(0.0, 0.0),
                                dpi::PhysicalSize::new(1.0, 1.0),
                            ))
                    },
                    |rect| self.ivars().host.text_input_rect_to_screen_rect(rect),
                )
            })
        }

        #[unsafe(method(characterIndexForPoint:))]
        fn characterIndexForPoint(&self, point: NSPoint) -> usize {
            character_index_for_point_from_host(&*self.ivars().host, point, |point| {
                self.ivars().host.screen_point_to_host(point)
            })
        }
    }
);

impl AppKitTextInputResponder {
    /// Create a reusable AppKit responder for keyboard, IME, and edit commands.
    ///
    /// Typical setup:
    ///
    /// 1. Construct this with a boxed [`AppKitTextInputResponderHost`].
    /// 2. Set its following responder to the host view or responder.
    /// 3. Make it the window's first responder.
    ///
    /// The caller must retain the responder for as long as AppKit may deliver
    /// events to it.
    pub fn new(
        mtm: MainThreadMarker,
        host: Box<dyn AppKitTextInputResponderHost>,
    ) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(TextInputResponderState {
            host,
            input_context: OnceCell::new(),
        });
        // SAFETY: `NSResponder` has no additional initialization requirements.
        let this: Retained<Self> = unsafe { msg_send![super(this), init] };
        let client = ProtocolObject::from_ref(&*this);
        let input_context =
            NSTextInputContext::initWithClient(NSTextInputContext::alloc(mtm), client);
        assert!(
            this.ivars().input_context.set(input_context).is_ok(),
            "a text-input responder initializes its context exactly once"
        );
        this
    }

    /// Translate an AppKit keyboard `NSEvent`, forward it to the host, and
    /// return the host's disposition.
    ///
    /// Events this adapter cannot translate are [`EventDisposition::Unhandled`].
    pub fn handle_keyboard_nsevent(&self, event: &NSEvent) -> EventDisposition {
        keyboard_event_from_nsevent(event).map_or(EventDisposition::Unhandled, |event| {
            self.ivars().host.handle_keyboard_event(event)
        })
    }

    /// Ask the input context to abandon its current marked text.
    pub fn discard_marked_text(&self) {
        self.with_input_context(NSTextInputContext::discardMarkedText);
    }

    /// Notify the input context that character or caret geometry changed.
    pub fn invalidate_character_coordinates(&self) {
        self.with_input_context(NSTextInputContext::invalidateCharacterCoordinates);
    }

    /// Notify the input context that the host selection changed.
    pub fn notify_selection_changed(&self) {
        self.with_input_context(NSTextInputContext::textInputClientDidUpdateSelection);
    }

    /// Notify the input context that the host scrolled.
    pub fn notify_scrolled(&self) {
        self.with_input_context(NSTextInputContext::textInputClientDidScroll);
    }

    /// Notify the input context that scrolling or zooming is about to start.
    pub fn notify_will_start_scrolling_or_zooming(&self) {
        self.with_input_context(NSTextInputContext::textInputClientWillStartScrollingOrZooming);
    }

    /// Notify the input context that scrolling or zooming ended.
    pub fn notify_did_end_scrolling_or_zooming(&self) {
        self.with_input_context(NSTextInputContext::textInputClientDidEndScrollingOrZooming);
    }

    /// Set the responder that should receive unhandled messages after this one.
    ///
    /// # Safety
    ///
    /// AppKit stores `next_responder` unretained. The caller must ensure that
    /// the next responder remains alive for as long as this responder may be
    /// asked to traverse the chain.
    pub unsafe fn set_following_responder(&self, next_responder: Option<&NSResponder>) {
        // SAFETY: The caller upholds AppKit's unretained responder lifetime.
        unsafe { self.setNextResponder(next_responder) };
    }

    fn with_input_context<T>(&self, f: impl FnOnce(&NSTextInputContext) -> T) -> Option<T> {
        self.ivars().input_context.get().map(|context| f(context))
    }

    fn handle_text_event(&self, event: TextInputEvent) {
        self.ivars().host.handle_text_input_event(event);
        self.notify_text_state_changed();
    }

    fn notify_text_state_changed(&self) {
        self.notify_selection_changed();
        self.invalidate_character_coordinates();
    }
}
