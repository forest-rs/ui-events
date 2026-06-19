<div align="center">

# UI Events Wayland Adapter

A library for bridging Wayland input events into the [`ui-events`] model.

[![Linebender Zulip, #general channel](https://img.shields.io/badge/Linebender-%23general-blue?logo=Zulip)](https://xi.zulipchat.com/#narrow/channel/147921-general)
[![dependency status](https://deps.rs/repo/github/endoli/ui-events/status.svg)](https://deps.rs/repo/github/endoli/ui-events)
[![Apache 2.0 or MIT license.](https://img.shields.io/badge/license-Apache--2.0_OR_MIT-blue.svg)](#license)
[![Build status](https://github.com/endoli/ui-events/workflows/CI/badge.svg)](https://github.com/endoli/ui-events/actions)
[![Crates.io](https://img.shields.io/crates/v/ui-events-wayland.svg)](https://crates.io/crates/ui-events-wayland)
[![Docs](https://docs.rs/ui-events-wayland/badge.svg)](https://docs.rs/ui-events-wayland)

</div>

<!-- We use cargo-rdme to update the README with the contents of lib.rs.
To edit the following section, update it in lib.rs, then run:
cargo rdme --workspace-project=ui-events-wayland --heading-base-level=0
Full documentation at https://github.com/orium/cargo-rdme -->

<!-- Intra-doc links used in lib.rs should be evaluated here.
See https://linebender.org/blog/doc-include/ for related discussion. -->
[`ui-events`]: https://docs.rs/ui-events/
[`PointerState`]: https://docs.rs/ui-events/latest/ui_events/pointer/struct.PointerState.html
<!-- cargo-rdme start -->

Bridges Wayland input into the [`ui-events`] model.

This crate is Linux-only in practice. The `wayland-client` protocol bindings
it builds on are depended upon only on `cfg(target_os = "linux")` targets,
and the stateful reducers that consume them are gated the same way. On every
other target the crate compiles to an essentially empty library, so it can
live in a cross-platform workspace without pulling in platform dependencies.

## Architecture

- [`mapping`] holds platform-neutral, value-based conversions from Wayland
  input primitives (evdev button codes, surface-local coordinates, axis
  values, modifier booleans) into [`ui-events`] building blocks. It is
  always compiled, performs no foreign-function calls, and references no
  `wayland-client` types, so its unit tests run on every target.
- Stateful reducers that consume `wayland-client` event streams are gated to
  `cfg(target_os = "linux")` and build on these helpers.

## Coordinates, scale, and time

Wayland delivers surface-local pointer coordinates as logical (post-scale)
`f64` values. Callers supply a `scale_factor` (for example from
`wp_fractional_scale_v1`'s preferred scale divided by 120, or an output's
integer scale), and these helpers multiply by it to produce the physical
pixels [`ui-events`] expects.

Reducers built on these helpers take a caller-provided monotonic nanosecond
timestamp for the pointer [`PointerState`], rather than Wayland's 32-bit
millisecond event timestamps, so input shares one clock domain with frame
sampling and timers.

## Resolving logical keys

The keyboard reducer resolves the physical key code of every key event with
no system dependencies. Enabling the non-default `xkb` feature additionally
links `libxkbcommon` and uses the compositor's keymap to resolve logical key
values, typed text, and the authoritative modifier set (including the lock
states and Alt Graph).

## Feature policy

- `std` is enabled by default.
- `no_std` builds require `libm`, which supplies the trigonometry the tablet
  and touch mapping use.
- `xkb` reads the compositor keymap through `std::fs`, so it implies `std`.

[`PointerState`]: ui_events::pointer::PointerState
[`ui-events`]: https://docs.rs/ui-events/

<!-- cargo-rdme end -->

## Minimum supported Rust Version (MSRV)

This version of UI Events Wayland has been verified to compile with **Rust 1.85** and later.

Future versions of UI Events Wayland might increase the Rust version requirement.
It will not be treated as a breaking change and as such can even happen with small patch releases.

<details>
<summary>Click here if compiling fails.</summary>

As time has passed, some UI Events Wayland dependencies could have released versions with a higher Rust requirement.
If you encounter a compilation issue due to a dependency and don't want to upgrade your Rust toolchain, then you could downgrade the dependency.

```sh
# Use the problematic dependency's name and version
cargo update -p package_name --precise 0.1.1
```

</details>

## Community

[![Linebender Zulip](https://img.shields.io/badge/Xi%20Zulip-%23general-blue?logo=Zulip)](https://xi.zulipchat.com/#narrow/channel/147921-general)

Discussion of UI Events Wayland development happens in the [Linebender Zulip](https://xi.zulipchat.com/), specifically the [#general channel](https://xi.zulipchat.com/#narrow/channel/147921-general).
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
