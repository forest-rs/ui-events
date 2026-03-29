// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use ui_events::pointer::{PersistentDeviceId, PointerId, PointerInfo, PointerType};

/// Offset applied to platform pointer identifiers before constructing
/// [`PointerId`] or [`PersistentDeviceId`] values.
///
/// `ui-events` reserves [`PointerId::PRIMARY`] with the underlying value `1`.
/// Offsetting platform id `0` by `2` makes the first platform-derived id `2`,
/// avoiding collisions with the synthetic primary pointer id.
pub const PLATFORM_POINTER_ID_OFFSET: u64 = 2;

/// Build a [`PointerInfo`] from a platform pointer identifier.
pub fn pointer_info_from_platform_pointer_id(
    pointer_type: PointerType,
    pointer_id: u64,
) -> PointerInfo {
    let pointer_id = pointer_id
        .checked_add(PLATFORM_POINTER_ID_OFFSET)
        .and_then(PointerId::new);
    PointerInfo {
        pointer_id,
        persistent_device_id: None,
        pointer_type,
    }
}

/// Build a [`PointerInfo`] from platform pointer and device identifiers.
pub fn pointer_info_from_platform_ids(
    pointer_type: PointerType,
    pointer_id: Option<u64>,
    device_id: Option<u64>,
) -> PointerInfo {
    let pointer_id = pointer_id
        .and_then(|id| id.checked_add(PLATFORM_POINTER_ID_OFFSET))
        .and_then(PointerId::new);
    let persistent_device_id = device_id
        .and_then(|id| id.checked_add(PLATFORM_POINTER_ID_OFFSET))
        .and_then(PersistentDeviceId::new);
    PointerInfo {
        pointer_id,
        persistent_device_id,
        pointer_type,
    }
}

/// Build a [`PointerInfo`] for the given [`PointerType`] with a primary id.
pub fn pointer_info_primary_for_type(pointer_type: PointerType) -> PointerInfo {
    PointerInfo {
        pointer_id: Some(PointerId::PRIMARY),
        persistent_device_id: None,
        pointer_type,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_pointer_id_zero_does_not_collide_with_primary() {
        let pointer = pointer_info_from_platform_pointer_id(PointerType::Pen, 0);
        assert_eq!(
            pointer
                .pointer_id
                .expect("platform ids should be assigned")
                .get_inner()
                .get(),
            PLATFORM_POINTER_ID_OFFSET
        );
        assert_ne!(pointer.pointer_id, Some(PointerId::PRIMARY));
        assert!(!pointer.is_primary_pointer());
    }

    #[test]
    fn platform_pointer_and_device_ids_zero_do_not_collide_with_primary() {
        let pointer = pointer_info_from_platform_ids(PointerType::Pen, Some(0), Some(0));
        assert_eq!(
            pointer
                .pointer_id
                .expect("platform ids should be assigned")
                .get_inner()
                .get(),
            PLATFORM_POINTER_ID_OFFSET
        );
        assert_eq!(
            pointer.persistent_device_id,
            PersistentDeviceId::new(PLATFORM_POINTER_ID_OFFSET)
        );
        assert_ne!(pointer.pointer_id, Some(PointerId::PRIMARY));
    }
}
