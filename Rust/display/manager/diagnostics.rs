//! Exact diagnostics operations of the parent owner; no duplicated state.
use super::*;

pub(super) unsafe extern "C" fn rejected_commit(_: usize, _: u64, _: *const BootSnapshot) -> u32 {
    1
}

pub(super) unsafe extern "C" fn rejected_restore(_: usize, _: u64, _: *const BootSnapshot) -> bool {
    false
}

pub fn probe_rejected_takeover() -> bool {
    let Some(boot) = (unsafe { (*context()).saved_boot }) else {
        return false;
    };
    let before = backend_state();
    if before.state != backend::BOOTFB || before.policy != 0 {
        return false;
    }
    let owner = u32::MAX as usize;
    let mut frame = boot.framebuffer;
    let mapping = Mapping {
        kind: 2,
        ..boot.mapping
    };
    let candidate = Backend {
        owner,
        adapter_id: u32::MAX,
        target: Target {
            name: Text::literal(b"rejection-probe"),
            kind: NATIVE,
            flags: CPU_PRESENT,
            framebuffer: &mut frame,
            mode: boot.mode,
            mapping,
        },
        context: 0,
        commit: Commit(rejected_commit),
        restore: Restore(rejected_restore),
        begin_cpu: None,
        end_cpu: None,
    };
    let Ok(generation) = prepare_native(candidate) else {
        return false;
    };
    let result = match commit_native(owner, generation) {
        Ok(value) => value,
        Err(_) => {
            let _ = abort_native(owner, generation);
            return false;
        }
    };
    let after = backend_state();
    result == 1
        && after.state == backend::BOOTFB
        && after.reason == 5
        && after.generation == before.generation
        && !retains_driver_owner(owner)
        && !firmware::r4os_rust_display_firmware_gate.is_revoked()
}

pub fn operation_names(flags: u32) -> &'static [u8] {
    if flags & 128 != 0 {
        b"present/blit/fill/rect/xrgb32"
    } else if flags & 64 != 0 {
        b"present/blit/fill/rect/packed32"
    } else if flags & 32 != 0 {
        b"fill/rect"
    } else {
        b"none"
    }
}

pub fn present_reason_name(reason: u8) -> &'static [u8] {
    match reason {
        0 => b"none",
        1 => b"fill",
        2 => b"rect",
        3 => b"packed32-present",
        4 => b"xrgb32-present",
        _ => panic!("present reason enum"),
    }
}

pub fn mapping_kind_name(kind: u8) -> &'static [u8] {
    match kind {
        0 => b"none",
        1 => b"bootloader-framebuffer",
        2 => b"native-scanout",
        _ => panic!("mapping kind enum"),
    }
}

pub fn cache_policy_name(policy: u8) -> &'static [u8] {
    match policy {
        0 => b"unknown",
        1 => b"bootloader-default",
        2 => b"pat-write-combining",
        3 => b"write-combining-unsupported",
        4 => b"write-combining-failed",
        _ => panic!("cache policy enum"),
    }
}
