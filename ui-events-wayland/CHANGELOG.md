<!-- Instructions

This changelog follows the patterns described here: <https://keepachangelog.com/en/>.

Subheadings to categorize changes are `added, changed, deprecated, removed, fixed, security`.

-->

# Changelog

UI Events Wayland has not yet been published.

## [Unreleased]

This release has an [MSRV][] of 1.85.

### Added

- Platform-neutral Wayland input mapping helpers in the `mapping` module:
  evdev pointer-button mapping, surface-local coordinate scaling, scroll-axis
  frame to `ScrollDelta` conversion, pointer identity helpers, and modifier
  helpers.

[Unreleased]: https://github.com/endoli/ui-events/compare/v0.3.0...HEAD

[MSRV]: README.md#minimum-supported-rust-version-msrv
