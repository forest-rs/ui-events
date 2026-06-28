<div align="center">

# UI Events for Windows

A library for bridging the raw [Win32 API] events into the [`ui-events`] model.

[![Linebender Zulip, #general channel](https://img.shields.io/badge/Linebender-%23general-blue?logo=Zulip)](https://xi.zulipchat.com/#narrow/channel/147921-general)
[![dependency status](https://deps.rs/repo/github/endoli/ui-events/status.svg)](https://deps.rs/repo/github/endoli/ui-events)
[![Apache 2.0 or MIT license.](https://img.shields.io/badge/license-Apache--2.0_OR_MIT-blue.svg)](#license)
[![Build status](https://github.com/endoli/ui-events/workflows/CI/badge.svg)](https://github.com/endoli/ui-events/actions)
[![Crates.io](https://img.shields.io/crates/v/ui-events-windows.svg)](https://crates.io/crates/ui-events-windows)
[![Docs](https://docs.rs/ui-events-windows/badge.svg)](https://docs.rs/ui-events-windows)

</div>

<!-- We use cargo-rdme to update the README with the contents of lib.rs.
To edit the following section, update it in lib.rs, then run:
cargo rdme --workspace-project=ui-events --heading-base-level=0
Full documentation at https://github.com/orium/cargo-rdme -->

<!-- Intra-doc links used in lib.rs should be evaluated here.
See https://linebender.org/blog/doc-include/ for related discussion. -->
[`ui-events`]: https://docs.rs/ui-events/
[`ui-events-winit`]: https://docs.rs/ui-events-winit/
[Win32 API]: https://learn.microsoft.com/en-us/windows/win32/api/
[`winit-win32`]: https://github.com/rust-windowing/winit
[`WindowEventReducer`]: https://docs.rs/ui-events-windows/latest/ui_events_windows/struct.WindowEventReducer.html
<!-- cargo-rdme start -->

This crate bridges the raw [Win32 API] window messages (mouse, touch,
keyboard, IME, etc.) into the [`ui-events`] model.

It is a Windows-native sibling of [`ui-events-winit`]: instead of
converting `winit`'s `WindowEvent`s, it converts the `WM_*` messages a
`WNDPROC` receives directly, inlining the same Win32-API-to-normalized-event
conversions that the [`winit-win32`] backend performs internally to build
its `winit` events in the first place.

The primary entry point is [`WindowEventReducer`].

Call [`WindowEventReducer::reduce`] with nanoseconds in the host clock
domain so input, timers, frame sampling, submission timestamps, and
diagnostics can share one timeline.
The timestamp must be real monotonic nanoseconds, not milliseconds,
microseconds, frame counts, or a constant value; tap counting uses
nanosecond-duration thresholds.

Unlike [`ui-events-winit`]'s reducer, [`WindowEventReducer::reduce`] here
returns a `Vec` of zero or more translations rather than a single
`Option`: a single raw Win32 message can legitimately produce more than
one normalized event (for example, the first `WM_MOUSEMOVE` after the
cursor entered the window produces a synthetic `PointerEvent::Enter`
followed by the `Move`, and a single `WM_TOUCH` message can carry more
than one simultaneous touch point).

This crate also takes on a few side-effecting Win32 calls that `winit`'s
windowing layer would otherwise be responsible for, since there is no
such layer here: it calls `TrackMouseEvent` so that `WM_MOUSELEAVE` is
delivered, and it calls `SetCapture`/`ReleaseCapture` around button
presses so that a drag that leaves the window still delivers its button-up.

All of this crate's functionality requires the `windows` target family;
on any other target, this crate still compiles, but is empty, the same
way `ui-events-appkit`'s `objc2`-dependent items are gated to
`target_os = "macos"`.

[`ui-events`]: https://docs.rs/ui-events/
[`ui-events-winit`]: https://docs.rs/ui-events-winit/
[Win32 API]: https://learn.microsoft.com/en-us/windows/win32/api/
[`winit-win32`]: https://github.com/rust-windowing/winit

<!-- cargo-rdme end -->

## Fidelity relative to `winit-win32`

This crate inlines the scancode-to-[`Code`], virtual-key-to-[`NamedKey`],
button, and IME-composition-string conversions straight from `winit-win32`'s
internals, but it deliberately simplifies a few things that would otherwise
require carrying along much more of `winit`'s windowing-layer state machine:

- Logical-key character resolution calls `ToUnicode` directly per
  `WM_KEYDOWN`/`WM_KEYUP`, rather than reproducing `winit`'s
  `PeekMessage`-driven `WM_KEYDOWN`/`WM_DEADCHAR`/`WM_CHAR` sequencing used to
  combine dead keys with the keystroke that follows them.
- Mouse-wheel line/character counts always use the Windows default of three
  lines per notch, rather than respecting the user's "Mouse Properties" wheel
  speed via `SystemParametersInfoW`.
- Mouse capture during a button drag is acquired and released per up/down
  message rather than reference-counted across multiple simultaneously held
  buttons.

[`Code`]: https://docs.rs/ui-events/latest/ui_events/keyboard/enum.Code.html
[`NamedKey`]: https://docs.rs/ui-events/latest/ui_events/keyboard/enum.NamedKey.html

## Minimum supported Rust Version (MSRV)

This version of UI Events for Windows has been verified to compile with **Rust 1.85** and later.

Future versions of UI Events for Windows might increase the Rust version requirement.
It will not be treated as a breaking change and as such can even happen with small patch releases.

<details>
<summary>Click here if compiling fails.</summary>

As time has passed, some of UI Events for Windows's dependencies could have released versions with a higher Rust requirement.
If you encounter a compilation issue due to a dependency and don't want to upgrade your Rust toolchain, then you could downgrade the dependency.

```sh
# Use the problematic dependency's name and version
cargo update -p package_name --precise 0.1.1
```

</details>

## Community

[![Linebender Zulip](https://img.shields.io/badge/Xi%20Zulip-%23general-blue?logo=Zulip)](https://xi.zulipchat.com/#narrow/channel/147921-general)

Discussion of UI Events for Windows development happens in the [Linebender Zulip](https://xi.zulipchat.com/), specifically the [#general channel](https://xi.zulipchat.com/#narrow/channel/147921-general).
All public content can be read without logging in.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

## Contribution

Contributions are welcome by pull request. The [Rust code of conduct] applies.
Please feel free to add your name to the [AUTHORS] file in any substantive pull request.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be licensed as above, without any additional terms or conditions.

[Rust Code of Conduct]: https://www.rust-lang.org/policies/code-of-conduct
[AUTHORS]: ./AUTHORS
