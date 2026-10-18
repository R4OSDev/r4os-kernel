//! Local architectural ID, independent of model or the BSP's cached view.
//! Kept pure so sparse/full-width CPUID topologies can exercise the same
//! selection used before the first BSP per-CPU slot is installed.
pub(crate) fn identify(mut query: impl FnMut(u32, u32) -> core::arch::x86_64::CpuidResult) -> u32 {
    let maximum = query(0, 0).eax;
    for leaf in [0x1f, 0x0b] {
        if maximum >= leaf {
            let value = query(leaf, 0);
            if value.ebx & 0xffff != 0 && value.ecx & 0xff00 != 0 {
                return value.edx;
            }
        }
    }
    if query(0x8000_0000, 0).eax >= 0x8000_001e && query(0x8000_0001, 0).ecx & (1 << 22) != 0 {
        return query(0x8000_001e, 0).eax;
    }
    query(1, 0).ebx >> 24
}
