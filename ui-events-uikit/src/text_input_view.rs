// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A hierarchy-backed UIKit text-input view.

use alloc::{boxed::Box, string::ToString, vec::Vec};
use core::cell::{OnceCell, RefCell};

use dpi::PhysicalPosition;
use objc2::rc::{Retained, Weak};
use objc2::runtime::{AnyObject, NSObjectProtocol, ProtocolObject};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, Message, define_class, msg_send};
use objc2_core_foundation::{CGPoint, CGRect};
use objc2_foundation::{
    NSArray, NSAttributedStringKey, NSComparisonResult, NSCopying, NSDictionary, NSRange, NSString,
};
use objc2_ui_kit::{
    NSWritingDirection, UIEvent, UIKey, UIKeyInput, UIKeyboardType, UIPress, UIPressesEvent,
    UIReturnKeyType, UITextAutocapitalizationType, UITextAutocorrectionType, UITextInput,
    UITextInputDelegate, UITextInputStringTokenizer, UITextInputTokenizer, UITextInputTraits,
    UITextLayoutDirection, UITextPosition, UITextRange, UITextSelectionRect,
    UITextSpellCheckingType, UITouch, UIView,
};
use ui_events::pointer::PointerEvent;
use ui_events::text::{TextInputAction, TextInputEvent, TextRangeEncoding, TextTargetRange};
use ui_events_apple_common::EventDisposition;
use ui_text_input::{
    TextClosestPositionProvider, TextGeometryProvider, TextHitTestProvider, TextInputHints,
    TextInputHost, TextInputPurpose, TextInputRect, TextRangeConverter, TextRangeProvider,
};

use crate::input_responder::UIKitInputResponderHost;
use crate::text_host::{
    document_range_from_host, first_rect_for_range_from_host, has_text_from_host,
    marked_text_range_from_host, selected_text_range_from_host, text_in_range_from_host,
};
use crate::{
    composition_end_event, composition_update_event_from_nsstring_and_selected_range,
    delete_backward_text_event, insert_text_event_from_nsstring, keyboard_event_from_uikey,
    keyboard_event_from_uipress, pointer_event_from_touch, pointer_event_from_touch_and_event,
};

/// A selection rectangle in `UIKitTextInputView` coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UIKitSelectionRect {
    /// Rectangle in UIKit logical points.
    pub rect: CGRect,
    /// Whether the rectangle contains the selection start.
    pub contains_start: bool,
    /// Whether the rectangle contains the selection end.
    pub contains_end: bool,
    /// Whether the text in this rectangle is vertical.
    pub is_vertical: bool,
    /// Base writing direction for the text in this rectangle.
    pub writing_direction: NSWritingDirection,
}

/// Host-side contract for [`UIKitTextInputView`].
///
/// All mutation callbacks must update host state synchronously. UIKit may
/// query the new selection, marked range, or text before the callback returns.
/// Raw [`KeyboardEvent`][ui_events::keyboard::KeyboardEvent] values are useful
/// for key state and shortcuts, but the
/// text callbacks are authoritative and must not be inserted a second time.
pub trait UIKitTextInputViewHost:
    UIKitInputResponderHost
    + TextInputHost
    + TextRangeProvider
    + TextRangeConverter
    + TextGeometryProvider
    + TextHitTestProvider
    + TextClosestPositionProvider
{
    /// Apply a translated UIKit text mutation synchronously.
    fn handle_text_input_event(&self, event: TextInputEvent);

    /// Convert a host-local text rectangle into view-local UIKit points.
    fn text_input_rect_to_view_rect(&self, rect: TextInputRect) -> CGRect;

    /// Convert a view-local UIKit point into the host hit-test coordinate space.
    fn view_point_to_text_input_point(&self, point: CGPoint) -> PhysicalPosition<f64>;

    /// Return the caret rectangle for an arbitrary UTF-16 document position.
    fn caret_rect_for_utf16_position(&self, position: u32) -> Option<TextInputRect>;

    /// Move from a UTF-16 position in a visual layout direction.
    ///
    /// The host owns this operation because bidirectional and vertical layout
    /// cannot be derived correctly from storage offsets.
    fn position_from_utf16_position_in_direction(
        &self,
        position: u32,
        direction: UITextLayoutDirection,
        offset: isize,
    ) -> Option<u32>;

    /// Return the farthest UTF-16 position in `range` in a layout direction.
    fn position_within_utf16_range_farthest_in_direction(
        &self,
        range: TextTargetRange,
        direction: UITextLayoutDirection,
    ) -> Option<u32>;

    /// Return the character range obtained by extending a UTF-16 position.
    fn character_range_by_extending_utf16_position(
        &self,
        position: u32,
        direction: UITextLayoutDirection,
    ) -> Option<TextTargetRange>;

    /// Return view-local rectangles used to display the given selection.
    fn selection_rects_for_utf16_range(&self, range: TextTargetRange) -> Vec<UIKitSelectionRect>;

    /// Return the base writing direction at a UTF-16 document position.
    fn base_writing_direction_for_utf16_position(&self, position: u32) -> NSWritingDirection;

    /// Apply a base writing direction to a UTF-16 document range.
    fn set_base_writing_direction_for_utf16_range(
        &self,
        range: TextTargetRange,
        direction: NSWritingDirection,
    );
}

#[derive(Debug)]
struct PositionState {
    offset: u32,
}

define_class!(
    #[unsafe(super = UITextPosition)]
    #[thread_kind = MainThreadOnly]
    #[name = "UIEventsUIKitTextPosition"]
    #[ivars = PositionState]
    struct UIKitTextPosition;
);

impl UIKitTextPosition {
    fn new(mtm: MainThreadMarker, offset: u32) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(PositionState { offset });
        // SAFETY: `UITextPosition` has no additional initialization arguments.
        unsafe { msg_send![super(this), init] }
    }

    fn offset(&self) -> u32 {
        self.ivars().offset
    }
}

#[derive(Debug)]
struct RangeState {
    start: u32,
    end: u32,
}

define_class!(
    #[unsafe(super = UITextRange)]
    #[thread_kind = MainThreadOnly]
    #[name = "UIEventsUIKitTextRange"]
    #[ivars = RangeState]
    struct UIKitTextRange;

    impl UIKitTextRange {
        #[unsafe(method(isEmpty))]
        fn is_empty(&self) -> bool {
            self.ivars().start == self.ivars().end
        }

        #[unsafe(method_id(start))]
        fn start(&self) -> Retained<UITextPosition> {
            UIKitTextPosition::new(MainThreadMarker::from(self), self.ivars().start).into_super()
        }

        #[unsafe(method_id(end))]
        fn end(&self) -> Retained<UITextPosition> {
            UIKitTextPosition::new(MainThreadMarker::from(self), self.ivars().end).into_super()
        }
    }
);

impl UIKitTextRange {
    fn new(mtm: MainThreadMarker, start: u32, end: u32) -> Option<Retained<Self>> {
        if start > end {
            return None;
        }
        let this = Self::alloc(mtm).set_ivars(RangeState { start, end });
        // SAFETY: `UITextRange` has no additional initialization arguments.
        Some(unsafe { msg_send![super(this), init] })
    }

    fn target_range(&self) -> TextTargetRange {
        TextTargetRange::utf16_code_units(self.ivars().start, self.ivars().end)
    }
}

#[derive(Debug)]
struct SelectionRectState {
    value: UIKitSelectionRect,
}

define_class!(
    #[unsafe(super = UITextSelectionRect)]
    #[thread_kind = MainThreadOnly]
    #[name = "UIEventsUIKitSelectionRect"]
    #[ivars = SelectionRectState]
    struct UIKitNativeSelectionRect;

    impl UIKitNativeSelectionRect {
        #[unsafe(method(rect))]
        fn rect(&self) -> CGRect {
            self.ivars().value.rect
        }

        #[unsafe(method(containsStart))]
        fn contains_start(&self) -> bool {
            self.ivars().value.contains_start
        }

        #[unsafe(method(containsEnd))]
        fn contains_end(&self) -> bool {
            self.ivars().value.contains_end
        }

        #[unsafe(method(isVertical))]
        fn is_vertical(&self) -> bool {
            self.ivars().value.is_vertical
        }

        #[unsafe(method(writingDirection))]
        fn writing_direction(&self) -> NSWritingDirection {
            self.ivars().value.writing_direction
        }
    }
);

impl UIKitNativeSelectionRect {
    fn new(mtm: MainThreadMarker, value: UIKitSelectionRect) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(SelectionRectState { value });
        // SAFETY: `UITextSelectionRect` has no additional initialization arguments.
        unsafe { msg_send![super(this), init] }
    }
}

#[doc(hidden)]
pub struct TextInputViewState {
    host: Box<dyn UIKitTextInputViewHost>,
    input_delegate: RefCell<Weak<ProtocolObject<dyn UITextInputDelegate>>>,
    tokenizer: OnceCell<Retained<UITextInputStringTokenizer>>,
    marked_text_style: RefCell<Option<Retained<NSDictionary<NSAttributedStringKey, AnyObject>>>>,
}

impl core::fmt::Debug for TextInputViewState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TextInputViewState")
            .field("host", &"<dyn UIKitTextInputViewHost>")
            .field(
                "input_delegate",
                &self.input_delegate.borrow().load().is_some(),
            )
            .field("tokenizer", &self.tokenizer.get().is_some())
            .field(
                "marked_text_style",
                &self.marked_text_style.borrow().is_some(),
            )
            .finish()
    }
}

define_class!(
    #[unsafe(super = UIView)]
    #[thread_kind = MainThreadOnly]
    #[name = "UIEventsUIKitTextInputView"]
    #[ivars = TextInputViewState]
    #[doc = "Hierarchy-backed UIKit view for keyboard and multistage text input."]
    pub struct UIKitTextInputView;

    impl UIKitTextInputView {
        #[unsafe(method(canBecomeFirstResponder))]
        fn can_become_first_responder(&self) -> bool {
            true
        }

        #[unsafe(method(touchesBegan:withEvent:))]
        fn touches_began(&self, touches: &objc2_foundation::NSSet<UITouch>, event: Option<&UIEvent>) {
            self.forward_unhandled_touches(touches, event, TouchPhase::Began);
        }

        #[unsafe(method(touchesMoved:withEvent:))]
        fn touches_moved(&self, touches: &objc2_foundation::NSSet<UITouch>, event: Option<&UIEvent>) {
            self.forward_unhandled_touches(touches, event, TouchPhase::Moved);
        }

        #[unsafe(method(touchesEnded:withEvent:))]
        fn touches_ended(&self, touches: &objc2_foundation::NSSet<UITouch>, event: Option<&UIEvent>) {
            self.forward_unhandled_touches(touches, event, TouchPhase::Ended);
        }

        #[unsafe(method(touchesCancelled:withEvent:))]
        fn touches_cancelled(&self, touches: &objc2_foundation::NSSet<UITouch>, event: Option<&UIEvent>) {
            self.forward_unhandled_touches(touches, event, TouchPhase::Cancelled);
        }

        #[unsafe(method(pressesBegan:withEvent:))]
        fn presses_began(&self, presses: &objc2_foundation::NSSet<UIPress>, event: Option<&UIPressesEvent>) {
            self.forward_unhandled_presses(presses, event, PressPhase::Began);
        }

        #[unsafe(method(pressesChanged:withEvent:))]
        fn presses_changed(&self, presses: &objc2_foundation::NSSet<UIPress>, event: Option<&UIPressesEvent>) {
            self.forward_unhandled_presses(presses, event, PressPhase::Changed);
        }

        #[unsafe(method(pressesEnded:withEvent:))]
        fn presses_ended(&self, presses: &objc2_foundation::NSSet<UIPress>, event: Option<&UIPressesEvent>) {
            self.forward_unhandled_presses(presses, event, PressPhase::Ended);
        }

        #[unsafe(method(pressesCancelled:withEvent:))]
        fn presses_cancelled(&self, presses: &objc2_foundation::NSSet<UIPress>, event: Option<&UIPressesEvent>) {
            self.forward_unhandled_presses(presses, event, PressPhase::Cancelled);
        }
    }

    unsafe impl NSObjectProtocol for UIKitTextInputView {}

    unsafe impl UITextInputTraits for UIKitTextInputView {
        #[unsafe(method(autocapitalizationType))]
        fn autocapitalization_type(&self) -> UITextAutocapitalizationType {
            let hints = self.ivars().host.text_input_configuration().hints;
            if hints.contains(TextInputHints::UPPERCASE) {
                UITextAutocapitalizationType::AllCharacters
            } else if hints.contains(TextInputHints::TITLECASE) {
                UITextAutocapitalizationType::Words
            } else if hints.contains(TextInputHints::AUTO_CAPITALIZATION) {
                UITextAutocapitalizationType::Sentences
            } else {
                UITextAutocapitalizationType::None
            }
        }

        #[unsafe(method(autocorrectionType))]
        fn autocorrection_type(&self) -> UITextAutocorrectionType {
            let config = self.ivars().host.text_input_configuration();
            if config.hints.contains(TextInputHints::SENSITIVE_DATA)
                || config.hints.contains(TextInputHints::HIDDEN_TEXT)
            {
                UITextAutocorrectionType::No
            } else if config.hints.contains(TextInputHints::COMPLETION) {
                UITextAutocorrectionType::Yes
            } else {
                UITextAutocorrectionType::Default
            }
        }

        #[unsafe(method(spellCheckingType))]
        fn spell_checking_type(&self) -> UITextSpellCheckingType {
            let config = self.ivars().host.text_input_configuration();
            if config.hints.contains(TextInputHints::SENSITIVE_DATA)
                || config.hints.contains(TextInputHints::HIDDEN_TEXT)
            {
                UITextSpellCheckingType::No
            } else if config.hints.contains(TextInputHints::SPELLCHECK) {
                UITextSpellCheckingType::Yes
            } else {
                UITextSpellCheckingType::Default
            }
        }

        #[unsafe(method(keyboardType))]
        fn keyboard_type(&self) -> UIKeyboardType {
            match self.ivars().host.text_input_configuration().purpose {
                TextInputPurpose::Alpha => UIKeyboardType::Alphabet,
                TextInputPurpose::Digits | TextInputPurpose::Pin => UIKeyboardType::NumberPad,
                TextInputPurpose::Number => UIKeyboardType::DecimalPad,
                TextInputPurpose::Phone => UIKeyboardType::PhonePad,
                TextInputPurpose::Url => UIKeyboardType::URL,
                TextInputPurpose::Email => UIKeyboardType::EmailAddress,
                TextInputPurpose::Name => UIKeyboardType::NamePhonePad,
                _ => UIKeyboardType::Default,
            }
        }

        #[unsafe(method(returnKeyType))]
        fn return_key_type(&self) -> UIReturnKeyType {
            match self.ivars().host.text_input_configuration().action {
                Some(TextInputAction::Go) => UIReturnKeyType::Go,
                Some(TextInputAction::Search) => UIReturnKeyType::Search,
                Some(TextInputAction::Send) => UIReturnKeyType::Send,
                Some(TextInputAction::Next) => UIReturnKeyType::Next,
                Some(TextInputAction::Done) => UIReturnKeyType::Done,
                _ => UIReturnKeyType::Default,
            }
        }

        #[unsafe(method(isSecureTextEntry))]
        fn is_secure_text_entry(&self) -> bool {
            let config = self.ivars().host.text_input_configuration();
            matches!(config.purpose, TextInputPurpose::Password | TextInputPurpose::Pin)
                || config.hints.contains(TextInputHints::HIDDEN_TEXT)
        }
    }

    unsafe impl UIKeyInput for UIKitTextInputView {
        #[unsafe(method(hasText))]
        fn has_text(&self) -> bool {
            has_text_from_host(&*self.ivars().host)
        }

        #[unsafe(method(insertText:))]
        fn insert_text(&self, text: &NSString) {
            self.ivars()
                .host
                .handle_text_input_event(insert_text_event_from_nsstring(text));
        }

        #[unsafe(method(deleteBackward))]
        fn delete_backward(&self) {
            self.ivars()
                .host
                .handle_text_input_event(delete_backward_text_event());
        }
    }

    #[allow(
        non_snake_case,
        reason = "objc2 protocol methods must match UIKit selector spellings"
    )]
    unsafe impl UITextInput for UIKitTextInputView {
        #[unsafe(method_id(textInRange:))]
        fn textInRange(&self, range: &UITextRange) -> Option<Retained<NSString>> {
            nsrange_from_text_range(range)
                .and_then(|range| text_in_range_from_host(&*self.ivars().host, range))
        }

        #[unsafe(method(replaceRange:withText:))]
        fn replaceRange_withText(&self, range: &UITextRange, text: &NSString) {
            let Some(range) = target_range_from_text_range(range) else {
                return;
            };
            self.ivars()
                .host
                .handle_text_input_event(TextInputEvent::replace(text.to_string(), range));
        }

        #[unsafe(method_id(selectedTextRange))]
        fn selectedTextRange(&self) -> Option<Retained<UITextRange>> {
            selected_text_range_from_host(&*self.ivars().host).and_then(retained_text_range)
        }

        #[unsafe(method(setSelectedTextRange:))]
        fn setSelectedTextRange(&self, selected_text_range: Option<&UITextRange>) {
            let Some(range) = selected_text_range.and_then(target_range_from_text_range) else {
                return;
            };
            self.ivars()
                .host
                .handle_text_input_event(TextInputEvent::set_selection(range));
        }

        #[unsafe(method_id(markedTextRange))]
        fn markedTextRange(&self) -> Option<Retained<UITextRange>> {
            marked_text_range_from_host(&*self.ivars().host).and_then(retained_text_range)
        }

        #[unsafe(method_id(markedTextStyle))]
        fn markedTextStyle(
            &self,
        ) -> Option<Retained<NSDictionary<NSAttributedStringKey, AnyObject>>> {
            self.ivars().marked_text_style.borrow().clone()
        }

        #[unsafe(method(setMarkedTextStyle:))]
        unsafe fn setMarkedTextStyle(
            &self,
            marked_text_style: Option<&NSDictionary<NSAttributedStringKey, AnyObject>>,
        ) {
            self.ivars()
                .marked_text_style
                .replace(marked_text_style.map(NSCopying::copy));
        }

        #[unsafe(method(setMarkedText:selectedRange:))]
        fn setMarkedText_selectedRange(
            &self,
            marked_text: Option<&NSString>,
            selected_range: NSRange,
        ) {
            let Some(marked_text) = marked_text else {
                self.ivars()
                    .host
                    .handle_text_input_event(composition_end_event());
                return;
            };
            if let Some(event) = composition_update_event_from_nsstring_and_selected_range(
                marked_text,
                selected_range,
            ) {
                self.ivars().host.handle_text_input_event(event);
            }
        }

        #[unsafe(method(unmarkText))]
        fn unmarkText(&self) {
            self.ivars()
                .host
                .handle_text_input_event(composition_end_event());
        }

        #[unsafe(method_id(beginningOfDocument))]
        fn beginningOfDocument(&self) -> Retained<UITextPosition> {
            let start = document_range_from_host(&*self.ivars().host)
                .and_then(|range| u32::try_from(range.location).ok())
                .unwrap_or_default();
            retained_text_position(start)
        }

        #[unsafe(method_id(endOfDocument))]
        fn endOfDocument(&self) -> Retained<UITextPosition> {
            let end = document_range_from_host(&*self.ivars().host)
                .and_then(|range| range.location.checked_add(range.length))
                .and_then(|end| u32::try_from(end).ok())
                .unwrap_or_default();
            retained_text_position(end)
        }

        #[unsafe(method_id(textRangeFromPosition:toPosition:))]
        fn textRangeFromPosition_toPosition(
            &self,
            from_position: &UITextPosition,
            to_position: &UITextPosition,
        ) -> Option<Retained<UITextRange>> {
            offset_from_text_position(from_position)
                .zip(offset_from_text_position(to_position))
                .and_then(|(from, to)| valid_utf16_range(&*self.ivars().host, from, to))
                .and_then(retained_target_text_range)
        }

        #[unsafe(method_id(positionFromPosition:offset:))]
        fn positionFromPosition_offset(
            &self,
            position: &UITextPosition,
            offset: isize,
        ) -> Option<Retained<UITextPosition>> {
            (|| {
                let position = offset_from_text_position(position)?;
                let offset = i64::try_from(offset).ok()?;
                let result = i64::from(position).checked_add(offset)?;
                let result = u32::try_from(result).ok()?;
                valid_utf16_position(&*self.ivars().host, result)
                    .then(|| retained_text_position(result))
            })()
        }

        #[unsafe(method_id(positionFromPosition:inDirection:offset:))]
        fn positionFromPosition_inDirection_offset(
            &self,
            position: &UITextPosition,
            direction: UITextLayoutDirection,
            offset: isize,
        ) -> Option<Retained<UITextPosition>> {
            (|| {
                let position = offset_from_text_position(position)?;
                let result = self
                    .ivars()
                    .host
                    .position_from_utf16_position_in_direction(position, direction, offset)?;
                valid_utf16_position(&*self.ivars().host, result)
                    .then(|| retained_text_position(result))
            })()
        }

        #[unsafe(method(comparePosition:toPosition:))]
        fn comparePosition_toPosition(
            &self,
            position: &UITextPosition,
            other: &UITextPosition,
        ) -> NSComparisonResult {
            offset_from_text_position(position)
                .zip(offset_from_text_position(other))
                .map_or(NSComparisonResult::Same, |(position, other)| {
                    NSComparisonResult::from(position.cmp(&other))
                })
        }

        #[unsafe(method(offsetFromPosition:toPosition:))]
        fn offsetFromPosition_toPosition(
            &self,
            from: &UITextPosition,
            to_position: &UITextPosition,
        ) -> isize {
            let Some((from, to)) =
                offset_from_text_position(from).zip(offset_from_text_position(to_position))
            else {
                return 0;
            };
            isize::try_from(i64::from(to) - i64::from(from)).unwrap_or_default()
        }

        #[unsafe(method_id(inputDelegate))]
        fn inputDelegate(&self) -> Option<Retained<ProtocolObject<dyn UITextInputDelegate>>> {
            self.ivars().input_delegate.borrow().load()
        }

        #[unsafe(method(setInputDelegate:))]
        fn setInputDelegate(
            &self,
            input_delegate: Option<&ProtocolObject<dyn UITextInputDelegate>>,
        ) {
            self.ivars()
                .input_delegate
                .replace(input_delegate.map(Weak::from).unwrap_or_default());
        }

        #[unsafe(method_id(tokenizer))]
        fn tokenizer(&self) -> Retained<ProtocolObject<dyn UITextInputTokenizer>> {
            let tokenizer = self.ivars().tokenizer.get_or_init(|| {
                // SAFETY: this class implements the complete `UITextInput` protocol.
                unsafe {
                    UITextInputStringTokenizer::initWithTextInput(
                        UITextInputStringTokenizer::alloc(MainThreadMarker::from(self)),
                        self,
                    )
                }
            });
            ProtocolObject::from_retained(tokenizer.clone())
        }

        #[unsafe(method_id(positionWithinRange:farthestInDirection:))]
        fn positionWithinRange_farthestInDirection(
            &self,
            range: &UITextRange,
            direction: UITextLayoutDirection,
        ) -> Option<Retained<UITextPosition>> {
            (|| {
                let range = target_range_from_text_range(range)?;
                let position = self
                    .ivars()
                    .host
                    .position_within_utf16_range_farthest_in_direction(range, direction)?;
                position_in_range(range, position).then(|| retained_text_position(position))
            })()
        }

        #[unsafe(method_id(characterRangeByExtendingPosition:inDirection:))]
        fn characterRangeByExtendingPosition_inDirection(
            &self,
            position: &UITextPosition,
            direction: UITextLayoutDirection,
        ) -> Option<Retained<UITextRange>> {
            (|| {
                let position = offset_from_text_position(position)?;
                let range = self
                    .ivars()
                    .host
                    .character_range_by_extending_utf16_position(position, direction)?;
                valid_host_target_range(&*self.ivars().host, range)?;
                retained_target_text_range(range)
            })()
        }

        #[unsafe(method(baseWritingDirectionForPosition:inDirection:))]
        fn baseWritingDirectionForPosition_inDirection(
            &self,
            position: &UITextPosition,
            _direction: objc2_ui_kit::UITextStorageDirection,
        ) -> NSWritingDirection {
            offset_from_text_position(position).map_or(NSWritingDirection::Natural, |position| {
                self.ivars()
                    .host
                    .base_writing_direction_for_utf16_position(position)
            })
        }

        #[unsafe(method(setBaseWritingDirection:forRange:))]
        fn setBaseWritingDirection_forRange(
            &self,
            writing_direction: NSWritingDirection,
            range: &UITextRange,
        ) {
            if let Some(range) = target_range_from_text_range(range) {
                self.ivars()
                    .host
                    .set_base_writing_direction_for_utf16_range(range, writing_direction);
            }
        }

        #[unsafe(method(firstRectForRange:))]
        fn firstRectForRange(&self, range: &UITextRange) -> CGRect {
            let Some(range) = nsrange_from_text_range(range) else {
                return CGRect::ZERO;
            };
            first_rect_for_range_from_host(&*self.ivars().host, range, |rect| {
                self.ivars().host.text_input_rect_to_view_rect(rect)
            })
            .unwrap_or(CGRect::ZERO)
        }

        #[unsafe(method(caretRectForPosition:))]
        fn caretRectForPosition(&self, position: &UITextPosition) -> CGRect {
            offset_from_text_position(position)
                .and_then(|position| {
                    self.ivars()
                        .host
                        .caret_rect_for_utf16_position(position)
                })
                .map(|rect| self.ivars().host.text_input_rect_to_view_rect(rect))
                .unwrap_or(CGRect::ZERO)
        }

        #[unsafe(method_id(selectionRectsForRange:))]
        fn selectionRectsForRange(
            &self,
            range: &UITextRange,
        ) -> Retained<NSArray<UITextSelectionRect>> {
            let mtm = MainThreadMarker::from(self);
            let rects = target_range_from_text_range(range)
                .map(|range| {
                    self.ivars()
                        .host
                        .selection_rects_for_utf16_range(range)
                        .into_iter()
                        .map(|rect| UIKitNativeSelectionRect::new(mtm, rect).into_super())
                        .collect::<Vec<Retained<UITextSelectionRect>>>()
                })
                .unwrap_or_default();
            NSArray::from_retained_slice(&rects)
        }

        #[unsafe(method_id(closestPositionToPoint:))]
        fn closestPositionToPoint(&self, point: CGPoint) -> Option<Retained<UITextPosition>> {
            let point = self.ivars().host.view_point_to_text_input_point(point);
            self
                .ivars()
                .host
                .closest_text_offset_to_point(point, TextRangeEncoding::Utf16CodeUnits)
                .filter(|position| valid_utf16_position(&*self.ivars().host, *position))
                .map(retained_text_position)
        }

        #[unsafe(method_id(closestPositionToPoint:withinRange:))]
        fn closestPositionToPoint_withinRange(
            &self,
            point: CGPoint,
            range: &UITextRange,
        ) -> Option<Retained<UITextPosition>> {
            (|| {
                let range = target_range_from_text_range(range)?;
                let point = self.ivars().host.view_point_to_text_input_point(point);
                let position = self
                    .ivars()
                    .host
                    .closest_text_offset_to_point(point, TextRangeEncoding::Utf16CodeUnits)?;
                let position = position.clamp(range.range.start, range.range.end);
                Some(retained_text_position(position))
            })()
        }

        #[unsafe(method_id(characterRangeAtPoint:))]
        fn characterRangeAtPoint(&self, point: CGPoint) -> Option<Retained<UITextRange>> {
            (|| {
                let point = self.ivars().host.view_point_to_text_input_point(point);
                let position = self
                    .ivars()
                    .host
                    .text_offset_at_point(point, TextRangeEncoding::Utf16CodeUnits)?;
                let range = self
                    .ivars()
                    .host
                    .character_range_by_extending_utf16_position(
                        position,
                        UITextLayoutDirection::Right,
                    )?;
                valid_host_target_range(&*self.ivars().host, range)?;
                retained_target_text_range(range)
            })()
        }

        #[unsafe(method_id(textInputView))]
        fn textInputView(&self) -> Retained<UIView> {
            self.retain().into_super()
        }
    }
);

impl UIKitTextInputView {
    /// Create a view-backed UIKit text-input client.
    ///
    /// The caller must add this view to the active key window's view hierarchy
    /// before calling `becomeFirstResponder`. Its frame should cover the
    /// editor area used by the host's point and rectangle conversions.
    pub fn new(
        mtm: MainThreadMarker,
        frame: CGRect,
        host: Box<dyn UIKitTextInputViewHost>,
    ) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(TextInputViewState {
            host,
            input_delegate: RefCell::new(Weak::default()),
            tokenizer: OnceCell::new(),
            marked_text_style: RefCell::new(None),
        });
        // SAFETY: `frame` is a valid Core Graphics value for `UIView` initialization.
        let this: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: frame] };
        this.setOpaque(false);
        this
    }

    /// Notify UIKit immediately before an external host text change.
    pub fn notify_text_will_change(&self) {
        if let Some(delegate) = self.ivars().input_delegate.borrow().load() {
            delegate.textWillChange(Some(ProtocolObject::from_ref(self)));
        }
    }

    /// Notify UIKit immediately after an external host text change.
    pub fn notify_text_did_change(&self) {
        if let Some(delegate) = self.ivars().input_delegate.borrow().load() {
            delegate.textDidChange(Some(ProtocolObject::from_ref(self)));
        }
    }

    /// Notify UIKit immediately before an external host selection change.
    pub fn notify_selection_will_change(&self) {
        if let Some(delegate) = self.ivars().input_delegate.borrow().load() {
            delegate.selectionWillChange(Some(ProtocolObject::from_ref(self)));
        }
    }

    /// Notify UIKit immediately after an external host selection change.
    pub fn notify_selection_did_change(&self) {
        if let Some(delegate) = self.ivars().input_delegate.borrow().load() {
            delegate.selectionDidChange(Some(ProtocolObject::from_ref(self)));
        }
    }

    fn handle_touch(&self, touch: &UITouch, event: Option<&UIEvent>) -> EventDisposition {
        let scale_factor = self.ivars().host.pointer_scale_factor();
        let event = event
            .and_then(|event| pointer_event_from_touch_and_event(touch, event, scale_factor))
            .or_else(|| pointer_event_from_touch(touch, scale_factor));
        event.map_or(EventDisposition::Unhandled, |event: PointerEvent| {
            self.ivars().host.handle_pointer_event(event)
        })
    }

    fn handle_press(&self, press: &UIPress) -> EventDisposition {
        let mtm = MainThreadMarker::from(self);
        let event = press
            .key(mtm)
            .as_deref()
            .and_then(|key: &UIKey| keyboard_event_from_uikey(press, key))
            .or_else(|| keyboard_event_from_uipress(press));
        event.map_or(EventDisposition::Unhandled, |event| {
            self.ivars().host.handle_keyboard_event(event)
        })
    }

    fn forward_unhandled_touches(
        &self,
        touches: &objc2_foundation::NSSet<UITouch>,
        event: Option<&UIEvent>,
        phase: TouchPhase,
    ) {
        let unhandled = touches
            .iter()
            .filter(|touch| !self.handle_touch(touch, event).is_handled())
            .collect::<Vec<_>>();
        if unhandled.is_empty() {
            return;
        }
        let unhandled = objc2_foundation::NSSet::from_retained_slice(&unhandled);
        // SAFETY: these calls invoke the corresponding overridden `UIView` method.
        unsafe {
            match phase {
                TouchPhase::Began => {
                    let _: () = msg_send![super(self), touchesBegan: &*unhandled, withEvent: event];
                }
                TouchPhase::Moved => {
                    let _: () = msg_send![super(self), touchesMoved: &*unhandled, withEvent: event];
                }
                TouchPhase::Ended => {
                    let _: () = msg_send![super(self), touchesEnded: &*unhandled, withEvent: event];
                }
                TouchPhase::Cancelled => {
                    let _: () =
                        msg_send![super(self), touchesCancelled: &*unhandled, withEvent: event];
                }
            }
        }
    }

    fn forward_unhandled_presses(
        &self,
        presses: &objc2_foundation::NSSet<UIPress>,
        event: Option<&UIPressesEvent>,
        phase: PressPhase,
    ) {
        let unhandled = presses
            .iter()
            .filter(|press| !self.handle_press(press).is_handled())
            .collect::<Vec<_>>();
        if unhandled.is_empty() {
            return;
        }
        let unhandled = objc2_foundation::NSSet::from_retained_slice(&unhandled);
        // SAFETY: these calls invoke the corresponding overridden `UIView` method.
        unsafe {
            match phase {
                PressPhase::Began => {
                    let _: () = msg_send![super(self), pressesBegan: &*unhandled, withEvent: event];
                }
                PressPhase::Changed => {
                    let _: () =
                        msg_send![super(self), pressesChanged: &*unhandled, withEvent: event];
                }
                PressPhase::Ended => {
                    let _: () = msg_send![super(self), pressesEnded: &*unhandled, withEvent: event];
                }
                PressPhase::Cancelled => {
                    let _: () =
                        msg_send![super(self), pressesCancelled: &*unhandled, withEvent: event];
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum TouchPhase {
    Began,
    Moved,
    Ended,
    Cancelled,
}

#[derive(Clone, Copy, Debug)]
enum PressPhase {
    Began,
    Changed,
    Ended,
    Cancelled,
}

fn retained_text_position(offset: u32) -> Retained<UITextPosition> {
    UIKitTextPosition::new(
        MainThreadMarker::new().expect("UIKit runs on the main thread"),
        offset,
    )
    .into_super()
}

fn offset_from_text_position(position: &UITextPosition) -> Option<u32> {
    position
        .downcast_ref::<UIKitTextPosition>()
        .map(UIKitTextPosition::offset)
}

fn retained_text_range(range: NSRange) -> Option<Retained<UITextRange>> {
    let start = u32::try_from(range.location).ok()?;
    let end = range.location.checked_add(range.length)?;
    retained_target_text_range(TextTargetRange::utf16_code_units(
        start,
        u32::try_from(end).ok()?,
    ))
}

fn retained_target_text_range(range: TextTargetRange) -> Option<Retained<UITextRange>> {
    (range.encoding == TextRangeEncoding::Utf16CodeUnits).then_some(())?;
    UIKitTextRange::new(
        MainThreadMarker::new().expect("UIKit runs on the main thread"),
        range.range.start,
        range.range.end,
    )
    .map(Retained::into_super)
}

fn target_range_from_text_range(range: &UITextRange) -> Option<TextTargetRange> {
    range
        .downcast_ref::<UIKitTextRange>()
        .map(UIKitTextRange::target_range)
}

fn nsrange_from_text_range(range: &UITextRange) -> Option<NSRange> {
    let range = target_range_from_text_range(range)?;
    let start = usize::try_from(range.range.start).ok()?;
    let length = usize::try_from(range.range.end.checked_sub(range.range.start)?).ok()?;
    Some(NSRange::new(start, length))
}

fn valid_utf16_position(
    host: &(impl TextInputHost + TextRangeConverter + ?Sized),
    position: u32,
) -> bool {
    document_target_range(host).is_some_and(|range| position_in_range(range, position))
}

fn valid_utf16_range(
    host: &(impl TextInputHost + TextRangeConverter + ?Sized),
    from: u32,
    to: u32,
) -> Option<TextTargetRange> {
    let range = TextTargetRange::utf16_code_units(from.min(to), from.max(to));
    valid_host_target_range(host, range)
}

fn valid_host_target_range(
    host: &(impl TextInputHost + TextRangeConverter + ?Sized),
    range: TextTargetRange,
) -> Option<TextTargetRange> {
    if range.encoding != TextRangeEncoding::Utf16CodeUnits || range.range.start > range.range.end {
        return None;
    }
    let document = document_target_range(host)?;
    (range.range.start >= document.range.start && range.range.end <= document.range.end)
        .then_some(range)
}

fn document_target_range(
    host: &(impl TextInputHost + TextRangeConverter + ?Sized),
) -> Option<TextTargetRange> {
    let range = document_range_from_host(host)?;
    let start = u32::try_from(range.location).ok()?;
    let end = range.location.checked_add(range.length)?;
    Some(TextTargetRange::utf16_code_units(
        start,
        u32::try_from(end).ok()?,
    ))
}

const fn position_in_range(range: TextTargetRange, position: u32) -> bool {
    position >= range.range.start && position <= range.range.end
}
