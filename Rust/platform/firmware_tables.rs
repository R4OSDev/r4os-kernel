//! Boot-owned immutable ACPI catalog. Only the BSP populates it, before
//! driver/task publication. Readers retain exact bytes and generation.
use crate::boot::info::MemoryEntry;
use core::{cell::UnsafeCell, ptr, slice};

pub const MAX_TABLES: usize = 256;
pub const MAX_BYTES: usize = 1024 * 1024;
const INVALID: i32 = -1;
const UNAVAILABLE: i32 = -2;
const CORRUPT: i32 = -3;
const NOT_FOUND: i32 = -4;
const STALE: i32 = -5;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Record {
    pub address: u64,
    pub bytes: u64,
    pub signature: u32,
    pub revision: u8,
    pub valid: bool,
    pub digest: [u8; 32],
}
impl Record {
    const EMPTY: Self = Self {
        address: 0,
        bytes: 0,
        signature: 0,
        revision: 0,
        valid: false,
        digest: [0; 32],
    };
    fn unchanged(&self) -> Result<(), i32> {
        if !self.valid {
            return Err(CORRUPT);
        }
        // The catalog only admits retained, permanent firmware bytes. No
        // mutable borrow or reference is kept between caller operations.
        let bytes =
            unsafe { slice::from_raw_parts(self.address as *const u8, self.bytes as usize) };
        if sha256(bytes) != self.digest {
            return Err(STALE);
        }
        Ok(())
    }
}
#[repr(C)]
#[derive(Default)]
pub struct Info {
    pub handle: u64,
    pub bytes: u64,
    pub generation: u64,
    pub signature: u32,
    pub revision: u8,
}
#[repr(C)]
pub struct Catalog {
    pub records: [Record; MAX_TABLES],
    pub count: u64,
    pub generation: u32,
    pub root_valid: bool,
    pub failed: bool,
    pub sealed: bool,
}
impl Catalog {
    pub const EMPTY: Self = Self {
        records: [Record::EMPTY; MAX_TABLES],
        count: 0,
        generation: 0,
        root_valid: false,
        failed: false,
        sealed: false,
    };
    pub fn begin(&mut self) {
        self.count = 0;
        self.root_valid = false;
        self.sealed = false;
        self.failed = self.generation == u32::MAX;
        if !self.failed {
            self.generation += 1;
        }
    }
    pub fn root_validated(&mut self) {
        if !self.sealed {
            self.root_valid = true;
        }
    }
    pub fn incomplete(&mut self) {
        if !self.sealed {
            self.failed = true;
        }
    }
    pub fn seal(&mut self) {
        self.sealed = true;
    }
    pub fn observe(&mut self, bytes: &[u8]) {
        if self.sealed || self.failed {
            return;
        }
        if bytes.len() < 36 || bytes.len() > MAX_BYTES {
            self.failed = true;
            return;
        }
        let address = bytes.as_ptr() as u64;
        if self.records[..self.count as usize]
            .iter()
            .any(|record| record.address == address)
        {
            return;
        }
        if self.count as usize == MAX_TABLES {
            self.failed = true;
            return;
        }
        let size = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
        let checksum = bytes.iter().fold(0u8, |sum, byte| sum.wrapping_add(*byte));
        self.records[self.count as usize] = Record {
            address,
            bytes: bytes.len() as u64,
            signature: u32::from_le_bytes(bytes[..4].try_into().unwrap()),
            revision: bytes[8],
            valid: size as usize == bytes.len() && checksum == 0,
            digest: sha256(bytes),
        };
        self.count += 1;
    }
    pub fn stat(&self, signature: u32, occurrence: u32) -> Result<Info, i32> {
        if !self.sealed || !self.root_valid || self.failed {
            return Err(UNAVAILABLE);
        }
        let mut index = 0;
        for (slot, record) in self.records[..self.count as usize].iter().enumerate() {
            if record.signature != signature {
                continue;
            }
            if index != occurrence {
                index += 1;
                continue;
            }
            record.unchanged()?;
            return Ok(Info {
                handle: (u64::from(self.generation) << 32) | (slot as u64 + 1),
                bytes: record.bytes,
                generation: u64::from(self.generation),
                signature: record.signature,
                revision: record.revision,
            });
        }
        Err(NOT_FOUND)
    }
    // Validate raw output before creating any mutable view: aliases into the
    // catalog or any retained source must fail without modifying either.
    unsafe fn read_at(
        &self,
        handle: u64,
        offset: u64,
        output: *mut u8,
        bytes: u64,
    ) -> Result<(), i32> {
        if !self.sealed || !self.root_valid || self.failed {
            return Err(UNAVAILABLE);
        }
        let index = handle & 0xffff_ffff;
        if handle >> 32 != u64::from(self.generation) || index == 0 || index > self.count {
            return Err(STALE);
        }
        let record = &self.records[index as usize - 1];
        if bytes == 0 || bytes > 65536 || offset > record.bytes || bytes > record.bytes - offset {
            return Err(INVALID);
        }
        let address = output as u64;
        let end = address.checked_add(bytes).ok_or(INVALID)?;
        let catalog_address = self as *const Self as u64;
        if address < catalog_address + core::mem::size_of::<Self>() as u64 && catalog_address < end
        {
            return Err(INVALID);
        }
        for source in &self.records[..self.count as usize] {
            if address < source.address + source.bytes && source.address < end {
                return Err(INVALID);
            }
        }
        record.unchanged()?;
        unsafe {
            ptr::copy_nonoverlapping(
                (record.address + offset) as *const u8,
                output,
                bytes as usize,
            )
        };
        record.unchanged()
    }
}
struct Storage(UnsafeCell<Catalog>);
// BSP-owned boot construction, sealed before any concurrent consumer exists.
unsafe impl Sync for Storage {}
static CATALOG: Storage = Storage(UnsafeCell::new(Catalog::EMPTY));
pub fn catalog() -> *mut Catalog {
    CATALOG.0.get()
}

/// Conservative work hint, never locality/admission data. Before discovery
/// completes retain stack anchors; a sealed catalog without a valid SRAT
/// cannot yield a NUMA preference later in this boot. Bytes are still fully
/// authenticated by boot_table before any proximity facts are published.
pub(crate) fn may_have_srat() -> bool {
    let catalog = unsafe { &*catalog() };
    !catalog.sealed
        || (catalog.root_valid
            && !catalog.failed
            && catalog.records[..catalog.count as usize]
                .iter()
                .any(|r| r.signature == u32::from_le_bytes(*b"SRAT") && r.valid))
}

/// Boot-only view of one unique, checksum/digest-verified retained table.
/// The sealed catalog and its firmware backing stay immutable and resident.
/// Duplicates, incomplete roots and changed bytes never become locality data.
pub(crate) fn boot_table(signature: [u8; 4]) -> Result<Option<&'static [u8]>, i32> {
    let catalog = unsafe { &*catalog() };
    let signature = u32::from_le_bytes(signature);
    let info = match catalog.stat(signature, 0) {
        Ok(info) => info,
        Err(NOT_FOUND) => return Ok(None),
        Err(error) => return Err(error),
    };
    if !matches!(catalog.stat(signature, 1), Err(NOT_FOUND)) {
        return Err(CORRUPT);
    }
    let record = &catalog.records[(info.handle as u32 - 1) as usize];
    Ok(Some(unsafe {
        slice::from_raw_parts(record.address as *const u8, record.bytes as usize)
    }))
}

pub fn retained_backing(base: u64, bytes: u64, entries: &[MemoryEntry]) -> bool {
    if base == 0 || bytes == 0 {
        return false;
    }
    let Some(end) = base.checked_add(bytes) else {
        return false;
    };
    let mut cursor = base;
    while cursor < end {
        let mut next = cursor;
        for entry in entries {
            if !entry.valid || entry.length == 0 || entry.base > cursor || entry.end <= cursor {
                continue;
            }
            if !matches!(entry.kind, 1..=3) {
                return false;
            }
            next = end.min(entry.end);
            break;
        }
        if next <= cursor {
            return false;
        }
        cursor = next;
    }
    true
}

// Same SHA-256 fingerprint as the former std.crypto.hash.sha2.Sha256 owner;
// bounded stack scratch and no allocator, foreign digest callback or SIMD.
pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    let mut state = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let (chunks, tail) = bytes.as_chunks::<64>();
    for block in chunks {
        compress(&mut state, block);
    }
    let mut block = [0u8; 64];
    block[..tail.len()].copy_from_slice(tail);
    block[tail.len()] = 0x80;
    if tail.len() >= 56 {
        compress(&mut state, &block);
        block = [0; 64];
    }
    block[56..].copy_from_slice(&(bytes.len() as u64).wrapping_mul(8).to_be_bytes());
    compress(&mut state, &block);
    let mut result = [0; 32];
    for (word, output) in state.iter().zip(result.as_chunks_mut::<4>().0) {
        output.copy_from_slice(&word.to_be_bytes());
    }
    result
}
fn compress(state: &mut [u32; 8], block: &[u8]) {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut words = [0u32; 64];
    for (word, input) in words[..16].iter_mut().zip(block.as_chunks::<4>().0) {
        *word = u32::from_be_bytes(*input);
    }
    for i in 16..64 {
        let x = words[i - 15];
        let y = words[i - 2];
        let s0 = x.rotate_right(7) ^ x.rotate_right(18) ^ (x >> 3);
        let s1 = y.rotate_right(17) ^ y.rotate_right(19) ^ (y >> 10);
        words[i] = words[i - 16]
            .wrapping_add(s0)
            .wrapping_add(words[i - 7])
            .wrapping_add(s1);
    }
    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *state;
    for i in 0..64 {
        let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let t1 = h
            .wrapping_add(s1)
            .wrapping_add((e & f) ^ (!e & g))
            .wrapping_add(K[i])
            .wrapping_add(words[i]);
        let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let t2 = s0.wrapping_add((a & b) ^ (a & c) ^ (b & c));
        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(t1);
        d = c;
        c = b;
        b = a;
        a = t1.wrapping_add(t2);
    }
    for (word, value) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
        *word = word.wrapping_add(value);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_firmware_catalog() -> *mut Catalog {
    catalog()
}
/// # Safety
/// Entries are immutable normalized C storage for count elements.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_firmware_retained(
    base: u64,
    bytes: u64,
    entries: *const MemoryEntry,
    count: u64,
) -> bool {
    let entries = if count == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(entries, count as usize) }
    };
    retained_backing(base, bytes, entries)
}
/// # Safety
/// Caller owns the catalog exclusively during boot construction; input is
/// retained immutable storage for bytes. No consumers exist before seal.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_firmware_control(
    catalog: *mut Catalog,
    operation: u8,
    input: *const u8,
    bytes: u64,
) {
    let catalog = unsafe { &mut *catalog };
    match operation {
        0 => catalog.begin(),
        1 => catalog.root_validated(),
        2 => catalog.incomplete(),
        3 => catalog.seal(),
        4 => {
            let input = if bytes == 0 {
                &[]
            } else {
                unsafe { slice::from_raw_parts(input, bytes as usize) }
            };
            catalog.observe(input);
        }
        _ => {}
    }
}
/// # Safety
/// Catalog is sealed immutable resident storage; output is exclusive C storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_firmware_stat(
    catalog: *const Catalog,
    signature: u32,
    occurrence: u32,
    output: *mut Info,
) -> i32 {
    match unsafe { (&*catalog).stat(signature, occurrence) } {
        Ok(info) => {
            unsafe { output.write(info) };
            0
        }
        Err(error) => error,
    }
}
/// # Safety
/// Catalog is sealed resident storage and output is valid caller memory for
/// bytes. Rejected aliases into the catalog and its sources are never written.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_firmware_read(
    catalog: *const Catalog,
    handle: u64,
    offset: u64,
    output: *mut u8,
    bytes: u64,
) -> i32 {
    match unsafe { (&*catalog).read_at(handle, offset, output, bytes) } {
        Ok(()) => 0,
        Err(error) => error,
    }
}
const _: () = {
    assert!(core::mem::size_of::<Record>() == 56);
    assert!(core::mem::size_of::<Catalog>() == 14352);
    assert!(core::mem::offset_of!(Catalog, count) == 14336);
    assert!(core::mem::size_of::<Info>() == 32);
};
