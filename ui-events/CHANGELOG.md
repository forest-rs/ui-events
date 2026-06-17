<!-- Instructions

This changelog follows the patterns described here: <https://keepachangelog.com/en/>.

Subheadings to categorize changes are `added, changed, deprecated, removed, fixed, security`.

-->

# Changelog

The latest published UI Events release is [0.3.0](#030-2026-01-18) which was released on 2026-01-18.
You can find its changes [documented below](#030-2026-01-18).

## [Unreleased]

This release has an [MSRV][] of 1.85.

### Added
* Added `ui_events::text` with `TextInputEvent`, `TextInsertEvent`, `CompositionState`, and explicit range metadata for soft-keyboard and IME input.
* Added `ui_events::edit` with `EditCommandEvent` for semantic editor commands such as movement, deletion, and selection changes.
* Added explicit text-input events for selection updates, composing regions, surrounding deletion, editor actions, and document-range conversion helpers.
* Added explicit cursor-placement metadata for text insertion and composition updates.
* Added `TextInputEvent` convenience constructors for common insert, replace, composition, selection, delete-surrounding, and action cases.

### Changed

## [0.3.0][] - 2026-01-18

This release has an [MSRV][] of 1.85.

### Added

* `PointerId::get_inner()` method to access the inner `NonZeroU64` value. ([#90][] by [@jrmoulton][])
* `ScrollDelta::{to_pixel_delta, into_pixel_delta}` helpers for converting line/page scroll deltas into pixels using caller-provided scaling. ([#101][] by [@waywardmonkeys][])

### Changed

* Clarified the semantics of gesture deltas. ([#100][] by [@waywardmonkeys][])
* Updated Kurbo to [v0.13.0](https://github.com/linebender/kurbo/releases/tag/v0.13.0). ([#105][] by [@AustinMReppert][])
* The `std` and `libm` features no longer enable the optional `kurbo` dependency; enable the `kurbo` feature explicitly if you want the dependency. ([#104][] by [@waywardmonkeys][])
* Bumped the MSRV to 1.85. ([#107][] by [@waywardmonkeys][])

## [0.2.0][] - 2025-10-28

This release has an [MSRV][] of 1.82.

### Added

* `PointerId`, `PointerInfo`, `PointerUpdate`, and `PointerEvent` now have an `is_primary_pointer` method. ([#54][] by [@waywardmonkeys][])
* `PointerGesture` and `PointerGestureEvent` types, with `Gesture` variant added to `PointerEvent`. ([#80][] by [@xorgy][] and [@arthur-fontaine][])
* `scale_factor` field to `EventState` for convenient conversion between logical and device pixels ([#82][] by [@jrmoulton][] and [@xorgy][]).
  * Optional `kurbo` integration behind the `kurbo` Cargo feature with convenience helpers for converting DPI position to `kurbo::Point`.

### Changed

* Convert `PointerEvent` struct variants (`Down`, `Up`, `Scroll`) to separate structs. ([#63][] by [@nicoburns][])

## [0.1.0][] - 2025-05-08

This release has an [MSRV][] of 1.73.

This is the initial release.


[@arthur-fontaine]: https://github.com/arthur-fontaine
[@AustinMReppert]: https://github.com/AustinMReppert
[@jrmoulton]: https://github.com/jrmoulton
[@nicoburns]: https://github.com/nicoburns
[@waywardmonkeys]: https://github.com/waywardmonkeys
[@xorgy]: https://github.com/xorgy

[#54]: https://github.com/endoli/ui-events/pull/54
[#63]: https://github.com/endoli/ui-events/pull/63
[#80]: https://github.com/endoli/ui-events/pull/80
[#82]: https://github.com/endoli/ui-events/pull/82
[#90]: https://github.com/endoli/ui-events/pull/90
[#100]: https://github.com/endoli/ui-events/pull/100
[#101]: https://github.com/endoli/ui-events/pull/101
[#104]: https://github.com/endoli/ui-events/pull/104
[#105]: https://github.com/endoli/ui-events/pull/105
[#107]: https://github.com/endoli/ui-events/pull/107

[Unreleased]: https://github.com/endoli/ui-events/compare/v0.3.0...HEAD
[0.1.0]: https://github.com/endoli/ui-events/releases/tag/v0.1.0
[0.2.0]: https://github.com/endoli/ui-events/releases/tag/v0.2.0
[0.3.0]: https://github.com/endoli/ui-events/releases/tag/v0.3.0

[MSRV]: README.md#minimum-supported-rust-version-msrv
