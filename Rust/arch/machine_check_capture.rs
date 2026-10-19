//! Bounded read-only MCA algorithm. Hardware reads are supplied by the local
//! boot or terminal exception owner; this code never allocates or clears MSRs.
use crate::contract as c;

pub(crate) const LIMIT: usize = 64;
const VAL: u64 = 1 << 63;
const ADDRV: u64 = 1 << 58;
const MISCV: u64 = 1 << 59;
const SYNDV: u64 = 1 << 53;

pub(crate) fn profile(
    vendor: &[u8; 12],
    family: u32,
    model: u32,
    stepping: u32,
    smca: bool,
) -> u32 {
    if vendor == b"GenuineIntel" && matches!(family, 6 | 15) {
        1
    } else if vendor == b"AuthenticAMD" && smca {
        if family == 0x19 && model == 1 && stepping == 1 {
            3
        } else {
            0
        }
    } else if vendor == b"AuthenticAMD" && family >= 0xf {
        2
    } else {
        0
    }
}

fn read(
    info: &mut c::CpuMachineInfo,
    register: u32,
    reader: &mut impl FnMut(u32) -> Result<u64, ()>,
) -> Option<u64> {
    match reader(register) {
        Ok(value) => Some(value),
        Err(()) => {
            info.read_faults += 1;
            info.flags |= c::cpu_machine_flag_read_fault;
            None
        }
    }
}

pub(crate) fn collect(
    mut info: c::CpuMachineInfo,
    mut reader: impl FnMut(u32) -> Result<u64, ()>,
    mut bank: impl FnMut(c::CpuMachineBank),
) -> c::CpuMachineInfo {
    let required = c::cpu_machine_flag_mce | c::cpu_machine_flag_mca | c::cpu_machine_flag_msr;
    if info.flags & required != required {
        info.flags |= c::cpu_machine_flag_unsupported;
        return info;
    }
    if let Some(value) = read(&mut info, 0x179, &mut reader) {
        info.mcg_cap = value;
        info.bank_count = (value & 255) as u32;
        info.flags |= c::cpu_machine_flag_cap_valid;
    }
    if let Some(value) = read(&mut info, 0x17a, &mut reader) {
        info.mcg_status = value;
        info.flags |= c::cpu_machine_flag_status_valid;
    }
    if info.profile == 0 {
        // Advertised SMCA with an unknown model must never fall through to
        // legacy banks or manufacture an ordinal-to-block mapping.
        info.flags |= c::cpu_machine_flag_unsupported;
        return info;
    }
    if info.flags & c::cpu_machine_flag_cap_valid == 0 {
        return info;
    }
    // AMD's documented legacy window contains banks0..31. The proven SMCA
    // profile supplies its independent 64-bank window; Intel uses the bound.
    let register_limit = if info.profile == 2 { 32 } else { LIMIT as u32 };
    if info.bank_count > register_limit {
        info.flags |= c::cpu_machine_flag_truncated;
    }
    for index in 0..info.bank_count.min(register_limit) {
        let smca = info.profile == 3;
        let base = if smca {
            0xc000_2000 + 16 * index
        } else {
            0x400 + 4 * index
        };
        let mut value = c::CpuMachineBank {
            cpu_index: info.cpu_index,
            bank_index: index,
            epoch: info.epoch,
            sequence: info.sequence,
            format: if smca { 2 } else { 1 },
            status_msr: base + 1,
            ..Default::default()
        };
        let before_faults = info.read_faults;
        if let Some(status) = read(&mut info, base + 1, &mut reader) {
            value.status = status;
            value.flags |= c::cpu_machine_bank_status_read;
            if status & VAL != 0 {
                value.flags |= c::cpu_machine_bank_status_valid;
                info.valid_banks += 1;
                if status & ADDRV != 0
                    && let Some(address) = read(&mut info, base + 2, &mut reader)
                {
                    value.address = address;
                    value.flags |= c::cpu_machine_bank_address_read;
                }
                if status & MISCV != 0
                    && let Some(misc) = read(&mut info, base + 3, &mut reader)
                {
                    value.misc = misc;
                    value.flags |= c::cpu_machine_bank_misc_read;
                }
                if smca && let Some(config) = read(&mut info, base + 4, &mut reader) {
                    value.config = config;
                    value.flags |= c::cpu_machine_bank_config_read;
                    if config & 1 != 0 {
                        if let Some(ipid) = read(&mut info, base + 5, &mut reader) {
                            value.ipid = ipid;
                            value.flags |= c::cpu_machine_bank_ipid_read;
                        }
                        if status & SYNDV != 0
                            && let Some(syndrome) = read(&mut info, base + 6, &mut reader)
                        {
                            value.syndrome = syndrome;
                            value.flags |= c::cpu_machine_bank_syndrome_read;
                        }
                    }
                }
                match read(&mut info, base + 1, &mut reader) {
                    Some(after) => {
                        value.status_after = after;
                        value.flags |= c::cpu_machine_bank_status_after_read;
                        if after != status {
                            value.flags |= c::cpu_machine_bank_unstable;
                        }
                    }
                    None => value.flags |= c::cpu_machine_bank_unstable,
                }
            }
        }
        if before_faults != info.read_faults {
            value.flags |= c::cpu_machine_bank_read_fault;
        }
        bank(value);
        info.stored_banks += 1;
    }
    info
}

pub(crate) fn may_enable(info: &c::CpuMachineInfo, unsafe_error: bool) -> bool {
    let known = c::cpu_machine_flag_cap_valid
        | c::cpu_machine_flag_status_valid
        | c::cpu_machine_flag_prepared
        | c::cpu_machine_flag_mce
        | c::cpu_machine_flag_mca
        | c::cpu_machine_flag_msr;
    info.profile != 0
        && info.flags & known == known
        && info.bank_count != 0
        && info.flags
            & (c::cpu_machine_flag_read_fault
                | c::cpu_machine_flag_truncated
                | c::cpu_machine_flag_unsupported)
            == 0
        && info.mcg_status & 4 == 0
        && !unsafe_error
}

/// Unknown AMD legacy models cannot use UC=0 alone as evidence of a safe
/// corrected boot record. Extended raw bits conservatively block activation;
/// only the documented SMCA profile interprets their detailed semantics.
pub(crate) fn unsafe_boot_bank(profile: u32, bank: &c::CpuMachineBank) -> bool {
    if bank.flags & (c::cpu_machine_bank_unstable | c::cpu_machine_bank_read_fault) != 0 {
        return true;
    }
    if bank.flags & c::cpu_machine_bank_status_valid == 0 {
        return false;
    }
    if profile == 3 && (bank.flags & c::cpu_machine_bank_config_read == 0 || bank.config & 1 == 0) {
        return true;
    }
    let extended = if matches!(profile, 2 | 3) {
        (1 << 55) | (1 << 44) | (1 << 43)
    } else {
        0
    };
    bank.status & ((1 << 62) | (1 << 61) | (1 << 57) | extended) != 0
}

#[cfg(test)]
mod tests {
    use super::*;
    fn base(profile: u32) -> c::CpuMachineInfo {
        c::CpuMachineInfo {
            cpu_index: 3,
            epoch: 41,
            sequence: 2,
            profile,
            bank_limit: 64,
            flags: c::cpu_machine_flag_mce | c::cpu_machine_flag_mca | c::cpu_machine_flag_msr,
            ..Default::default()
        }
    }
    #[test]
    fn legacy_and_smca_unknown_extended_boot_state_cannot_enable_mce() {
        let mut bank = c::CpuMachineBank {
            flags: c::cpu_machine_bank_status_valid | c::cpu_machine_bank_config_read,
            config: 1,
            status: VAL,
            ..Default::default()
        };
        assert!(!unsafe_boot_bank(1, &bank));
        assert!(!unsafe_boot_bank(2, &bank));
        assert!(!unsafe_boot_bank(3, &bank));
        for bit in [55, 44, 43] {
            bank.status = VAL | (1 << bit);
            assert!(unsafe_boot_bank(2, &bank));
            assert!(unsafe_boot_bank(3, &bank));
        }
        for bit in [62, 61, 57] {
            bank.status = VAL | (1 << bit);
            for profile in [1, 2, 3] {
                assert!(unsafe_boot_bank(profile, &bank));
            }
        }
        bank.status = VAL;
        bank.flags = c::cpu_machine_bank_status_valid;
        assert!(unsafe_boot_bank(3, &bank));
        bank.flags = 0;
        bank.status = 1 << 44;
        assert!(!unsafe_boot_bank(2, &bank));
        bank.flags = c::cpu_machine_bank_read_fault;
        assert!(unsafe_boot_bank(2, &bank));
    }
    #[test]
    fn model_gate_never_falls_back_from_unknown_smca() {
        assert_eq!(profile(b"AuthenticAMD", 0x19, 1, 1, true), 3);
        for (family, model, stepping) in [(0x19, 1, 0), (0x19, 0x21, 1), (0x1a, 1, 1)] {
            assert_eq!(profile(b"AuthenticAMD", family, model, stepping, true), 0);
        }
        assert_eq!(profile(b"AuthenticAMD", 0x17, 1, 1, false), 2);
        assert_eq!(profile(b"GenuineIntel", 6, 0x3c, 3, false), 1);
        assert_eq!(profile(b"OtherVendor!", 6, 0x3c, 3, false), 0);
    }
    #[test]
    fn absent_capability_performs_no_instruction() {
        let mut input = base(1);
        input.flags &= !c::cpu_machine_flag_mca;
        let out = collect(
            input,
            |_| panic!("unqualified read"),
            |_| panic!("invented bank"),
        );
        assert_eq!(out.bank_count, 0);
        assert_ne!(out.flags & c::cpu_machine_flag_unsupported, 0);
    }
    #[test]
    fn unknown_smca_retains_real_global_count_without_bank_reads() {
        let out = collect(
            base(0),
            |id| match id {
                0x179 => Ok(53),
                0x17a => Ok(4),
                _ => panic!("guessed register"),
            },
            |_| panic!("invented bank"),
        );
        assert_eq!((out.bank_count, out.stored_banks), (53, 0));
    }
    #[test]
    fn global_cap_fault_cannot_invent_count() {
        let out = collect(
            base(1),
            |id| match id {
                0x179 => Err(()),
                0x17a => Ok(0),
                _ => panic!("bank without count"),
            },
            |_| panic!("invented bank"),
        );
        assert_eq!((out.bank_count, out.read_faults), (0, 1));
        assert_eq!(out.flags & c::cpu_machine_flag_cap_valid, 0);
    }
    #[test]
    fn invalid_status_forbids_all_auxiliary_reads() {
        let mut row = None;
        let out = collect(
            base(3),
            |id| match id {
                0x179 => Ok(1),
                0x17a => Ok(0),
                0xc0002001 => Ok(ADDRV | MISCV | SYNDV),
                _ => panic!("invalid VAL auxiliary read"),
            },
            |v| row = Some(v),
        );
        assert_eq!((out.stored_banks, out.valid_banks), (1, 0));
        assert_eq!(row.unwrap().flags, c::cpu_machine_bank_status_read);
    }
    #[test]
    fn valid_legacy_auxiliary_bits_and_read_faults_are_independent() {
        let mut row = None;
        let status = VAL | ADDRV | MISCV | 0x1234;
        let out = collect(
            base(1),
            |id| match id {
                0x179 => Ok(1),
                0x17a => Ok(4),
                0x401 => Ok(status),
                0x402 => Ok(0xabcdef),
                0x403 => Err(()),
                _ => panic!("unowned read"),
            },
            |v| row = Some(v),
        );
        let v = row.unwrap();
        assert_eq!((out.valid_banks, out.read_faults), (1, 1));
        assert_eq!((v.status_msr, v.address, v.misc), (0x401, 0xabcdef, 0));
        assert_ne!(v.flags & c::cpu_machine_bank_address_read, 0);
        assert_eq!(v.flags & c::cpu_machine_bank_misc_read, 0);
        assert_ne!(v.flags & c::cpu_machine_bank_read_fault, 0);
    }
    #[test]
    fn smca_configuration_and_syndv_gate_extensions() {
        for config in [0, 1, 1 | (1 << 32)] {
            for syndv in [false, true] {
                let mut row = None;
                let status = VAL | if syndv { SYNDV } else { 0 };
                collect(
                    base(3),
                    |id| match id {
                        0x179 => Ok(1),
                        0x17a => Ok(0),
                        0xc0002001 => Ok(status),
                        0xc0002004 => Ok(config),
                        0xc0002005 if config & 1 != 0 => Ok(0x001000b000000001),
                        0xc0002006 if config & 1 != 0 && syndv => Ok(2 << 24),
                        _ => panic!("unqualified SMCA auxiliary read"),
                    },
                    |v| row = Some(v),
                );
                let v = row.unwrap();
                assert_eq!(
                    v.flags & c::cpu_machine_bank_ipid_read != 0,
                    config & 1 != 0
                );
                assert_eq!(
                    v.flags & c::cpu_machine_bank_syndrome_read != 0,
                    config & 1 != 0 && syndv
                );
            }
        }
    }
    #[test]
    fn status_change_or_final_fault_explicitly_invalidates_coherence() {
        for fault in [false, true] {
            let mut reads = 0;
            let mut row = None;
            collect(
                base(1),
                |id| match id {
                    0x179 => Ok(1),
                    0x17a => Ok(0),
                    0x401 => {
                        reads += 1;
                        if reads == 1 {
                            Ok(VAL)
                        } else if fault {
                            Err(())
                        } else {
                            Ok(VAL | (1 << 61))
                        }
                    }
                    _ => panic!("extra read"),
                },
                |v| row = Some(v),
            );
            let v = row.unwrap();
            assert_ne!(v.flags & c::cpu_machine_bank_unstable, 0);
            assert_eq!(v.flags & c::cpu_machine_bank_status_after_read != 0, !fault);
        }
    }
    #[test]
    fn exact_count_and_documented_register_bounds_are_preserved() {
        for (profile, limit) in [(1, 64), (2, 32), (3, 64)] {
            let mut count = 0;
            let out = collect(
                base(profile),
                |id| match id {
                    0x179 => Ok(255),
                    0x17a => Ok(0),
                    _ => Ok(0),
                },
                |v| {
                    assert_eq!(v.bank_index, count);
                    assert_eq!((v.cpu_index, v.epoch, v.sequence), (3, 41, 2));
                    count += 1;
                },
            );
            assert_eq!(
                (out.bank_count, out.stored_banks, count),
                (255, limit, limit)
            );
            assert_ne!(out.flags & c::cpu_machine_flag_truncated, 0);
        }
    }
    #[test]
    fn mce_enable_requires_complete_safe_local_scan_and_clear_mcip() {
        let mut v = base(1);
        v.flags |= c::cpu_machine_flag_cap_valid
            | c::cpu_machine_flag_status_valid
            | c::cpu_machine_flag_prepared;
        v.bank_count = 4;
        assert!(may_enable(&v, false));
        assert!(!may_enable(&v, true));
        for flag in [
            c::cpu_machine_flag_read_fault,
            c::cpu_machine_flag_truncated,
        ] {
            let mut failed = v;
            failed.flags |= flag;
            assert!(!may_enable(&failed, false));
        }
        for flag in [
            c::cpu_machine_flag_cap_valid,
            c::cpu_machine_flag_status_valid,
            c::cpu_machine_flag_prepared,
            c::cpu_machine_flag_mce,
            c::cpu_machine_flag_mca,
            c::cpu_machine_flag_msr,
        ] {
            let mut failed = v;
            failed.flags &= !flag;
            assert!(!may_enable(&failed, false));
        }
        v.mcg_status = 4;
        assert!(!may_enable(&v, false));
        v.mcg_status = 0;
        v.profile = 0;
        assert!(!may_enable(&v, false));
        v.profile = 1;
        v.bank_count = 0;
        assert!(!may_enable(&v, false));
    }
}
