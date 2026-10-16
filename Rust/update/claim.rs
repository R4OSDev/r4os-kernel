//! Durable upload claim bytes and replay. Backend identity is checked by the
//! actual VFS adapter, never inferred from an equal name by this owner.
use crate::fs::vfs_types::Bytes;
pub const MAX_PATH: usize = 1024;
pub const MAX_NAME: usize = 768;
pub const MAX_CLAIMS: usize = 16;
pub const CLAIM_MAX: usize = 4096;
pub const SEED: u32 = 2166136261;
#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct Identity {
    pub node: u64,
    pub generation: u16,
    pub size: u64,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Claim {
    pub generation: u64,
    pub volume: u64,
    pub parent_path: [u8; MAX_PATH],
    pub parent_len: usize,
    pub stage_name: [u8; MAX_NAME],
    pub stage_len: usize,
    pub target_name: [u8; MAX_NAME],
    pub target_len: usize,
    pub backup_name: [u8; MAX_NAME],
    pub backup_len: usize,
    pub identity: Identity,
    pub protocol: u8,
}
impl Claim {
    pub const EMPTY: Self = Self {
        generation: 0,
        volume: 0,
        parent_path: [0; MAX_PATH],
        parent_len: 0,
        stage_name: [0; MAX_NAME],
        stage_len: 0,
        target_name: [0; MAX_NAME],
        target_len: 0,
        backup_name: [0; MAX_NAME],
        backup_len: 0,
        identity: Identity {
            node: 0,
            generation: 0,
            size: 0,
        },
        protocol: 0,
    };
    pub fn parent(&self) -> &[u8] {
        &self.parent_path[..self.parent_len]
    }
    pub fn stage(&self) -> &[u8] {
        &self.stage_name[..self.stage_len]
    }
    pub fn target(&self) -> &[u8] {
        &self.target_name[..self.target_len]
    }
    pub fn backup(&self) -> &[u8] {
        &self.backup_name[..self.backup_len]
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Summary {
    pub published: usize,
    pub rolled_back: usize,
    pub retired: usize,
    pub foreign: usize,
    pub invalid: usize,
    pub failed: usize,
}
impl Summary {
    pub fn total(&self) -> usize {
        self.published + self.rolled_back + self.retired + self.foreign + self.invalid + self.failed
    }
    pub fn add(&mut self, status: u32) {
        match status {
            0 => self.published += 1,
            1 => self.rolled_back += 1,
            2 => self.retired += 1,
            3 => self.foreign += 1,
            4 => self.invalid += 1,
            5 => self.failed += 1,
            _ => panic!("Invalid claim replay receipt"),
        }
    }
}
pub fn checksum(data: &[u8]) -> u32 {
    data.iter()
        .fold(SEED, |h, b| (h ^ u32::from(*b)).wrapping_mul(16777619))
}
pub fn same(a: &[u8], b: &[u8]) -> bool {
    a.eq_ignore_ascii_case(b)
}
pub fn name_valid(v: &[u8]) -> bool {
    !v.is_empty()
        && v.len() <= MAX_NAME
        && v != b"."
        && v != b".."
        && !v.iter().any(|c| b"\\/:\0".contains(c))
}
pub fn parent_valid(v: &[u8]) -> bool {
    v.len() >= 3
        && v.len() <= MAX_PATH
        && v[1] == b':'
        && v[2] == b'\\'
        && !v.windows(2).any(|w| w == b"..")
        && !v.iter().any(|c| *c == 0 || *c == b'/')
}
pub fn valid(c: &Claim) -> bool {
    c.generation != 0
        && parent_valid(c.parent())
        && name_valid(c.stage())
        && name_valid(c.target())
        && (c.backup_len == 0 || name_valid(c.backup()))
        && !same(c.stage(), c.target())
        && (c.backup_len == 0 || (!same(c.backup(), c.stage()) && !same(c.backup(), c.target())))
}
pub fn protocol_text(value: u8) -> &'static [u8] {
    match value {
        0 => b"sftp",
        1 => b"scp",
        2 => b"ftp",
        3 => b"storage",
        _ => panic!("Invalid claim protocol"),
    }
}
fn protocol_parse(v: &[u8]) -> Option<u8> {
    (0..4).find(|p| protocol_text(*p) == v)
}
fn set(out: &mut [u8], len: &mut usize, value: &[u8]) -> bool {
    if value.len() > out.len() {
        return false;
    }
    out.fill(0);
    out[..value.len()].copy_from_slice(value);
    *len = value.len();
    true
}
#[repr(C)]
pub struct Build {
    pub generation: u64,
    pub volume: u64,
    pub parent: Bytes,
    pub stage: Bytes,
    pub target: Bytes,
    pub backup: Bytes,
    pub identity: Identity,
    pub protocol: u8,
}
/// # Safety
/// The raw span stays readable and its owner outlives every returned view.
pub unsafe fn bytes<'a>(v: Bytes) -> &'a [u8] {
    if v.len == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(v.data, v.len) }
    }
}
pub fn span(v: &[u8]) -> Bytes {
    Bytes {
        data: v.as_ptr(),
        len: v.len(),
    }
}
/// # Safety
/// Output is an exclusive initialized-or-writable exact Claim; inputs do not alias it.
pub unsafe fn build(out: *mut Claim, args: &Build) -> bool {
    unsafe {
        out.write(Claim::EMPTY);
        let c = &mut *out;
        c.generation = args.generation;
        c.volume = args.volume;
        c.identity = args.identity;
        c.protocol = args.protocol;
        set(&mut c.parent_path, &mut c.parent_len, bytes(args.parent))
            && set(&mut c.stage_name, &mut c.stage_len, bytes(args.stage))
            && set(&mut c.target_name, &mut c.target_len, bytes(args.target))
            && set(&mut c.backup_name, &mut c.backup_len, bytes(args.backup))
            && valid(c)
    }
}
fn line(out: &mut [u8], n: &mut usize, key: &[u8], value: &[u8]) -> Option<()> {
    let need = key.len() + 1 + value.len() + 1;
    if *n + need > out.len() {
        return None;
    }
    for v in [key, b"=", value, b"\n"] {
        out[*n..*n + v.len()].copy_from_slice(v);
        *n += v.len()
    }
    Some(())
}
fn number(out: &mut [u8], n: &mut usize, key: &[u8], mut v: u64) -> Option<()> {
    let mut digits = [0u8; 20];
    let mut len = 0;
    loop {
        digits[len] = b'0' + (v % 10) as u8;
        len += 1;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    digits[..len].reverse();
    line(out, n, key, &digits[..len])
}
pub fn serialize(c: &Claim, out: &mut [u8]) -> Option<usize> {
    if !valid(c) {
        return None;
    }
    let mut n = 0;
    line(out, &mut n, b"magic", b"R4U_CLAIM=1")?;
    number(out, &mut n, b"generation", c.generation)?;
    number(out, &mut n, b"volume", c.volume)?;
    line(out, &mut n, b"parent", c.parent())?;
    line(out, &mut n, b"stage", c.stage())?;
    line(out, &mut n, b"target", c.target())?;
    line(out, &mut n, b"backup", c.backup())?;
    number(out, &mut n, b"node", c.identity.node)?;
    number(out, &mut n, b"nodegen", u64::from(c.identity.generation))?;
    number(out, &mut n, b"size", c.identity.size)?;
    line(out, &mut n, b"protocol", protocol_text(c.protocol))?;
    let sum = checksum(&out[..n]);
    number(out, &mut n, b"checksum", u64::from(sum))?;
    Some(n)
}
fn parse_number(v: &[u8]) -> Option<u64> {
    if v.is_empty() || v.len() > 20 {
        return None;
    }
    v.iter().try_fold(0u64, |n, b| {
        if b.is_ascii_digit() {
            n.checked_mul(10)?.checked_add(u64::from(*b - b'0'))
        } else {
            None
        }
    })
}
fn field<'a>(data: &'a [u8], cursor: &mut usize, key: &[u8]) -> Option<&'a [u8]> {
    let end = *cursor + data.get(*cursor..)?.iter().position(|b| *b == b'\n')?;
    let line = &data[*cursor..end];
    *cursor = end + 1;
    let rest = line.strip_prefix(key)?;
    rest.strip_prefix(b"=")
}
pub fn parse(data: &[u8], c: &mut Claim) -> bool {
    *c = Claim::EMPTY;
    fn inner(data: &[u8], c: &mut Claim) -> Option<()> {
        if data.len() > CLAIM_MAX {
            return None;
        }
        let mut cur = 0;
        if field(data, &mut cur, b"magic")? != b"R4U_CLAIM=1" {
            return None;
        }
        c.generation = parse_number(field(data, &mut cur, b"generation")?)?;
        c.volume = parse_number(field(data, &mut cur, b"volume")?)?;
        for (name, buffer, len) in [
            (
                b"parent" as &[u8],
                &mut c.parent_path[..],
                &mut c.parent_len,
            ),
            (b"stage", &mut c.stage_name[..], &mut c.stage_len),
            (b"target", &mut c.target_name[..], &mut c.target_len),
            (b"backup", &mut c.backup_name[..], &mut c.backup_len),
        ] {
            if !set(buffer, len, field(data, &mut cur, name)?) {
                return None;
            }
        }
        c.identity.node = parse_number(field(data, &mut cur, b"node")?)?;
        c.identity.generation =
            u16::try_from(parse_number(field(data, &mut cur, b"nodegen")?)?).ok()?;
        c.identity.size = parse_number(field(data, &mut cur, b"size")?)?;
        c.protocol = protocol_parse(field(data, &mut cur, b"protocol")?)?;
        let body = cur;
        let want = u32::try_from(parse_number(field(data, &mut cur, b"checksum")?)?).ok()?;
        if want != checksum(&data[..body]) || cur != data.len() || !valid(c) {
            return None;
        }
        Some(())
    }
    inner(data, c).is_some()
}
/// Original I/O receipts: observe=0; publish=1; discardStage=2; retire=3.
/// Raw claim pointers remain resident; no metadata borrow spans a callback.
pub trait ReplayIo {
    /// # Safety
    /// The exact resident claim remains valid through the selected backend call.
    unsafe fn invoke(&mut self, op: u32, claim: *const Claim) -> u32;
}
/// # Safety
/// Claim and callback admission remain alive and exclusive through replay I/O.
pub unsafe fn replay(io: &mut impl ReplayIo, c: *const Claim) -> u32 {
    if !unsafe { valid(&*c) } {
        return 4;
    }
    match unsafe { io.invoke(0, c) } {
        0 => {
            if unsafe { io.invoke(2, c) } == 0 || unsafe { io.invoke(3, c) } == 0 {
                5
            } else {
                1
            }
        }
        1 | 2 => {
            if unsafe { io.invoke(1, c) } == 0 || unsafe { io.invoke(3, c) } == 0 {
                5
            } else {
                0
            }
        }
        3 => {
            if unsafe { io.invoke(3, c) } != 0 {
                0
            } else {
                5
            }
        }
        4 => {
            if unsafe { io.invoke(3, c) } != 0 {
                2
            } else {
                5
            }
        }
        5 => {
            if unsafe { io.invoke(3, c) } != 0 {
                3
            } else {
                5
            }
        }
        6 => 5,
        _ => panic!("Invalid claim observation"),
    }
}
#[repr(C)]
pub struct Io {
    pub context: *mut core::ffi::c_void,
    pub call: unsafe extern "C" fn(*mut core::ffi::c_void, u32, *const Claim) -> u32,
}
impl ReplayIo for Io {
    unsafe fn invoke(&mut self, op: u32, c: *const Claim) -> u32 {
        unsafe { (self.call)(self.context, op, c) }
    }
}
/// # Safety
/// Private fixed-layout records and bounded caller spans remain alive for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_claim_build(out: *mut Claim, args: *const Build) -> bool {
    unsafe { build(out, &*args) }
}
/// # Safety
/// Inputs are valid bounded spans and the operation selects their documented type.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_claim_value(op: u32, a: Bytes, b: Bytes, value: u32) -> u64 {
    unsafe {
        match op {
            0 => u64::from(checksum(bytes(a))),
            1 => u64::from(name_valid(bytes(a))),
            2 => u64::from(parent_valid(bytes(a))),
            3 => u64::from(same(bytes(a), bytes(b))),
            4 => u64::from(protocol_parse(bytes(a)).unwrap_or(4)),
            5 => value as u64,
            _ => panic!("Invalid claim value operation"),
        }
    }
}
/// # Safety
/// Caller owns the exact claim record and read/write span; no concurrent mutation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_claim_record(
    op: u32,
    c: *mut Claim,
    data: *mut u8,
    len: usize,
) -> usize {
    unsafe {
        match op {
            0 => usize::from(valid(&*c)),
            1 => serialize(&*c, core::slice::from_raw_parts_mut(data, len)).unwrap_or(usize::MAX),
            2 => usize::from(parse(bytes(Bytes { data, len }), &mut *c)),
            _ => panic!("Invalid claim record operation"),
        }
    }
}
/// # Safety
/// The exact typed callback and claim/summary records remain alive across all I/O.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_claim_replay(
    io: *mut Io,
    c: *const Claim,
    count: usize,
    summary: *mut Summary,
) -> u32 {
    unsafe {
        if summary.is_null() {
            return replay(&mut *io, c);
        }
        summary.write(Summary::default());
        for i in 0..count {
            let status = replay(&mut *io, c.add(i));
            (*summary).add(status)
        }
        0
    }
}
/// # Safety
/// Output is a live fixed-layout span record; returned bytes are static.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_claim_text(op: u32, value: u32, out: *mut Bytes) {
    let v = match op {
        0 => protocol_text(value as u8),
        1 => match value {
            0 => &b"published"[..],
            1 => b"rolled-back",
            2 => b"retired",
            3 => b"foreign",
            4 => b"invalid",
            5 => b"io",
            _ => panic!("Invalid claim status"),
        },
        2 => match value {
            0 => &b"stage-only"[..],
            1 => b"half-published",
            2 => b"alias",
            3 => b"target-only",
            4 => b"missing",
            5 => b"foreign",
            6 => b"io",
            _ => panic!("Invalid claim observation"),
        },
        _ => panic!("Invalid claim text operation"),
    };
    unsafe { out.write(span(v)) }
}

const _: () = {
    assert!(core::mem::size_of::<Claim>() == 3408);
    assert!(core::mem::offset_of!(Claim, identity) == 3376);
    assert!(core::mem::offset_of!(Claim, protocol) == 3400);
    assert!(core::mem::size_of::<Build>() == 112);
    assert!(core::mem::size_of::<Summary>() == 48);
    assert!(core::mem::size_of::<Io>() == 16);
};
