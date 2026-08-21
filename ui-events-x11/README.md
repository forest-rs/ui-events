<div align="center">

# UI Events X11 Adapter

A library for bridging X11 (XInput 2 + XKB) input events into the [`ui-events`] model.

[![Linebender Zulip, #general channel](https://img.shields.io/badge/Linebender-%23general-blue?logo=Zulip)](https://xi.zulipchat.com/#narrow/channel/147921-general)
[![dependency status](https://deps.rs/repo/github/endoli/ui-events/status.svg)](https://deps.rs/repo/github/endoli/ui-events)
[![Apache 2.0 or MIT license.](https://img.shields.io/badge/license-Apache--2.0_OR_MIT-blue.svg)](#license)
[![Build status](https://github.com/endoli/ui-events/workflows/CI/badge.svg)](https://github.com/endoli/ui-events/actions)
[![Crates.io](https://img.shields.io/crates/v/ui-events-x11.svg)](https://crates.io/crates/ui-events-x11)
[![Docs](https://docs.rs/ui-events-x11/badge.svg)](https://docs.rs/ui-events-x11)

</div>

<!-- We use cargo-rdme to update the README with the contents of lib.rs.
To edit the following section, update it in lib.rs, then run:
cargo rdme --workspace-project=ui-events-x11 --heading-base-level=0
Full documentation at https://github.com/orium/cargo-rdme -->

<!-- Intra-doc links used in lib.rs should be evaluated here.
See https://linebender.org/blog/doc-include/ for related discussion. -->
[`ui-events`]: https://docs.rs/ui-events/
<!-- cargo-rdme start -->

Bridges X11 input into the [`ui-events`] model.

Input goes through XInput 2 and XKB.
There is no core-protocol fallback.
The crate owns no connection: select XInput 2 events, pump the queue, and
pass each decoded [`x11rb` event] to the [`keyboard`], [`pointer`],
[`touch`], [`gesture`], and [`xim`] reducers.
Pointer, touch, and gesture take a scale factor and a monotonic nanosecond
timestamp in the host clock.
[`xim`] is an XIM client on the same connection.
[`mapping`] holds the plain-value conversions.

Physical key codes need no system library.
The `xkb` feature links `libxkbcommon` (via [`ui-events-xkb`]) for logical
keys, text, and full modifier state from a keymap.

`x11rb` defaults to pure-Rust [`RustConnection`].
The `xcb` feature switches to [`XCBConnection`] for sharing an existing XCB
connection; `dl-libxcb` loads `libxcb` at run time.

[`RustConnection`]: https://docs.rs/x11rb/latest/x11rb/rust_connection/struct.RustConnection.html
[`XCBConnection`]: https://docs.rs/x11rb/latest/x11rb/xcb_ffi/struct.XCBConnection.html
[`x11rb` event]: https://docs.rs/x11rb/latest/x11rb/protocol/enum.Event.html
[`pointer`]: https://docs.rs/ui-events-x11/latest/ui_events_x11/pointer/
[`gesture`]: https://docs.rs/ui-events-x11/latest/ui_events_x11/gesture/
[`xim`]: https://docs.rs/ui-events-x11/latest/ui_events_x11/xim/
[`ui-events-xkb`]: https://docs.rs/ui-events-xkb/
[`ui-events`]: https://docs.rs/ui-events/

<!-- cargo-rdme end -->

## Minimum supported Rust Version (MSRV)

This version of UI Events X11 has been verified to compile with **Rust 1.85** and later.

Future versions of UI Events X11 might increase the Rust version requirement.
It will not be treated as a breaking change and as such can even happen with small patch releases.

<details>
<summary>Click here if compiling fails.</summary>

As time has passed, some of UI Events X11's dependencies could have released versions with a higher Rust requirement.
If you encounter a compilation issue due to a dependency and don't want to upgrade your Rust toolchain, then you could downgrade the dependency.

```sh
# Use the problematic dependency's name and version
cargo update -p package_name --precise 0.1.1
```

</details>

## Community

[![Linebender Zulip](https://img.shields.io/badge/Xi%20Zulip-%23general-blue?logo=Zulip)](https://xi.zulipchat.com/#narrow/channel/147921-general)

Discussion of UI Events X11 development happens in the [Linebender Zulip](https://xi.zulipchat.com/), specifically the [#general channel](https://xi.zulipchat.com/#narrow/channel/147921-general).
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
