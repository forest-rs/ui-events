// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

/// Whether a host handled a platform input event.
///
/// Apple responder adapters use this value to decide whether to stop routing
/// an event or pass it to the framework's normal responder chain.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum EventDisposition {
    /// The host handled the event, so native responder routing should stop.
    Handled,
    /// The host did not handle the event, so native responder routing should continue.
    #[default]
    Unhandled,
}

impl EventDisposition {
    /// Return whether the host handled the event.
    #[must_use]
    pub const fn is_handled(self) -> bool {
        matches!(self, Self::Handled)
    }
}

#[cfg(test)]
mod tests {
    use super::EventDisposition;

    #[test]
    fn unhandled_is_the_safe_default() {
        assert_eq!(EventDisposition::default(), EventDisposition::Unhandled);
        assert!(!EventDisposition::default().is_handled());
        assert!(EventDisposition::Handled.is_handled());
    }
}
