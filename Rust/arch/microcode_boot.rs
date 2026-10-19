//! Actual early preparation controller, with a typed hardware peer for source
//! qualification. Only the native entry supplies privileged instructions.
use super::{
    microcode_catalog::{PINS, Pin},
    microcode_format as format,
    microcode_policy::{self as policy, Decision, Status},
};
use crate::{
    contract as c,
    platform::cpu_capabilities_decode::{self as decode, Raw},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LoadResult {
    NotSubmitted,
    Written,
    Faulted,
}
pub(crate) trait Hardware {
    fn capture(&mut self) -> Raw;
    fn revision(&mut self, vendor: u32) -> Result<u32, ()>;
    fn platform(&mut self) -> Result<u32, ()>;
    fn apply(&mut self, pin: Pin, patch: format::Patch, file: &[u8]) -> LoadResult;
}
pub(crate) fn fingerprint(raw: &Raw) -> u64 {
    raw.encode()
        .iter()
        .fold(0xcbf29ce484222325u64, |hash, word| {
            word.to_le_bytes().iter().fold(hash, |h, byte| {
                (h ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
            })
        })
}
fn vendor(raw: &Raw) -> u32 {
    match &raw.vendor() {
        b"GenuineIntel" => 1,
        b"AuthenticAMD" => 2,
        _ => 0,
    }
}
fn signature(raw: &Raw) -> u32 {
    raw.get(1, 0).map_or(0, |v| v.eax)
}
fn revision(out: &mut c::CpuMicrocodeInfo, hw: &mut impl Hardware, raw: &Raw) -> Option<u32> {
    let supported = raw.get(1, 0).is_some_and(|v| v.edx & (1 << 5) != 0)
        && raw
            .signature()
            .is_some_and(|(family, _, _)| match out.vendor {
                1 => family == 6 || family == 15,
                2 => family >= 0x10,
                _ => false,
            });
    if !supported {
        return None;
    }
    match hw.revision(out.vendor) {
        Ok(value) => Some(value),
        Err(()) => {
            out.read_faults += 1;
            None
        }
    }
}
pub(crate) fn prepare(
    index: u32,
    initial: bool,
    bundle: Option<bool>,
    files: [&[u8]; 2],
    hw: &mut impl Hardware,
) -> c::CpuMicrocodeInfo {
    let before = hw.capture();
    let virtual_cpu = before.get(1, 0).is_some_and(|v| v.ecx & (1 << 31) != 0);
    let mut out = c::CpuMicrocodeInfo {
        cpu_index: index,
        vendor: vendor(&before),
        signature: signature(&before),
        before_features_hash: fingerprint(&before),
        ..Default::default()
    };
    if initial {
        out.flags |= c::cpu_microcode_flag_initial_boot;
    }
    if virtual_cpu {
        out.flags |= c::cpu_microcode_flag_virtual;
    }
    if bundle == Some(true) {
        out.flags |= c::cpu_microcode_flag_bundle_valid;
    }
    let pin = PINS
        .into_iter()
        .find(|p| p.vendor == out.vendor && p.signature == out.signature);
    if let Some(pin) = pin {
        out.package_id = pin.id;
        out.package_bytes = pin.bytes as u32;
        out.target_revision = pin.revision;
        out.minimum_revision = pin.minimum_revision;
        if bundle == Some(true) {
            out.package_sha256 = pin.sha256;
            out.flags |= c::cpu_microcode_flag_payload_verified;
        }
    }
    let before_revision = revision(&mut out, hw, &before);
    if let Some(value) = before_revision {
        out.before_revision = value;
        out.flags |= c::cpu_microcode_flag_before_valid;
    }
    let mut supported = pin.is_some();
    let mut platform_failed = false;
    if out.vendor == 1 && supported && !virtual_cpu {
        match hw.platform() {
            Ok(value) => {
                out.platform_mask = value;
                supported = pin.is_some_and(|p| {
                    p.platform_mask & value != 0 && value.is_power_of_two() && value <= 128
                });
            }
            Err(()) => {
                out.read_faults += 1;
                platform_failed = true;
                supported = false;
            }
        }
    }
    if supported && !virtual_cpu {
        out.flags |= c::cpu_microcode_flag_matched;
    }
    let decision = policy::decide(
        bundle,
        supported,
        virtual_cpu,
        before_revision,
        out.target_revision,
        out.minimum_revision,
        initial,
    );
    let mut status = match decision {
        Decision::Keep(value) => value,
        Decision::Load => Status::NotPrepared,
    };
    if platform_failed && !matches!(status, Status::PackageRejected) {
        status = Status::RevisionUnavailable;
    }
    if decision == Decision::Load {
        let pin = pin.expect("Load requires exact supported pin");
        let file = files[(pin.id - 1) as usize];
        let patch = if file.len() == pin.bytes {
            match pin.vendor {
                1 => format::intel(file, pin.signature, out.platform_mask),
                2 => format::amd(file, pin.signature),
                _ => unreachable!(),
            }
        } else {
            Err(format::Error::Format)
        };
        match patch {
            Ok(Some(patch)) if patch.revision == pin.revision => {
                let result = hw.apply(pin, patch, file);
                if result != LoadResult::NotSubmitted {
                    out.write_attempts = 1;
                    out.flags |= c::cpu_microcode_flag_update_attempt;
                }
                status = if result == LoadResult::Written {
                    Status::Updated
                } else {
                    Status::LoadFailed
                };
            }
            _ => status = Status::PackageRejected,
        }
    }
    let after = hw.capture();
    out.after_features_hash = fingerprint(&after);
    out.flags |= c::cpu_microcode_flag_post_features_fresh;
    if before.encode() != after.encode() {
        out.flags |= c::cpu_microcode_flag_features_changed;
    }
    let after_revision = revision(&mut out, hw, &after);
    if let Some(value) = after_revision {
        out.after_revision = value;
        out.flags |= c::cpu_microcode_flag_after_valid;
    }
    if status == Status::Updated && after_revision != Some(out.target_revision) {
        status = Status::RevisionMismatch;
    }
    if matches!(status, Status::AlreadyCurrent | Status::RestartVerified)
        && after_revision.is_none_or(|revision| revision < out.target_revision)
    {
        status = Status::RevisionMismatch;
    }
    if vendor(&after) != out.vendor || signature(&after) != out.signature {
        status = Status::IdentityChanged;
    }
    out.status = status as u32;
    out
}
pub(crate) fn failed(info: &c::CpuMicrocodeInfo) -> bool {
    policy::terminal_code(info.status)
}
pub(crate) fn admit(info: &mut c::CpuMicrocodeInfo, raw: &decode::Raw) -> bool {
    if failed(info) || vendor(raw) != info.vendor || signature(raw) != info.signature {
        return false;
    }
    info.admitted_features_hash = fingerprint(raw);
    info.flags |= c::cpu_microcode_flag_post_baseline;
    true
}
const _: () = {
    assert!(Status::PackageMissing as u32 == c::cpu_microcode_status_package_missing);
    assert!(Status::VirtualCpu as u32 == c::cpu_microcode_status_virtual_cpu);
    assert!(Status::Updated as u32 == c::cpu_microcode_status_updated);
    assert!(Status::FirmwarePrerequisite as u32 == c::cpu_microcode_status_firmware_prerequisite);
    assert!(
        Status::RestartRequiresUpdate as u32 == c::cpu_microcode_status_restart_requires_update
    );
    assert!(Status::IdentityChanged as u32 == c::cpu_microcode_status_identity_changed);
};
