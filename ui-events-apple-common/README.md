<div align="center">

# UI Events Apple Common

Shared `no_std` mapping helpers used by the AppKit and UIKit adapters.

[![Linebender Zulip, #general channel](https://img.shields.io/badge/Linebender-%23general-blue?logo=Zulip)](https://xi.zulipchat.com/#narrow/channel/147921-general)
[![dependency status](https://deps.rs/repo/github/endoli/ui-events/status.svg)](https://deps.rs/repo/github/endoli/ui-events)
[![Apache 2.0 or MIT license.](https://img.shields.io/badge/license-Apache--2.0_OR_MIT-blue.svg)](#license)
[![Build status](https://github.com/endoli/ui-events/workflows/CI/badge.svg)](https://github.com/endoli/ui-events/actions)
[![Crates.io](https://img.shields.io/crates/v/ui-events-apple-common.svg)](https://crates.io/crates/ui-events-apple-common)
[![Docs](https://docs.rs/ui-events-apple-common/badge.svg)](https://docs.rs/ui-events-apple-common)

</div>

<!-- We use cargo-rdme to update the README with the contents of lib.rs.
To edit the following section, update it in lib.rs, then run:
cargo rdme --workspace-project=ui-events-apple-common --heading-base-level=0
Full documentation at https://github.com/orium/cargo-rdme -->

<!-- Intra-doc links used in lib.rs should be evaluated here.
See https://linebender.org/blog/doc-include/ for related discussion. -->
[`ui-events`]: https://docs.rs/ui-events/
<!-- cargo-rdme start -->

Shared `no_std` mapping helpers for Apple platform adapters.

This crate owns platform-neutral construction of raw [`ui-events`] building
blocks from values extracted by the AppKit and UIKit adapter crates. It
intentionally does not depend on AppKit, UIKit, or `objc2`, and it does not
decide which platform callbacks should become which high-level events.

## Feature Policy

- `std` is enabled by default.
- `libm` is accepted as a no-op compatibility feature for the workspace
  `no_std` check matrix.

## Helper Groups

- Pointer identity helpers reserve [`PointerId::PRIMARY`] and map platform
  ids through [`PLATFORM_POINTER_ID_OFFSET`].
- Button helpers translate platform button indexes and bitmasks into
  [`PointerButton`] and [`PointerButtons`].
- Modifier helpers build [`Modifiers`] from platform modifier bits.
- State helpers build [`PointerState`] and [`ScrollDelta`] from finite raw
  values already extracted by an adapter crate.

These helpers are intentionally small and value-based. AppKit and UIKit
crates still own all Objective-C selector access and platform-specific event
routing.

[`Modifiers`]: ui_events::keyboard::Modifiers
[`PointerButton`]: ui_events::pointer::PointerButton
[`PointerButtons`]: ui_events::pointer::PointerButtons
[`PointerId::PRIMARY`]: ui_events::pointer::PointerId::PRIMARY
[`PointerState`]: ui_events::pointer::PointerState
[`ScrollDelta`]: ui_events::ScrollDelta
[`ui-events`]: https://docs.rs/ui-events/

<!-- cargo-rdme end -->

## Minimum supported Rust Version (MSRV)

This version of UI Events Apple Common has been verified to compile with **Rust 1.85** and later.

Future versions of UI Events Apple Common might increase the Rust version requirement.
It will not be treated as a breaking change and as such can even happen with small patch releases.

<details>
<summary>Click here if compiling fails.</summary>

As time has passed, some UI Events Apple Common dependencies could have released versions with a higher Rust requirement.
If you encounter a compilation issue due to a dependency and don't want to upgrade your Rust toolchain, then you could downgrade the dependency.

```sh
# Use the problematic dependency's name and version
cargo update -p package_name --precise 0.1.1
```

</details>

## Community

[![Linebender Zulip](https://img.shields.io/badge/Xi%20Zulip-%23general-blue?logo=Zulip)](https://xi.zulipchat.com/#narrow/channel/147921-general)

Discussion of UI Events Apple Common development happens in the [Linebender Zulip](https://xi.zulipchat.com/), specifically the [#general channel](https://xi.zulipchat.com/#narrow/channel/147921-general).
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
