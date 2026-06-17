<!-- Instructions

This changelog follows the patterns described here: <https://keepachangelog.com/en/>.

Subheadings to categorize changes are `added, changed, deprecated, removed, fixed, security`.

-->

# Changelog

The latest published UI Events Winit release is [0.3.0](#030-2026-01-18) which was released on 2026-01-18.
You can find its changes [documented below](#030-2026-01-18).

## [Unreleased]

This release has an [MSRV][] of 1.85.

### Changed

* Changed `WindowEventReducer::reduce` to require a caller-provided monotonic nanosecond timestamp. Hosts should pass their frame/timer clock timestamp so pointer events are emitted in the same clock domain as frame sampling and timers.

### Added

* Added `WindowEvent::Ime` translation to `ui_events::text::TextInputEvent`.

### Removed

* Removed the `ui_events_winit::Instant` re-export and the inert `std` feature. `ui-events-winit` no longer owns a reducer-local clock.

### Fixed

* Touch pointer states now preserve the reducer `scale_factor` instead of always reporting `1.0`.

## [0.3.0][] - 2026-01-18

This release has an [MSRV][] of 1.85.

### Fixed

* Replaced stale tap state for `pointer_id`. ([#92][] by [@xorgy][])

### Changed

* Bumped the MSRV to 1.85. ([#107][] by [@waywardmonkeys][])

## [0.2.0][] - 2025-10-28

This release has an [MSRV][] of 1.82.

### Added

* `PointerGesture` and `PointerGestureEvent` types, with `Gesture` variant added to `PointerEvent`. ([#80][] by [@xorgy][] and [@arthur-fontaine][])
* `scale_factor` parameter to `WindowEventReducer::reduce` for device-independent slop in tap detection. ([#78][] by [@xorgy][])

### Changed

* Convert `PointerEvent` struct variants (`Down`, `Up`, `Scroll`) to separate structs. ([#63][] by [@nicoburns][])
* Reduce allocations in `TapCounter`. ([#61][] by [@nicoburns][])

## [0.1.0][] - 2025-05-08

This release has an [MSRV][] of 1.73.

This is the initial release.


[@arthur-fontaine]: https://github.com/arthur-fontaine
[@nicoburns]: https://github.com/nicoburns
[@waywardmonkeys]: https://github.com/waywardmonkeys
[@xorgy]: https://github.com/xorgy

[#61]: https://github.com/endoli/ui-events/pull/61
[#63]: https://github.com/endoli/ui-events/pull/63
[#78]: https://github.com/endoli/ui-events/pull/78
[#80]: https://github.com/endoli/ui-events/pull/80
[#92]: https://github.com/endoli/ui-events/pull/92
[#107]: https://github.com/endoli/ui-events/pull/107

[Unreleased]: https://github.com/endoli/ui-events/compare/v0.3.0...HEAD
[0.1.0]: https://github.com/endoli/ui-events/releases/tag/v0.1.0
[0.2.0]: https://github.com/endoli/ui-events/releases/tag/v0.2.0
[0.3.0]: https://github.com/endoli/ui-events/releases/tag/v0.3.0

[MSRV]: README.md#minimum-supported-rust-version-msrv
