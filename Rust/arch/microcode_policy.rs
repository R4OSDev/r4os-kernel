//! Early-load decisions shared with explicit source tests. No global reload,
//! downgrade or simulation success can be inferred from a supplied package.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Status {
    NotPrepared = 0,
    PackageMissing = 1,
    UnsupportedCpu = 2,
    VirtualCpu = 3,
    AlreadyCurrent = 4,
    Updated = 5,
    FirmwarePrerequisite = 6,
    PackageRejected = 7,
    RevisionUnavailable = 8,
    LoadFailed = 9,
    RevisionMismatch = 10,
    RestartVerified = 11,
    RestartRequiresUpdate = 12,
    IdentityChanged = 13,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Decision {
    Keep(Status),
    Load,
}
pub(crate) fn decide(
    bundle: Option<bool>,
    supported: bool,
    virtual_cpu: bool,
    revision: Option<u32>,
    target: u32,
    minimum: u32,
    initial: bool,
) -> Decision {
    if bundle == Some(false) {
        return Decision::Keep(Status::PackageRejected);
    }
    if virtual_cpu {
        return Decision::Keep(Status::VirtualCpu);
    }
    if !supported {
        return Decision::Keep(Status::UnsupportedCpu);
    }
    if bundle.is_none() {
        return Decision::Keep(Status::PackageMissing);
    }
    let Some(revision) = revision else {
        return Decision::Keep(Status::RevisionUnavailable);
    };
    if revision >= target {
        return Decision::Keep(if initial {
            Status::AlreadyCurrent
        } else {
            Status::RestartVerified
        });
    }
    if !initial {
        return Decision::Keep(Status::RestartRequiresUpdate);
    }
    if revision < minimum {
        return Decision::Keep(Status::FirmwarePrerequisite);
    }
    Decision::Load
}
#[cfg(test)]
pub(crate) fn terminal(status: Status) -> bool {
    terminal_code(status as u32)
}
pub(crate) fn terminal_code(status: u32) -> bool {
    // Refusing optional bytes precedes any trigger and keeps early durable
    // boot recovery reachable. An attempted/uncertain load, changed CPU
    // identity or required runtime reload cannot admit that CPU.
    [
        Status::LoadFailed,
        Status::RevisionMismatch,
        Status::RestartRequiresUpdate,
        Status::IdentityChanged,
    ]
    .into_iter()
    .any(|failed| status == failed as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn virtual_never_loads_even_matching_old_revision() {
        for revision in [None, Some(0), Some(39), Some(40)] {
            assert_eq!(
                decide(Some(true), true, true, revision, 40, 0, true),
                Decision::Keep(Status::VirtualCpu)
            );
        }
    }
    #[test]
    fn absent_and_invalid_are_separate() {
        assert_eq!(
            decide(None, true, false, Some(1), 40, 0, true),
            Decision::Keep(Status::PackageMissing)
        );
        assert_eq!(
            decide(Some(false), true, false, Some(1), 40, 0, true),
            Decision::Keep(Status::PackageRejected)
        );
        assert!(!terminal(Status::PackageRejected));
    }
    #[test]
    fn unknown_cpu_never_loads() {
        assert_eq!(
            decide(Some(true), false, false, Some(1), 40, 0, true),
            Decision::Keep(Status::UnsupportedCpu)
        );
    }
    #[test]
    fn update_requires_actual_revision_and_strict_upgrade() {
        assert_eq!(
            decide(Some(true), true, false, None, 40, 0, true),
            Decision::Keep(Status::RevisionUnavailable)
        );
        assert_eq!(
            decide(Some(true), true, false, Some(39), 40, 0, true),
            Decision::Load
        );
        for rev in [40, 41, u32::MAX] {
            assert_eq!(
                decide(Some(true), true, false, Some(rev), 40, 0, true),
                Decision::Keep(Status::AlreadyCurrent)
            );
        }
    }
    #[test]
    fn amd_signing_base_is_required_before_any_attempt() {
        assert_eq!(
            decide(
                Some(true),
                true,
                false,
                Some(0x0a0011d5),
                0x0a0011de,
                0x0a0011d9,
                true
            ),
            Decision::Keep(Status::FirmwarePrerequisite)
        );
        assert_eq!(
            decide(
                Some(true),
                true,
                false,
                Some(0x0a0011d9),
                0x0a0011de,
                0x0a0011d9,
                true
            ),
            Decision::Load
        );
    }
    #[test]
    fn logical_restart_never_submits_an_update() {
        assert_eq!(
            decide(Some(true), true, false, Some(39), 40, 0, false),
            Decision::Keep(Status::RestartRequiresUpdate)
        );
        assert_eq!(
            decide(Some(true), true, false, Some(40), 40, 0, false),
            Decision::Keep(Status::RestartVerified)
        );
        assert!(terminal(Status::RestartRequiresUpdate));
    }
}
