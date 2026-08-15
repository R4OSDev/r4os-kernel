const bootlog = @import("../kernel/bootlog.zig");
const msr = @import("../arch/x86_64/msr.zig");

const IA32_PAT: u32 = 0x277;
const IA32_MTRRCAP: u32 = 0x0FE;

const FeatureSet = struct {
    tsc: bool = false,
    msr: bool = false,
    apic: bool = false,
    mtrr: bool = false,
    pat: bool = false,
    fxsr: bool = false,
    sse: bool = false,
    sse2: bool = false,
    sse3: bool = false,
    ssse3: bool = false,
    sse41: bool = false,
    sse42: bool = false,
    x2apic: bool = false,
    tsc_deadline: bool = false,
    xsave: bool = false,
    osxsave: bool = false,
    avx: bool = false,
    avx2: bool = false,
    fsgsbase: bool = false,
    erms: bool = false,
    invpcid: bool = false,
    clflushopt: bool = false,
    syscall: bool = false,
    nx: bool = false,
    page1gb: bool = false,
    rdtscp: bool = false,
    long_mode: bool = false,
    invariant_tsc: bool = false,
};

pub const Status = struct {
    detected: bool = false,
    vendor: [12]u8 = .{0} ** 12,
    vendor_len: usize = 0,
    brand: [48]u8 = .{0} ** 48,
    brand_len: usize = 0,
    max_basic_leaf: u32 = 0,
    max_extended_leaf: u32 = 0,
    stepping: u8 = 0,
    base_model: u8 = 0,
    base_family: u8 = 0,
    display_model: u16 = 0,
    display_family: u16 = 0,
    processor_type: u8 = 0,
    apic_id: u8 = 0,
    logical_processors: u8 = 0,
    clflush_line_bytes: u16 = 0,
    physical_address_bits: u8 = 0,
    virtual_address_bits: u8 = 0,
    l2_cache_kb: u32 = 0,
    tsc_denominator: u32 = 0,
    tsc_numerator: u32 = 0,
    crystal_hz: u32 = 0,
    base_mhz: u16 = 0,
    max_mhz: u16 = 0,
    bus_mhz: u16 = 0,
    xcr0: u64 = 0,
    pat_msr: u64 = 0,
    mtrr_cap: u64 = 0,
    features: FeatureSet = .{},
};

const CpuidRegs = struct {
    eax: u32,
    ebx: u32,
    ecx: u32,
    edx: u32,
};

var current: Status = .{};
var module_simd_allowed: bool = false;

pub fn detect() Status {
    current = .{};

    const leaf0 = cpuid(0, 0);
    current.max_basic_leaf = leaf0.eax;
    writeU32Le12(&current.vendor, 0, leaf0.ebx);
    writeU32Le12(&current.vendor, 4, leaf0.edx);
    writeU32Le12(&current.vendor, 8, leaf0.ecx);
    current.vendor_len = trimmedLen(current.vendor[0..]);

    const ext0 = cpuid(0x80000000, 0);
    current.max_extended_leaf = ext0.eax;

    if (current.max_basic_leaf >= 1) {
        const leaf1 = cpuid(1, 0);
        current.stepping = @truncate(leaf1.eax & 0xF);
        current.base_model = @truncate((leaf1.eax >> 4) & 0xF);
        current.base_family = @truncate((leaf1.eax >> 8) & 0xF);
        current.processor_type = @truncate((leaf1.eax >> 12) & 0x3);
        const ext_model: u16 = @truncate((leaf1.eax >> 16) & 0xF);
        const ext_family: u16 = @truncate((leaf1.eax >> 20) & 0xFF);
        current.display_family = current.base_family;
        if (current.base_family == 0x0F) current.display_family += ext_family;
        current.display_model = current.base_model;
        if (current.base_family == 0x06 or current.base_family == 0x0F) {
            current.display_model += ext_model << 4;
        }
        current.apic_id = @truncate((leaf1.ebx >> 24) & 0xFF);
        current.logical_processors = @truncate((leaf1.ebx >> 16) & 0xFF);
        current.clflush_line_bytes = @as(u16, @truncate((leaf1.ebx >> 8) & 0xFF)) * 8;

        current.features.tsc = bit(leaf1.edx, 4);
        current.features.msr = bit(leaf1.edx, 5);
        current.features.apic = bit(leaf1.edx, 9);
        current.features.mtrr = bit(leaf1.edx, 12);
        current.features.pat = bit(leaf1.edx, 16);
        current.features.fxsr = bit(leaf1.edx, 24);
        current.features.sse = bit(leaf1.edx, 25);
        current.features.sse2 = bit(leaf1.edx, 26);
        current.features.sse3 = bit(leaf1.ecx, 0);
        current.features.ssse3 = bit(leaf1.ecx, 9);
        current.features.sse41 = bit(leaf1.ecx, 19);
        current.features.sse42 = bit(leaf1.ecx, 20);
        current.features.x2apic = bit(leaf1.ecx, 21);
        current.features.tsc_deadline = bit(leaf1.ecx, 24);
        current.features.xsave = bit(leaf1.ecx, 26);
        current.features.osxsave = bit(leaf1.ecx, 27);
        current.features.avx = bit(leaf1.ecx, 28);
    }

    if (current.max_basic_leaf >= 7) {
        const leaf7 = cpuid(7, 0);
        current.features.fsgsbase = bit(leaf7.ebx, 0);
        current.features.avx2 = bit(leaf7.ebx, 5);
        current.features.erms = bit(leaf7.ebx, 9);
        current.features.invpcid = bit(leaf7.ebx, 10);
        current.features.clflushopt = bit(leaf7.ebx, 23);
    }

    if (current.max_basic_leaf >= 0x15) {
        const leaf15 = cpuid(0x15, 0);
        current.tsc_denominator = leaf15.eax;
        current.tsc_numerator = leaf15.ebx;
        current.crystal_hz = leaf15.ecx;
    }

    if (current.max_basic_leaf >= 0x16) {
        const leaf16 = cpuid(0x16, 0);
        current.base_mhz = @truncate(leaf16.eax & 0xFFFF);
        current.max_mhz = @truncate(leaf16.ebx & 0xFFFF);
        current.bus_mhz = @truncate(leaf16.ecx & 0xFFFF);
    }

    if (current.max_extended_leaf >= 0x80000004) {
        var offset: usize = 0;
        var leaf: u32 = 0x80000002;
        while (leaf <= 0x80000004) : (leaf += 1) {
            const regs = cpuid(leaf, 0);
            writeU32Le48(&current.brand, offset + 0, regs.eax);
            writeU32Le48(&current.brand, offset + 4, regs.ebx);
            writeU32Le48(&current.brand, offset + 8, regs.ecx);
            writeU32Le48(&current.brand, offset + 12, regs.edx);
            offset += 16;
        }
        current.brand_len = trimmedLen(current.brand[0..]);
    }

    if (current.max_extended_leaf >= 0x80000001) {
        const leaf_ext1 = cpuid(0x80000001, 0);
        current.features.syscall = bit(leaf_ext1.edx, 11);
        current.features.nx = bit(leaf_ext1.edx, 20);
        current.features.page1gb = bit(leaf_ext1.edx, 26);
        current.features.rdtscp = bit(leaf_ext1.edx, 27);
        current.features.long_mode = bit(leaf_ext1.edx, 29);
    }

    if (current.max_extended_leaf >= 0x80000006) {
        const leaf_ext6 = cpuid(0x80000006, 0);
        current.l2_cache_kb = (leaf_ext6.ecx >> 16) & 0xFFFF;
    }

    if (current.max_extended_leaf >= 0x80000007) {
        const leaf_ext7 = cpuid(0x80000007, 0);
        current.features.invariant_tsc = bit(leaf_ext7.edx, 8);
    }

    if (current.max_extended_leaf >= 0x80000008) {
        const leaf_ext8 = cpuid(0x80000008, 0);
        current.physical_address_bits = @truncate(leaf_ext8.eax & 0xFF);
        current.virtual_address_bits = @truncate((leaf_ext8.eax >> 8) & 0xFF);
    }

    if (current.features.osxsave) {
        current.xcr0 = xgetbv(0);
    }
    if (current.features.msr and current.features.pat) {
        current.pat_msr = msr.read(IA32_PAT);
    }
    if (current.features.msr and current.features.mtrr) {
        current.mtrr_cap = msr.read(IA32_MTRRCAP);
    }

    current.detected = true;
    logStatus();
    return current;
}

pub fn status() Status {
    if (!current.detected) return detect();
    return current;
}

pub fn patAvailable() bool {
    return status().features.pat and status().features.msr;
}

pub fn mtrrAvailable() bool {
    return status().features.mtrr and status().features.msr;
}

pub fn writeCombiningBasisAvailable() bool {
    const s = status();
    return s.features.pat and s.features.mtrr and s.features.msr and patEntry5IsWriteCombining();
}

pub fn patEntry5IsWriteCombining() bool {
    const s = status();
    if (!s.features.pat or !s.features.msr) return false;
    return patEntryType(s.pat_msr, 5) == 0x01;
}

pub fn moduleSimdAllowed() bool {
    return module_simd_allowed;
}

pub fn setModuleSimdAllowed(allowed: bool) void {
    module_simd_allowed = allowed;
}

pub fn noteOsXsaveEnabled(xcr0_value: u64) void {
    if (!current.detected) _ = detect();
    current.features.osxsave = true;
    current.xcr0 = xcr0_value;
}

// 0.59.19: Initiale APIC-ID des Boot-Prozessors (CPUID.1:EBX[31:24]) fuer
// die MSI-Zieladresse. R4OS betreibt ausschliesslich den BSP.
pub fn bootApicId() u32 {
    return cpuid(1, 0).ebx >> 24;
}

fn cpuid(leaf: u32, subleaf: u32) CpuidRegs {
    var eax: u32 = leaf;
    var ebx: u32 = 0;
    var ecx: u32 = subleaf;
    var edx: u32 = 0;
    asm volatile ("cpuid"
        : [eax] "={eax}" (eax),
          [ebx] "={ebx}" (ebx),
          [ecx] "={ecx}" (ecx),
          [edx] "={edx}" (edx),
        : [leaf] "{eax}" (leaf),
          [subleaf] "{ecx}" (subleaf),
    );
    return .{ .eax = eax, .ebx = ebx, .ecx = ecx, .edx = edx };
}

fn xgetbv(index: u32) u64 {
    var eax: u32 = 0;
    var edx: u32 = 0;
    asm volatile ("xgetbv"
        : [eax] "={eax}" (eax),
          [edx] "={edx}" (edx),
        : [index] "{ecx}" (index),
    );
    return (@as(u64, edx) << 32) | eax;
}

fn bit(value: u32, index: u5) bool {
    return (value & (@as(u32, 1) << index)) != 0;
}

fn patEntryType(pat_msr: u64, entry_index: u3) u8 {
    return @truncate((pat_msr >> (@as(u6, entry_index) * 8)) & 0xFF);
}

fn writeU32Le12(dest: *[12]u8, offset: usize, value: u32) void {
    dest[offset + 0] = @truncate(value);
    dest[offset + 1] = @truncate(value >> 8);
    dest[offset + 2] = @truncate(value >> 16);
    dest[offset + 3] = @truncate(value >> 24);
}

fn writeU32Le48(dest: *[48]u8, offset: usize, value: u32) void {
    dest[offset + 0] = @truncate(value);
    dest[offset + 1] = @truncate(value >> 8);
    dest[offset + 2] = @truncate(value >> 16);
    dest[offset + 3] = @truncate(value >> 24);
}

fn trimmedLen(bytes: []const u8) usize {
    var len = bytes.len;
    while (len > 0 and (bytes[len - 1] == 0 or bytes[len - 1] == ' ')) {
        len -= 1;
    }
    return len;
}

fn logStatus() void {
    bootlog.puts("[CPU] vendor=");
    bootlog.puts(current.vendor[0..current.vendor_len]);
    bootlog.puts(" family=");
    bootlog.putDec(current.display_family);
    bootlog.puts(" model=");
    bootlog.putDec(current.display_model);
    bootlog.puts(" stepping=");
    bootlog.putDec(current.stepping);
    bootlog.puts(" apic=");
    bootlog.putDec(current.apic_id);
    bootlog.puts(" features=");
    bootlog.puts(if (current.features.long_mode) "lm" else "no-lm");
    bootlog.puts(",");
    bootlog.puts(if (current.features.pat) "pat" else "no-pat");
    bootlog.puts(",");
    bootlog.puts(if (current.features.mtrr) "mtrr" else "no-mtrr");
    bootlog.puts(",");
    bootlog.puts(if (current.features.invariant_tsc) "invtsc" else "tsc-variable");
    bootlog.puts(",");
    bootlog.puts(if (current.features.avx) "avx" else "no-avx");
    bootlog.puts(",");
    bootlog.puts(if (current.features.avx2) "avx2" else "no-avx2");
    bootlog.puts(" modules_simd=");
    bootlog.puts(if (moduleSimdAllowed()) "allowed" else "blocked");
    bootlog.puts("\r\n");
}
