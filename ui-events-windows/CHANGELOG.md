<!-- Instructions

This changelog follows the patterns described here: <https://keepachangelog.com/en/1.0.0/>.

Subheadings to categorize changes are `Added, Changed, Fixed, Removed`.

-->

# Changelog

The latest published UI Events for Windows release is [0.3.0](#030---2025-09-04).

## [Unreleased]

### Added

- Initial release of `ui-events-windows`, a Windows-native (raw Win32 API)
  sibling of `ui-events-winit`, inlining `winit-win32`'s Win32-message
  conversions directly rather than going through `winit`.

## [0.3.0] - 2025-09-04

This is the first version of `ui-events-windows`; prior released versions
listed below are inherited from `ui-events-winit`, which this crate was
copied from.

[Unreleased]: https://github.com/endoli/ui-events/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/endoli/ui-events/releases/tag/v0.3.0
