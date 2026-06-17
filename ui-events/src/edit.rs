// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! # Edit command event types
//!
//! This module contains transport-agnostic editing commands such as cursor
//! movement, deletion, and selection operations.
//!
//! These events are intentionally separate from
//! [`crate::keyboard::KeyboardEvent`] and [`crate::text::TextInputEvent`]:
//!
//! - Keyboard events represent key transitions and shortcuts.
//! - Text events represent committed text, composition updates, and text-input
//!   deletion requests.
//! - Edit commands represent semantic editor actions such as moving the caret,
//!   deleting a word, or selecting all content.
//!
//! This maps especially well to AppKit's `NSStandardKeyBindingResponding`
//! protocol and `doCommandBySelector:` callbacks, where the platform
//! communicates editing intent directly rather than sending a physical key
//! transition or committed text payload.
//!
//! ## Deletion routing
//!
//! Plain backward/forward deletion may be reported through either a text-input
//! path or a resolved editor-command path, so it exists in both
//! [`crate::text::TextInputEvent`] and [`EditCommandEvent`], mirroring the
//! authoritative platform source. Richer deletes such as word, line, and
//! paragraph deletion live only on [`EditCommandEvent`].
//!
//! Adapters must emit deletion through exactly one event family per platform
//! action. They should choose the family matching the authoritative platform
//! callback, and must not additionally mutate from the corresponding
//! [`crate::keyboard::KeyboardEvent`]. For example, AppKit
//! `deleteBackward:`/`deleteForward:` delivered through `doCommandBySelector:`
//! map to [`EditCommandEvent::DeleteBackward`] and
//! [`EditCommandEvent::DeleteForward`]. Hosts observing multiple channels
//! should treat the text/edit callback as authoritative over the raw key event.

/// A semantic editing command.
///
/// These commands are higher-level than physical key events and lower-level
/// than any particular text-editor implementation.
///
/// The current variants intentionally mirror the subset of AppKit standard
/// key-binding commands from `NSStandardKeyBindingResponding` that
/// `ui-events` currently models.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EditCommandEvent {
    /// Move backward in the current text direction.
    MoveBackward,
    /// Extend the selection while moving backward.
    MoveBackwardAndModifySelection,
    /// Move forward in the current text direction.
    MoveForward,
    /// Extend the selection while moving forward.
    MoveForwardAndModifySelection,
    /// Move to the visual left.
    MoveLeft,
    /// Extend the selection while moving to the visual left.
    MoveLeftAndModifySelection,
    /// Move to the visual right.
    MoveRight,
    /// Extend the selection while moving to the visual right.
    MoveRightAndModifySelection,
    /// Move upward.
    MoveUp,
    /// Extend the selection while moving upward.
    MoveUpAndModifySelection,
    /// Move downward.
    MoveDown,
    /// Extend the selection while moving downward.
    MoveDownAndModifySelection,
    /// Move backward by one word.
    MoveWordBackward,
    /// Extend the selection while moving backward by one word.
    MoveWordBackwardAndModifySelection,
    /// Move forward by one word.
    MoveWordForward,
    /// Extend the selection while moving forward by one word.
    MoveWordForwardAndModifySelection,
    /// Move one word to the visual left.
    MoveWordLeft,
    /// Extend the selection while moving one word to the visual left.
    MoveWordLeftAndModifySelection,
    /// Move one word to the visual right.
    MoveWordRight,
    /// Extend the selection while moving one word to the visual right.
    MoveWordRightAndModifySelection,
    /// Move to the beginning of the current line.
    MoveToBeginningOfLine,
    /// Extend the selection while moving to the beginning of the current line.
    MoveToBeginningOfLineAndModifySelection,
    /// Move to the end of the current line.
    MoveToEndOfLine,
    /// Extend the selection while moving to the end of the current line.
    MoveToEndOfLineAndModifySelection,
    /// Move to the left edge of the current line.
    MoveToLeftEndOfLine,
    /// Extend the selection while moving to the left edge of the current line.
    MoveToLeftEndOfLineAndModifySelection,
    /// Move to the right edge of the current line.
    MoveToRightEndOfLine,
    /// Extend the selection while moving to the right edge of the current line.
    MoveToRightEndOfLineAndModifySelection,
    /// Move to the beginning of the current paragraph.
    MoveToBeginningOfParagraph,
    /// Extend the selection while moving to the beginning of the current paragraph.
    MoveToBeginningOfParagraphAndModifySelection,
    /// Move to the end of the current paragraph.
    MoveToEndOfParagraph,
    /// Extend the selection while moving to the end of the current paragraph.
    MoveToEndOfParagraphAndModifySelection,
    /// Move one paragraph backward while extending the selection.
    MoveParagraphBackwardAndModifySelection,
    /// Move one paragraph forward while extending the selection.
    MoveParagraphForwardAndModifySelection,
    /// Move to the beginning of the document.
    MoveToBeginningOfDocument,
    /// Extend the selection while moving to the beginning of the document.
    MoveToBeginningOfDocumentAndModifySelection,
    /// Move to the end of the document.
    MoveToEndOfDocument,
    /// Extend the selection while moving to the end of the document.
    MoveToEndOfDocumentAndModifySelection,
    /// Move one page upward.
    PageUp,
    /// Extend the selection while moving one page upward.
    PageUpAndModifySelection,
    /// Move one page downward.
    PageDown,
    /// Extend the selection while moving one page downward.
    PageDownAndModifySelection,
    /// Delete the unit before the insertion point.
    ///
    /// This is a deliberate overlap with
    /// [`crate::text::TextInputEvent::DeleteBackward`] for platforms that
    /// report plain deletion as a resolved editor command.
    DeleteBackward,
    /// Delete backward while decomposing the previous character cluster.
    DeleteBackwardByDecomposingPreviousCharacter,
    /// Delete the unit after the insertion point.
    ///
    /// This is a deliberate overlap with
    /// [`crate::text::TextInputEvent::DeleteForward`] for platforms that report
    /// plain deletion as a resolved editor command.
    DeleteForward,
    /// Delete the previous word.
    DeleteWordBackward,
    /// Delete the next word.
    DeleteWordForward,
    /// Delete to the beginning of the current line.
    DeleteToBeginningOfLine,
    /// Delete to the beginning of the current paragraph.
    DeleteToBeginningOfParagraph,
    /// Delete to the end of the current line.
    DeleteToEndOfLine,
    /// Delete to the end of the current paragraph.
    DeleteToEndOfParagraph,
    /// Insert a back-tab.
    InsertBacktab,
    /// Insert a line break.
    InsertLineBreak,
    /// Insert a newline.
    InsertNewline,
    /// Insert a paragraph separator.
    InsertParagraphSeparator,
    /// Insert a tab.
    InsertTab,
    /// Insert a literal double quote without substitution.
    InsertDoubleQuoteIgnoringSubstitution,
    /// Insert a literal single quote without substitution.
    InsertSingleQuoteIgnoringSubstitution,
    /// Select all content.
    SelectAll,
    /// Select the current line.
    SelectLine,
    /// Select the current paragraph.
    SelectParagraph,
    /// Select the current word.
    SelectWord,
    /// Scroll one line upward.
    ScrollLineUp,
    /// Scroll one line downward.
    ScrollLineDown,
    /// Scroll one page upward.
    ScrollPageUp,
    /// Scroll one page downward.
    ScrollPageDown,
    /// Scroll to the beginning of the document.
    ScrollToBeginningOfDocument,
    /// Scroll to the end of the document.
    ScrollToEndOfDocument,
    /// Center the selection in the visible area.
    CenterSelectionInVisibleArea,
    /// Transpose adjacent units.
    Transpose,
    /// Capitalize the current word.
    CapitalizeWord,
    /// Lowercase the current word.
    LowercaseWord,
    /// Uppercase the current word.
    UppercaseWord,
    /// Request completion for the current editing context.
    Complete,
    /// Cancel the current editing operation.
    CancelOperation,
}

#[cfg(test)]
mod tests {
    use super::EditCommandEvent;

    #[test]
    fn edit_commands_are_copy_and_comparable() {
        let command = EditCommandEvent::MoveWordForwardAndModifySelection;
        let copied = command;
        assert_eq!(copied, EditCommandEvent::MoveWordForwardAndModifySelection);
    }

    #[test]
    fn plain_delete_commands_are_deliberate_overlap() {
        assert_eq!(
            EditCommandEvent::DeleteBackward,
            EditCommandEvent::DeleteBackward
        );
        assert_eq!(
            EditCommandEvent::DeleteForward,
            EditCommandEvent::DeleteForward
        );
    }
}
