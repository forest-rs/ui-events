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
cargo rdme --workspace-project=ui-events-windows --heading-base-level=0
Full documentation at https://github.com/orium/cargo-rdme -->

<!-- Intra-doc links used in lib.rs should be evaluated here.
See https://linebender.org/blog/doc-include/ for related discussion. -->
[`ui-events`]: https://docs.rs/ui-events/
[Win32 API]: https://learn.microsoft.com/en-us/windows/win32/api/
<!-- cargo-rdme start -->

This crate bridges the raw [Win32 API] window messages (mouse, touch, keyboard, IME, etc.)
into the [`ui-events`] model.

The primary entry point is [`EventReducer`].

Call [`EventReducer::reduce`] with nanoseconds in the host clock domain so input, timers,
frame sampling, submission timestamps, and diagnostics can share one timeline.
The timestamp must be real monotonic nanoseconds, not milliseconds, microseconds, frame counts,
or a constant value. Tap counting uses it for nanosecond-duration thresholds.

[`EventReducer::reduce`] returns a `Vec` of zero or more translations.
A single raw Win32 message can produce more than one normalized event (for example,
the first `WM_MOUSEMOVE` after the cursor entered the window produces a synthetic
`PointerEvent::Enter` followed by the `Move`, and a single `WM_TOUCH` message can carry
more than one simultaneous touch point).

This crate also handles some side-effecting Win32 calls:

- It calls `TrackMouseEvent` on mouse enter so that `WM_MOUSELEAVE` is delivered.
- It calls `SetCapture`/`ReleaseCapture` around button presses so that a drag that leaves
    the window still delivers its button-up.

<!-- cargo-rdme end -->

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
