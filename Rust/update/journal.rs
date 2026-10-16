//! Fixed private C records and original SYSUPD journal v2--v5 bytes.
use super::claim::{bytes, span};
use crate::fs::vfs_types::Bytes;
pub const MAX_PATH: usize = 1024;
pub const MAX_VERSION: usize = 32;
pub const MAX_PACKAGE_PAYLOADS: usize = 32;
pub const MAX_PAYLOADS: usize = 34;
pub const JOURNAL_MAX: usize = 192 * 1024;
pub const SEED: u32 = 2166136261;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Payload {
    pub target_path: [u8; 1025],
    pub target_len: usize,
    pub stage_path: [u8; 1025],
    pub stage_len: usize,
    pub backup_path: [u8; 1025],
    pub backup_len: usize,
    pub previous_backup_path: [u8; 1025],
    pub previous_backup_len: usize,
    pub previous_backup_known: bool,
    pub previous_backup_size: u64,
    pub previous_backup_checksum: u32,
    pub size: u64,
    pub checksum: u32,
    pub target_existed: bool,
    pub old_known: bool,
    pub old_size: u64,
    pub old_checksum: u32,
    pub committed: bool,
    pub rolled_back: bool,
    pub replace_required: bool,
}
impl Payload {
    pub const EMPTY: Self = Self {
        target_path: [0; 1025],
        target_len: 0,
        stage_path: [0; 1025],
        stage_len: 0,
        backup_path: [0; 1025],
        backup_len: 0,
        previous_backup_path: [0; 1025],
        previous_backup_len: 0,
        previous_backup_known: false,
        previous_backup_size: 0,
        previous_backup_checksum: 0,
        size: 0,
        checksum: 0,
        target_existed: false,
        old_known: false,
        old_size: 0,
        old_checksum: 0,
        committed: false,
        rolled_back: false,
        replace_required: true,
    };
    pub fn target(&self) -> &[u8] {
        &self.target_path[..self.target_len]
    }
    pub fn stage(&self) -> &[u8] {
        &self.stage_path[..self.stage_len]
    }
    pub fn backup(&self) -> &[u8] {
        &self.backup_path[..self.backup_len]
    }
    pub fn previous(&self) -> &[u8] {
        &self.previous_backup_path[..self.previous_backup_len]
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Component {
    pub payload_index: u32,
    pub kind: u32,
    pub name: [u8; 50],
    pub name_len: usize,
    pub target: [u8; 1025],
    pub target_len: usize,
    pub version: [u8; 33],
    pub version_len: usize,
}
impl Component {
    pub const EMPTY: Self = Self {
        payload_index: 0,
        kind: 1,
        name: [0; 50],
        name_len: 0,
        target: [0; 1025],
        target_len: 0,
        version: [0; 33],
        version_len: 0,
    };
    pub fn name(&self) -> &[u8] {
        &self.name[..self.name_len]
    }
    pub fn target(&self) -> &[u8] {
        &self.target[..self.target_len]
    }
    pub fn version(&self) -> &[u8] {
        &self.version[..self.version_len]
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Journal {
    pub valid: bool,
    pub slot: u8,
    pub transaction_generation: u64,
    pub journal_generation: u64,
    pub phase: u32,
    pub package_digest: u32,
    pub package_length: u64,
    pub manifest_checksum: u32,
    pub component_digest: u32,
    pub component_count: u32,
    pub components_bound: bool,
    pub components: [Component; 32],
    pub foundation: bool,
    pub package_version: [u8; 33],
    pub package_version_len: usize,
    pub source_path: [u8; 1025],
    pub source_len: usize,
    pub source_version: [u8; 33],
    pub source_version_len: usize,
    pub target_version: [u8; 33],
    pub target_version_len: usize,
    pub package_payload_count: u32,
    pub payload_count: u32,
    pub committed_count: u32,
    pub rollback_count: u32,
    pub reboot: bool,
    pub batch: bool,
    pub rollback_markers_present: bool,
    pub payloads: [Payload; 34],
}
impl Journal {
    pub const EMPTY: Self = Self {
        valid: false,
        slot: 0,
        transaction_generation: 0,
        journal_generation: 0,
        phase: 0,
        package_digest: 0,
        package_length: 0,
        manifest_checksum: 0,
        component_digest: 0,
        component_count: 0,
        components_bound: true,
        components: [Component::EMPTY; 32],
        foundation: false,
        package_version: [0; 33],
        package_version_len: 0,
        source_path: [0; 1025],
        source_len: 0,
        source_version: [0; 33],
        source_version_len: 0,
        target_version: [0; 33],
        target_version_len: 0,
        package_payload_count: 0,
        payload_count: 0,
        committed_count: 0,
        rollback_count: 0,
        reboot: false,
        batch: false,
        rollback_markers_present: true,
        payloads: [Payload::EMPTY; 34],
    };
    pub fn source(&self) -> &[u8] {
        &self.source_path[..self.source_len]
    }
    pub fn source_version(&self) -> &[u8] {
        &self.source_version[..self.source_version_len]
    }
    pub fn target_version(&self) -> &[u8] {
        &self.target_version[..self.target_version_len]
    }
    pub fn package_version(&self) -> &[u8] {
        &self.package_version[..self.package_version_len]
    }
}
// Initialise the resident journal in place; never materialise its large value on a task stack.
/// # Safety
/// Journal is exclusive writable resident storage; initialization has no I/O.
pub unsafe fn reset(j: *mut Journal) {
    unsafe {
        core::ptr::write_bytes(j, 0, 1);
        (*j).components_bound = true;
        (*j).rollback_markers_present = true;
        for i in 0..MAX_PACKAGE_PAYLOADS {
            (*j).components[i].kind = 1
        }
        for i in 0..MAX_PAYLOADS {
            (*j).payloads[i].replace_required = true
        }
    }
}

pub fn checksum_update(seed: u32, data: &[u8]) -> u32 {
    data.iter()
        .fold(seed, |v, b| (v ^ u32::from(*b)).wrapping_mul(16777619))
}
pub fn checksum(data: &[u8]) -> u32 {
    checksum_update(SEED, data)
}
pub fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(&x, &y)| fold(x) == fold(y))
}
fn fold(b: u8) -> u8 {
    if b == b'/' {
        b'\\'
    } else {
        b.to_ascii_uppercase()
    }
}
fn prefix(v: &[u8], p: &[u8]) -> bool {
    v.len() >= p.len() && same(&v[..p.len()], p)
}
fn suffix(v: &[u8], p: &[u8]) -> bool {
    v.len() >= p.len() && same(&v[v.len() - p.len()..], p)
}
fn sep(b: u8) -> bool {
    b == b'/' || b == b'\\'
}
pub fn version_valid(v: &[u8]) -> bool {
    if v.is_empty() || v.len() >= MAX_VERSION {
        return false;
    }
    let mut digits = 0;
    let mut part = 0u32;
    for &b in v {
        if b.is_ascii_digit() {
            let Some(next) = part
                .checked_mul(10)
                .and_then(|n| n.checked_add(u32::from(b - b'0')))
            else {
                return false;
            };
            part = next;
            digits += 1;
            continue;
        }
        if b != b'.' || digits == 0 {
            return false;
        }
        digits = 0;
        part = 0
    }
    digits != 0
}
pub fn normalize(out: &mut [u8], v: &[u8]) -> Option<usize> {
    if out.len() < 2 || v.is_empty() {
        return None;
    }
    out.fill(0);
    let mut n = 0;
    fn put(out: &mut [u8], n: &mut usize, b: u8) -> Option<()> {
        if *n + 1 >= out.len() {
            return None;
        }
        out[*n] = b;
        *n += 1;
        Some(())
    }
    let mut pos = if v.len() >= 3 && sep(v[0]) && v[1].is_ascii_alphabetic() && sep(v[2]) {
        put(out, &mut n, v[1].to_ascii_uppercase())?;
        put(out, &mut n, b':')?;
        put(out, &mut n, b'\\')?;
        3
    } else if v.len() >= 2 && v[0].is_ascii_alphabetic() && v[1] == b':' {
        put(out, &mut n, v[0].to_ascii_uppercase())?;
        put(out, &mut n, b':')?;
        if v.len() > 2 && !sep(v[2]) {
            return None;
        }
        put(out, &mut n, b'\\')?;
        2
    } else if sep(v[0]) {
        put(out, &mut n, b'\\')?;
        1
    } else {
        return None;
    };
    while pos < v.len() {
        while pos < v.len() && sep(v[pos]) {
            pos += 1
        }
        if pos >= v.len() {
            break;
        }
        let start = pos;
        while pos < v.len() && !sep(v[pos]) {
            pos += 1
        }
        let c = &v[start..pos];
        if same(c, b".") {
            continue;
        }
        if same(c, b"..")
            || c.is_empty()
            || matches!(c[c.len() - 1], b' ' | b'.')
            || core::str::from_utf8(c).is_err()
        {
            return None;
        }
        if c.iter()
            .any(|b| *b < b' ' || *b == 0x7f || b"\"*:;<>?|".contains(b))
        {
            return None;
        }
        if n > 0 && out[n - 1] != b'\\' {
            put(out, &mut n, b'\\')?
        }
        for &b in c {
            put(out, &mut n, b)?
        }
    }
    if n == 0 || n + 1 > out.len() {
        return None;
    }
    out[n] = 0;
    Some(n)
}
pub fn internal_name(
    out: &mut [u8],
    target: &[u8],
    prefix: u8,
    generation: u64,
    index: usize,
) -> Option<usize> {
    if prefix != b'S' && prefix != b'B' {
        return None;
    }
    let parent = target.iter().rposition(|b| sep(*b)).map_or(0, |i| i + 1);
    if parent + 13 > out.len() {
        return None;
    }
    out.fill(0);
    out[..parent].copy_from_slice(&target[..parent]);
    out[parent] = prefix;
    let mixed =
        (generation.wrapping_mul(0x9e3779b9) ^ (index as u64).wrapping_mul(0x85ebca6b)) as u32;
    for i in 0..7 {
        out[parent + 1 + i] = b"0123456789ABCDEF"[((mixed >> ((6 - i) * 4)) & 15) as usize]
    }
    out[parent + 8..parent + 12].copy_from_slice(b".R4U");
    Some(parent + 12)
}
pub fn short_name(v: &[u8]) -> bool {
    if v.is_empty() || v.len() > 12 || v[0] == b'.' {
        return false;
    }
    let mut dot = None;
    for (i, &b) in v.iter().enumerate() {
        if b == b'.' {
            if dot.is_some() {
                return false;
            }
            dot = Some(i);
            continue;
        }
        let b = b.to_ascii_uppercase();
        if !b.is_ascii_uppercase() && !b.is_ascii_digit() && !b"$%'-_@~".contains(&b) {
            return false;
        }
    }
    let base = dot.unwrap_or(v.len());
    let ext = dot.map_or(0, |i| v.len() - i - 1);
    (1..=8).contains(&base) && ext <= 3 && (dot.is_none() || ext >= 1)
}
pub fn parent_path(v: &[u8]) -> &[u8] {
    for i in (2..=v.len()).rev() {
        if sep(v[i - 1]) {
            return &v[..i - 1];
        }
    }
    &v[..1]
}
pub fn base_name(v: &[u8]) -> &[u8] {
    v.iter().rposition(|b| sep(*b)).map_or(v, |i| &v[i + 1..])
}
fn target_valid(v: &[u8]) -> bool {
    if super::paths::companion_kind(v).is_some()
        || same(v, b"\\boot\\r4os.elf")
        || same(v, b"C:\\CONFIG.R4S")
    {
        return true;
    }
    for (p, s) in [
        (b"C:\\R4OS\\LIBS\\" as &[u8], &b".R4L"[..]),
        (b"C:\\R4OS\\DRIVERS\\", b".R4D"),
        (b"C:\\R4OS\\PROTOCOLS\\", b".R4P"),
        (b"C:\\R4OS\\SERVICES\\", b".R4X"),
        (b"C:\\R4OS\\SOFTWARE\\", b""),
        (b"C:\\R4OS\\SUBSYSTEMS\\", b".R4X"),
        (b"C:\\R4OS\\FONTS\\", b".R4F"),
        (b"C:\\R4OS\\CONFIG\\", b""),
        (b"C:\\R4OS\\SDK\\", b""),
        (b"C:\\TEMP\\", b""),
    ] {
        if prefix(v, p) && suffix(v, s) {
            return true;
        }
    }
    v.len() >= 4
        && v[0].is_ascii_alphabetic()
        && !v[0].eq_ignore_ascii_case(&b'C')
        && v[1] == b':'
        && v[2] == b'\\'
}
pub fn phase_text(v: u32) -> &'static [u8] {
    match v {
        0 => b"prepare",
        1 => b"stage",
        2 => b"commit",
        3 => b"verify",
        4 => b"inventory",
        5 => b"applied",
        6 => b"post_boot",
        7 => b"cleanup",
        8 => b"rollback",
        9 => b"rolled_back",
        _ => panic!("Invalid journal phase"),
    }
}
pub fn kind_text(v: u32) -> &'static [u8] {
    match v {
        0 => b"KERNEL",
        1 => b"R4X",
        2 => b"R4L",
        3 => b"R4D",
        4 => b"R4P",
        _ => panic!("Invalid journal component kind"),
    }
}
fn kind_parse(v: &[u8]) -> Option<u32> {
    (0..5).find(|k| v.eq_ignore_ascii_case(kind_text(*k)))
}
pub fn terminal(phase: u32) -> bool {
    phase == 7 || phase == 9
}
pub fn select(present: [bool; 2], valid: [bool; 2], generation: [u64; 2]) -> u32 {
    if (valid[0] && !present[0]) || (valid[1] && !present[1]) {
        return 3;
    }
    if valid[0] && valid[1] {
        return if generation[0] == generation[1] {
            3
        } else {
            u32::from(generation[0] < generation[1])
        };
    }
    if valid[0] {
        return 0;
    }
    if valid[1] {
        return 1;
    }
    if present[0] || present[1] { 3 } else { 2 }
}
impl Payload {
    pub fn path(&self, index: usize) -> &[u8] {
        match index {
            0 => self.target(),
            1 => self.stage(),
            2 => self.backup(),
            3 => self.previous(),
            _ => &[],
        }
    }
}
pub fn paths_valid(j: &Journal) -> bool {
    if !prefix(j.source(), b"C:\\R4OS\\UPDATE\\INBOX\\") {
        return false;
    }
    for i in 0..j.payload_count as usize {
        let e = &j.payloads[i];
        let target = e.target();
        let stage = e.stage();
        let backup = e.backup();
        let previous = e.previous();
        let mut s = [0; 1024];
        let mut b = [0; 1024];
        let Some(sn) = internal_name(&mut s, target, b'S', j.transaction_generation, i) else {
            return false;
        };
        let Some(bn) = internal_name(&mut b, target, b'B', j.transaction_generation, i) else {
            return false;
        };
        if !target_valid(target)
            || !short_name(base_name(stage))
            || !short_name(base_name(backup))
            || !same(parent_path(target), parent_path(stage))
            || !same(parent_path(target), parent_path(backup))
            || !same(stage, &s[..sn])
            || !same(backup, &b[..bn])
            || (!previous.is_empty()
                && (!short_name(base_name(previous))
                    || !same(parent_path(target), parent_path(previous))))
        {
            return false;
        }
        for p in 0..4 {
            let path = e.path(p);
            if path.is_empty() {
                continue;
            }
            if same(j.source(), path) {
                return false;
            }
            for other in i..j.payload_count as usize {
                let start = if other == i { p + 1 } else { 0 };
                for q in start..4 {
                    let peer = j.payloads[other].path(q);
                    if !peer.is_empty() && same(path, peer) {
                        return false;
                    }
                }
            }
        }
    }
    true
}
fn component_valid(c: &Component) -> bool {
    let name = c.name();
    let target = c.target();
    if name.is_empty()
        || name.len() > 48
        || name
            .iter()
            .any(|b| *b <= b' ' || *b >= 0x7f || b";=|".contains(b))
        || !version_valid(c.version())
        || target.is_empty()
        || target.len() >= MAX_PATH
        || target[0] != b'/'
        || target
            .iter()
            .any(|b| *b < 0x20 || *b >= 0x7f || b";=|\\".contains(b))
    {
        return false;
    }
    match c.kind {
        0 => same(name, b"KERNEL") && same(target, b"/boot/r4os.elf"),
        1 => suffix(target, b".R4X"),
        2 => suffix(target, b".R4L"),
        3 => suffix(target, b".R4D"),
        4 => suffix(target, b".R4P"),
        _ => false,
    }
}
pub fn valid(j: &Journal) -> bool {
    if !j.valid
        || j.slot > 1
        || j.transaction_generation == 0
        || j.journal_generation == 0
        || j.package_length == 0
        || j.package_payload_count == 0
        || j.package_payload_count > 32
        || j.payload_count == 0
        || j.payload_count > 34
        || j.payload_count < j.package_payload_count
        || j.payload_count > j.package_payload_count + if j.batch { 2 } else { 1 }
        || j.committed_count > j.payload_count
        || j.rollback_count > j.payload_count
        || j.component_count > j.package_payload_count
        || !version_valid(j.source_version())
        || !version_valid(j.target_version())
        || !version_valid(j.package_version())
        || !paths_valid(j)
    {
        return false;
    }
    if (j.batch && (!j.reboot || !j.components_bound)) || (j.phase == 6 && !j.batch) {
        return false;
    }
    if j.components_bound {
        for i in 0..j.component_count as usize {
            let c = &j.components[i];
            if c.payload_index >= j.package_payload_count || !component_valid(c) {
                return false;
            }
            for prior in &j.components[..i] {
                if (c.kind == prior.kind && same(c.name(), prior.name()))
                    || same(c.target(), prior.target())
                {
                    return false;
                }
            }
        }
    }
    let (mut committed, mut rolled) = (0, 0);
    let (mut saw_uncommitted, mut saw_rollback) = (false, false);
    for e in &j.payloads[..j.payload_count as usize] {
        if e.target_len == 0
            || e.stage_len == 0
            || e.backup_len == 0
            || (e.old_known && !e.target_existed)
            || (e.previous_backup_known && e.previous_backup_len == 0)
            || (e.committed && e.rolled_back)
            || (!e.target_existed && (e.old_known || e.old_size != 0 || e.old_checksum != 0))
            || (!e.previous_backup_known
                && (e.previous_backup_size != 0 || e.previous_backup_checksum != 0))
            || (!e.replace_required
                && (!e.target_existed
                    || (e.old_known && (e.old_size != e.size || e.old_checksum != e.checksum))))
        {
            return false;
        }
        if e.committed {
            if saw_uncommitted {
                return false;
            }
            committed += 1
        } else {
            saw_uncommitted = true
        }
        if e.rolled_back {
            saw_rollback = true;
            rolled += 1
        } else if saw_rollback {
            return false;
        }
    }
    if committed != j.committed_count || rolled != j.rollback_count {
        return false;
    }
    match j.phase {
        0 | 1 => committed == 0 && rolled == 0,
        2 => rolled == 0,
        3 => {
            j.payload_count == j.package_payload_count
                && committed == j.payload_count
                && rolled == 0
        }
        4 => {
            j.payload_count == j.package_payload_count + 1
                && (committed == j.package_payload_count || committed == j.payload_count)
                && rolled == 0
        }
        5..=7 => committed == j.payload_count && rolled == 0,
        8 => true,
        9 => {
            committed == 0
                && if j.rollback_markers_present {
                    rolled == j.payload_count
                } else {
                    j.payloads[..j.payload_count as usize]
                        .iter()
                        .all(|e| !e.replace_required || e.rolled_back)
                }
        }
        _ => false,
    }
}
struct Writer<'a> {
    out: &'a mut [u8],
    n: usize,
}
impl Writer<'_> {
    fn text(&mut self, v: &[u8]) -> Option<()> {
        if v.len() > self.out.len().saturating_sub(self.n) {
            return None;
        }
        self.out[self.n..self.n + v.len()].copy_from_slice(v);
        self.n += v.len();
        Some(())
    }
    fn byte(&mut self, v: u8) -> Option<()> {
        if self.n >= self.out.len() {
            return None;
        }
        self.out[self.n] = v;
        self.n += 1;
        Some(())
    }
    fn num(&mut self, mut v: u64) -> Option<()> {
        let mut digits = [0u8; 20];
        let mut n = 0;
        loop {
            digits[n] = b'0' + (v % 10) as u8;
            n += 1;
            v /= 10;
            if v == 0 {
                break;
            }
        }
        if n > self.out.len().saturating_sub(self.n) {
            return None;
        }
        for i in 0..n {
            self.out[self.n + i] = digits[n - i - 1]
        }
        self.n += n;
        Some(())
    }
    fn line(&mut self, v: &[u8]) -> Option<()> {
        self.text(v)?;
        self.byte(b'\n')
    }
    fn key(&mut self, k: &[u8], v: &[u8]) -> Option<()> {
        self.text(k)?;
        self.line(v)
    }
    fn key_num(&mut self, k: &[u8], v: u64) -> Option<()> {
        self.text(k)?;
        self.num(v)?;
        self.byte(b'\n')
    }
}
pub fn serialize(j: &Journal, out: &mut [u8]) -> Option<usize> {
    if !valid(j) {
        return None;
    }
    let mut w = Writer { out, n: 0 };
    w.line(b"R4U_JOURNAL=5")?;
    w.key_num(b"TRANSACTION_GENERATION=", j.transaction_generation)?;
    w.key_num(b"JOURNAL_GENERATION=", j.journal_generation)?;
    w.key(b"PHASE=", phase_text(j.phase))?;
    w.key_num(b"PACKAGE_DIGEST=", u64::from(j.package_digest))?;
    w.key_num(b"PACKAGE_LENGTH=", j.package_length)?;
    w.key_num(b"MANIFEST_CHECKSUM=", u64::from(j.manifest_checksum))?;
    w.key_num(b"COMPONENT_DIGEST=", u64::from(j.component_digest))?;
    w.key_num(b"COMPONENTS=", u64::from(j.component_count))?;
    w.key(
        b"COMPONENT_BINDING=",
        if j.components_bound {
            b"bound"
        } else {
            b"legacy"
        },
    )?;
    w.key(
        b"PRIORITY=",
        if j.foundation {
            b"foundation"
        } else {
            b"normal"
        },
    )?;
    w.key(b"PACKAGE_VERSION=", j.package_version())?;
    w.key(b"SOURCE_RELEASE=", j.source_version())?;
    w.key(b"RELEASE=", j.target_version())?;
    w.key(b"SOURCE=", j.source())?;
    for (k, v) in [
        (&b"PACKAGE_PAYLOADS="[..], j.package_payload_count),
        (b"PAYLOADS=", j.payload_count),
        (b"COMMITTED=", j.committed_count),
        (b"ROLLED_BACK=", j.rollback_count),
    ] {
        w.key_num(k, u64::from(v))?
    }
    w.key(b"REBOOT=", if j.reboot { b"yes" } else { b"no" })?;
    w.key(b"BATCH=", if j.batch { b"yes" } else { b"no" })?;
    if j.components_bound {
        for (i, c) in j.components[..j.component_count as usize]
            .iter()
            .enumerate()
        {
            w.text(b"COMPONENT;index=")?;
            w.num(i as u64)?;
            w.text(b";payload=")?;
            w.num(u64::from(c.payload_index))?;
            w.text(b";kind=")?;
            w.text(kind_text(c.kind))?;
            w.text(b";name=")?;
            w.text(c.name())?;
            w.text(b";target=")?;
            w.text(c.target())?;
            w.text(b";version=")?;
            w.text(c.version())?;
            w.byte(b'\n')?
        }
    }
    for (i, e) in j.payloads[..j.payload_count as usize].iter().enumerate() {
        w.text(b"PAYLOAD;index=")?;
        w.num(i as u64)?;
        w.text(b";target=")?;
        w.text(e.target())?;
        w.text(b";stage=")?;
        w.text(e.stage())?;
        w.text(b";backup=")?;
        w.text(e.backup())?;
        w.text(b";previous_backup=")?;
        w.text(e.previous())?;
        w.text(b";previous_known=")?;
        w.byte(if e.previous_backup_known { b'1' } else { b'0' })?;
        w.text(b";previous_size=")?;
        w.num(e.previous_backup_size)?;
        w.text(b";previous_checksum=")?;
        w.num(u64::from(e.previous_backup_checksum))?;
        w.text(b";size=")?;
        w.num(e.size)?;
        w.text(b";checksum=")?;
        w.num(u64::from(e.checksum))?;
        w.text(b";existed=")?;
        w.byte(if e.target_existed { b'1' } else { b'0' })?;
        w.text(b";old_known=")?;
        w.byte(if e.old_known { b'1' } else { b'0' })?;
        w.text(b";old_size=")?;
        w.num(e.old_size)?;
        w.text(b";old_checksum=")?;
        w.num(u64::from(e.old_checksum))?;
        w.text(b";required=")?;
        w.byte(if e.replace_required { b'1' } else { b'0' })?;
        w.text(b";committed=")?;
        w.byte(if e.committed { b'1' } else { b'0' })?;
        w.text(b";rolled_back=")?;
        w.byte(if e.rolled_back { b'1' } else { b'0' })?;
        w.byte(b'\n')?;
    }
    let sum = checksum(&w.out[..w.n]);
    w.key_num(b"CHECKSUM=", u64::from(sum))?;
    Some(w.n)
}
fn trim(v: &[u8]) -> &[u8] {
    let start = v
        .iter()
        .position(|b| !b" \t\r\n".contains(b))
        .unwrap_or(v.len());
    let end = v
        .iter()
        .rposition(|b| !b" \t\r\n".contains(b))
        .map_or(start, |i| i + 1);
    &v[start..end]
}
struct Lines<'a> {
    data: &'a [u8],
    pos: usize,
}
impl<'a> Iterator for Lines<'a> {
    type Item = &'a [u8];
    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.data.len() {
            return None;
        }
        let start = self.pos;
        let end = self.data[start..]
            .iter()
            .position(|b| *b == b'\n')
            .map_or(self.data.len(), |i| start + i);
        self.pos = if end < self.data.len() { end + 1 } else { end };
        Some(&self.data[start..end])
    }
}
fn lines(data: &[u8]) -> Lines<'_> {
    Lines { data, pos: 0 }
}
fn journal_field<'a>(data: &'a [u8], key: &[u8]) -> Option<&'a [u8]> {
    let mut found = None;
    for line in lines(data) {
        if let Some(v) = trim(line).strip_prefix(key) {
            if found.is_some() {
                return None;
            }
            found = Some(v)
        }
    }
    found
}
fn field<'a>(line: &'a [u8], key: &[u8]) -> Option<&'a [u8]> {
    let mut found = None;
    for part in line.split(|b| *b == b';') {
        if let Some(v) = part.strip_prefix(key) {
            if found.is_some() {
                return None;
            }
            found = Some(v)
        }
    }
    found
}
fn number(v: &[u8]) -> Option<u64> {
    let v = trim(v);
    if v.is_empty() {
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
fn num32(v: &[u8]) -> Option<u32> {
    u32::try_from(number(v)?).ok()
}
fn boolean(v: &[u8]) -> Option<bool> {
    match v {
        b"0" => Some(false),
        b"1" => Some(true),
        _ => None,
    }
}
fn yes_no(v: &[u8]) -> Option<bool> {
    if same(v, b"yes") {
        Some(true)
    } else if same(v, b"no") {
        Some(false)
    } else {
        None
    }
}
pub fn copy_z(out: &mut [u8], v: &[u8]) -> Option<usize> {
    if v.len() + 1 > out.len() {
        return None;
    }
    out.fill(0);
    out[..v.len()].copy_from_slice(v);
    Some(v.len())
}
fn known_lines(data: &[u8], modern: bool, component_format: bool, current: bool) -> bool {
    const COMMON: &[&[u8]] = &[
        b"R4U_JOURNAL=",
        b"TRANSACTION_GENERATION=",
        b"JOURNAL_GENERATION=",
        b"PHASE=",
        b"PACKAGE_DIGEST=",
        b"PACKAGE_LENGTH=",
        b"SOURCE=",
        b"PAYLOADS=",
        b"COMMITTED=",
        b"ROLLED_BACK=",
        b"REBOOT=",
    ];
    const MODERN: &[&[u8]] = &[
        b"MANIFEST_CHECKSUM=",
        b"COMPONENT_DIGEST=",
        b"COMPONENTS=",
        b"COMPONENT_BINDING=",
        b"PRIORITY=",
        b"PACKAGE_VERSION=",
        b"SOURCE_RELEASE=",
        b"RELEASE=",
    ];
    for raw in lines(data) {
        let line = trim(raw);
        if line.is_empty() {
            return false;
        }
        if line.starts_with(b"PAYLOAD;") {
            continue;
        }
        if line.starts_with(b"COMPONENT;") {
            if !component_format {
                return false;
            }
            continue;
        }
        if !COMMON.iter().any(|p| line.starts_with(p))
            && !(modern && MODERN.iter().any(|p| line.starts_with(p)))
            && !(component_format && line.starts_with(b"PACKAGE_PAYLOADS="))
            && !(current && line.starts_with(b"BATCH="))
            && !(!modern
                && (line.starts_with(b"SOURCE_VERSION=") || line.starts_with(b"TARGET_VERSION=")))
        {
            return false;
        }
    }
    true
}
fn known_fields(line: &[u8], component: bool) -> bool {
    const PAYLOAD: &[&[u8]] = &[
        b"index=",
        b"target=",
        b"stage=",
        b"backup=",
        b"previous_backup=",
        b"previous_known=",
        b"previous_size=",
        b"previous_checksum=",
        b"size=",
        b"checksum=",
        b"existed=",
        b"old_known=",
        b"old_size=",
        b"old_checksum=",
        b"required=",
        b"committed=",
        b"rolled_back=",
    ];
    const COMPONENT: &[&[u8]] = &[
        b"index=",
        b"payload=",
        b"kind=",
        b"name=",
        b"target=",
        b"version=",
    ];
    let allowed = if component { COMPONENT } else { PAYLOAD };
    let mut parts = line.split(|b| *b == b';');
    if parts.next()
        != Some(if component {
            &b"COMPONENT"[..]
        } else {
            &b"PAYLOAD"[..]
        })
    {
        return false;
    }
    if parts.any(|part| !allowed.iter().any(|k| part.starts_with(k))) {
        return false;
    }
    allowed.iter().all(|k| {
        let n = line
            .split(|b| *b == b';')
            .filter(|part| part.starts_with(k))
            .count();
        if component { n == 1 } else { n <= 1 }
    })
}
/// # Safety
/// The large journal is caller-owned resident storage; parsing performs no I/O.
pub unsafe fn parse(data: &[u8], slot: u8, journal: *mut Journal) -> bool {
    unsafe fn inner(data: &[u8], slot: u8, journal: *mut Journal) -> Option<()> {
        unsafe {
            if slot > 1 || data.is_empty() || data.len() > JOURNAL_MAX {
                return None;
            }
            let marker = data.windows(9).rposition(|w| w == b"CHECKSUM=")?;
            if marker > 0 && data[marker - 1] != b'\n' {
                return None;
            }
            let end = marker + data[marker..].iter().position(|b| *b == b'\n')?;
            if end + 1 != data.len()
                || checksum(&data[..marker]) != num32(trim(&data[marker + 9..end]))?
            {
                return None;
            }
            let jf = |key: &[u8]| journal_field(data, key);
            let version = jf(b"R4U_JOURNAL=")?;
            let current = same(version, b"5");
            let component_format = current || same(version, b"4");
            let prior = same(version, b"3");
            let modern = component_format || prior;
            if (!modern && !same(version, b"2"))
                || !known_lines(&data[..marker], modern, component_format, current)
            {
                return None;
            }
            let reboot = yes_no(jf(b"REBOOT=")?)?;
            let foundation = if modern {
                let v = jf(b"PRIORITY=")?;
                if same(v, b"foundation") {
                    true
                } else if same(v, b"normal") {
                    false
                } else {
                    return None;
                }
            } else {
                false
            };
            // All scalar initializer expressions were evaluated before the original
            // whole-record assignment. Preserve that failure boundary without a copy.
            let transaction_generation = number(jf(b"TRANSACTION_GENERATION=").unwrap_or(b""))?;
            let journal_generation = number(jf(b"JOURNAL_GENERATION=").unwrap_or(b""))?;
            let phase = (0..10).find(|p| same(jf(b"PHASE=").unwrap_or(b""), phase_text(*p)))?;
            let package_digest = num32(jf(b"PACKAGE_DIGEST=").unwrap_or(b""))?;
            let package_length = number(jf(b"PACKAGE_LENGTH=").unwrap_or(b""))?;
            let manifest_checksum = if modern {
                num32(jf(b"MANIFEST_CHECKSUM=").unwrap_or(b""))?
            } else {
                0
            };
            let component_digest = if modern {
                num32(jf(b"COMPONENT_DIGEST=").unwrap_or(b""))?
            } else {
                0
            };
            let component_count = if modern {
                num32(jf(b"COMPONENTS=").unwrap_or(b""))?
            } else {
                0
            };
            let components_bound = if component_format {
                let v = jf(b"COMPONENT_BINDING=")?;
                if same(v, b"bound") {
                    true
                } else if same(v, b"legacy") {
                    false
                } else {
                    return None;
                }
            } else {
                false
            };
            let package_payload_count = num32(
                jf(if component_format {
                    b"PACKAGE_PAYLOADS="
                } else {
                    b"PAYLOADS="
                })
                .unwrap_or(b""),
            )?;
            let payload_count = num32(jf(b"PAYLOADS=").unwrap_or(b""))?;
            let committed_count = num32(jf(b"COMMITTED=").unwrap_or(b""))?;
            let rollback_count = num32(jf(b"ROLLED_BACK=").unwrap_or(b""))?;
            let batch = if current {
                yes_no(jf(b"BATCH=")?)?
            } else {
                false
            };
            reset(journal);
            let j = &mut *journal;
            j.valid = true;
            j.slot = slot;
            j.transaction_generation = transaction_generation;
            j.journal_generation = journal_generation;
            j.phase = phase;
            j.package_digest = package_digest;
            j.package_length = package_length;
            j.manifest_checksum = manifest_checksum;
            j.component_digest = component_digest;
            j.component_count = component_count;
            j.components_bound = components_bound;
            j.foundation = foundation;
            j.package_payload_count = package_payload_count;
            j.payload_count = payload_count;
            j.committed_count = committed_count;
            j.rollback_count = rollback_count;
            j.reboot = reboot;
            j.batch = batch;
            if j.transaction_generation == 0
                || j.journal_generation == 0
                || j.payload_count == 0
                || j.payload_count > 34
                || j.package_payload_count == 0
                || j.package_payload_count > 32
                || j.payload_count < j.package_payload_count
                || j.payload_count > j.package_payload_count + if j.batch { 2 } else { 1 }
            {
                return None;
            }
            j.source_len = normalize(&mut j.source_path[..MAX_PATH], jf(b"SOURCE=")?)?;
            j.source_version_len = copy_z(
                &mut j.source_version[..MAX_VERSION],
                jf(if modern {
                    b"SOURCE_RELEASE="
                } else {
                    b"SOURCE_VERSION="
                })?,
            )?;
            j.target_version_len = copy_z(
                &mut j.target_version[..MAX_VERSION],
                jf(if modern {
                    b"RELEASE="
                } else {
                    b"TARGET_VERSION="
                })?,
            )?;
            j.package_version_len = if modern {
                copy_z(
                    &mut j.package_version[..MAX_VERSION],
                    jf(b"PACKAGE_VERSION=")?,
                )?
            } else {
                copy_z(
                    &mut j.package_version[..MAX_VERSION],
                    &j.target_version[..j.target_version_len],
                )?
            };
            if !version_valid(j.source_version())
                || !version_valid(j.target_version())
                || !version_valid(j.package_version())
                || j.component_count > j.package_payload_count
            {
                return None;
            }
            let (mut seen_payloads, mut seen_components) = ([false; 34], [false; 32]);
            let (mut found, mut found_components, mut marker_count) = (0u32, 0u32, 0u32);
            for raw in lines(&data[..marker]) {
                let line = trim(raw);
                if line.starts_with(b"COMPONENT;") {
                    if !component_format || !known_fields(line, true) {
                        return None;
                    }
                    let i = num32(field(line, b"index=")?)? as usize;
                    if i >= j.component_count as usize || seen_components[i] {
                        return None;
                    }
                    seen_components[i] = true;
                    let c = &mut j.components[i];
                    c.payload_index = num32(field(line, b"payload=")?)?;
                    c.kind = kind_parse(field(line, b"kind=")?)?;
                    c.name_len = copy_z(&mut c.name[..49], field(line, b"name=")?)?;
                    c.target_len = copy_z(&mut c.target[..MAX_PATH], field(line, b"target=")?)?;
                    c.version_len =
                        copy_z(&mut c.version[..MAX_VERSION], field(line, b"version=")?)?;
                    if c.payload_index >= j.package_payload_count || !component_valid(c) {
                        return None;
                    }
                    found_components += 1;
                    continue;
                }
                if !line.starts_with(b"PAYLOAD;") {
                    continue;
                }
                if !known_fields(line, false) {
                    return None;
                }
                let i = num32(field(line, b"index=")?)? as usize;
                if i >= j.payload_count as usize || seen_payloads[i] {
                    return None;
                }
                seen_payloads[i] = true;
                let e = &mut j.payloads[i];
                e.target_len = normalize(&mut e.target_path[..MAX_PATH], field(line, b"target=")?)?;
                e.stage_len = normalize(&mut e.stage_path[..MAX_PATH], field(line, b"stage=")?)?;
                e.backup_len = normalize(&mut e.backup_path[..MAX_PATH], field(line, b"backup=")?)?;
                let previous = field(line, b"previous_backup=").unwrap_or(b"");
                e.previous_backup_len = if previous.is_empty() {
                    0
                } else {
                    normalize(&mut e.previous_backup_path[..MAX_PATH], previous)?
                };
                if let Some(v) = field(line, b"previous_known=") {
                    let known = boolean(v)?;
                    let size = field(line, b"previous_size=");
                    let sum = field(line, b"previous_checksum=");
                    if known {
                        e.previous_backup_known = true;
                        e.previous_backup_size = number(size?)?;
                        e.previous_backup_checksum = num32(sum?)?
                    } else {
                        if let Some(v) = size
                            && number(v)? != 0
                        {
                            return None;
                        }
                        if let Some(v) = sum
                            && num32(v)? != 0
                        {
                            return None;
                        }
                    }
                } else if field(line, b"previous_size=").is_some()
                    || field(line, b"previous_checksum=").is_some()
                {
                    return None;
                }
                if e.previous_backup_known && e.previous_backup_len == 0 {
                    return None;
                }
                e.size = number(field(line, b"size=")?)?;
                e.checksum = num32(field(line, b"checksum=")?)?;
                e.target_existed = boolean(field(line, b"existed=")?)?;
                if let Some(v) = field(line, b"old_known=") {
                    let known = boolean(v)?;
                    let size = field(line, b"old_size=");
                    let sum = field(line, b"old_checksum=");
                    if known {
                        e.old_known = true;
                        e.old_size = number(size?)?;
                        e.old_checksum = num32(sum?)?
                    } else {
                        if let Some(v) = size
                            && number(v)? != 0
                        {
                            return None;
                        }
                        if let Some(v) = sum
                            && num32(v)? != 0
                        {
                            return None;
                        }
                    }
                } else if field(line, b"old_size=").is_some()
                    || field(line, b"old_checksum=").is_some()
                {
                    return None;
                }
                if e.old_known && !e.target_existed {
                    return None;
                }
                e.replace_required = boolean(field(line, b"required=")?)?;
                e.committed = boolean(field(line, b"committed=")?)?;
                if let Some(v) = field(line, b"rolled_back=") {
                    marker_count += 1;
                    e.rolled_back = boolean(v)?
                }
                if (e.committed && e.rolled_back)
                    || (!e.replace_required
                        && (!e.target_existed
                            || (e.old_known
                                && (e.old_size != e.size || e.old_checksum != e.checksum))))
                {
                    return None;
                }
                found += 1;
            }
            if (component_format && j.components_bound && found_components != j.component_count)
                || (component_format && !j.components_bound && found_components != 0)
                || found != j.payload_count
                || (marker_count != 0 && marker_count != j.payload_count)
            {
                return None;
            }
            j.rollback_markers_present = marker_count != 0;
            if marker_count == 0 && j.rollback_count != 0 {
                let mut remaining = j.rollback_count;
                let mut cursor = j.payload_count as usize;
                while cursor > 0 && remaining > 0 {
                    cursor -= 1;
                    j.payloads[cursor].rolled_back = true;
                    if j.payloads[cursor].replace_required {
                        remaining -= 1
                    }
                }
                if remaining != 0 {
                    return None;
                }
                j.rollback_count = j.payloads[..j.payload_count as usize]
                    .iter()
                    .filter(|e| e.rolled_back)
                    .count() as u32
            }
            if !valid(j) {
                return None;
            }
            Some(())
        }
    }
    unsafe { inner(data, slot, journal).is_some() }
}
#[repr(C)]
pub struct ValueArgs {
    pub a: Bytes,
    pub b: Bytes,
    pub out: *mut u8,
    pub capacity: usize,
    pub generation: u64,
    pub index: usize,
    pub value: u32,
    pub present: [bool; 2],
    pub valid: [bool; 2],
    pub generations: [u64; 2],
}
/// # Safety
/// All selected spans/records are live and retain the fixed private C layout.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_journal_value(op: u32, args: *const ValueArgs) -> u64 {
    unsafe {
        let a = &*args;
        match op {
            0 => u64::from(checksum_update(a.value, bytes(a.a))),
            1 => u64::from(version_valid(bytes(a.a))),
            2 => u64::from(same(bytes(a.a), bytes(a.b))),
            3 => u64::from(short_name(bytes(a.a))),
            4 => normalize(
                core::slice::from_raw_parts_mut(a.out, a.capacity),
                bytes(a.a),
            )
            .map_or(u64::MAX, |n| n as u64),
            5 => internal_name(
                core::slice::from_raw_parts_mut(a.out, a.capacity),
                bytes(a.a),
                a.value as u8,
                a.generation,
                a.index,
            )
            .map_or(u64::MAX, |n| n as u64),
            6 => u64::from(select(a.present, a.valid, a.generations)),
            7 => u64::from(terminal(a.value)),
            8 => u64::from(kind_parse(bytes(a.a)).unwrap_or(5)),
            9 => copy_z(
                core::slice::from_raw_parts_mut(a.out, a.capacity),
                bytes(a.a),
            )
            .map_or(u64::MAX, |n| n as u64),
            _ => panic!("Invalid journal scalar operation"),
        }
    }
}
/// # Safety
/// The journal and bounded data buffer have the documented access for this op.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_journal_record(
    op: u32,
    j: *mut Journal,
    data: *mut u8,
    len: usize,
    slot: u8,
) -> usize {
    unsafe {
        match op {
            0 => usize::from(valid(&*j)),
            1 => usize::from(paths_valid(&*j)),
            2 => serialize(&*j, core::slice::from_raw_parts_mut(data, len)).unwrap_or(usize::MAX),
            3 => usize::from(parse(bytes(Bytes { data, len }), slot, j)),
            _ => panic!("Invalid journal record operation"),
        }
    }
}
/// # Safety
/// Output retains a fixed-layout span; all returned text has static lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_journal_text(op: u32, value: u32, out: *mut Bytes) {
    let v = match op {
        0 => phase_text(value),
        1 => kind_text(value),
        2 => match value {
            0 => &b"ok"[..],
            1 => b"journal-invalid",
            2 => b"state-conflict",
            3 => b"storage-io",
            _ => panic!("Invalid recovery status"),
        },
        _ => panic!("Invalid journal text operation"),
    };
    unsafe { out.write(span(v)) }
}

const _: () = {
    assert!(core::mem::size_of::<Payload>() == 4216);
    assert!(core::mem::offset_of!(Payload, target_path) == 0);
    assert!(core::mem::offset_of!(Payload, target_len) == 1032);
    assert!(core::mem::offset_of!(Payload, stage_path) == 1040);
    assert!(core::mem::offset_of!(Payload, stage_len) == 2072);
    assert!(core::mem::offset_of!(Payload, backup_path) == 2080);
    assert!(core::mem::offset_of!(Payload, backup_len) == 3112);
    assert!(core::mem::offset_of!(Payload, previous_backup_path) == 3120);
    assert!(core::mem::offset_of!(Payload, previous_backup_len) == 4152);
    assert!(core::mem::offset_of!(Payload, previous_backup_known) == 4160);
    assert!(core::mem::offset_of!(Payload, previous_backup_size) == 4168);
    assert!(core::mem::offset_of!(Payload, previous_backup_checksum) == 4176);
    assert!(core::mem::offset_of!(Payload, size) == 4184);
    assert!(core::mem::offset_of!(Payload, checksum) == 4192);
    assert!(core::mem::offset_of!(Payload, target_existed) == 4196);
    assert!(core::mem::offset_of!(Payload, old_known) == 4197);
    assert!(core::mem::offset_of!(Payload, old_size) == 4200);
    assert!(core::mem::offset_of!(Payload, old_checksum) == 4208);
    assert!(core::mem::offset_of!(Payload, committed) == 4212);
    assert!(core::mem::offset_of!(Payload, rolled_back) == 4213);
    assert!(core::mem::offset_of!(Payload, replace_required) == 4214);
    assert!(core::mem::size_of::<Component>() == 1160);
    assert!(core::mem::offset_of!(Component, payload_index) == 0);
    assert!(core::mem::offset_of!(Component, kind) == 4);
    assert!(core::mem::offset_of!(Component, name) == 8);
    assert!(core::mem::offset_of!(Component, name_len) == 64);
    assert!(core::mem::offset_of!(Component, target) == 72);
    assert!(core::mem::offset_of!(Component, target_len) == 1104);
    assert!(core::mem::offset_of!(Component, version) == 1112);
    assert!(core::mem::offset_of!(Component, version_len) == 1152);
    assert!(core::mem::size_of::<Journal>() == 181728);
    assert!(core::mem::offset_of!(Journal, valid) == 0);
    assert!(core::mem::offset_of!(Journal, slot) == 1);
    assert!(core::mem::offset_of!(Journal, transaction_generation) == 8);
    assert!(core::mem::offset_of!(Journal, journal_generation) == 16);
    assert!(core::mem::offset_of!(Journal, phase) == 24);
    assert!(core::mem::offset_of!(Journal, package_digest) == 28);
    assert!(core::mem::offset_of!(Journal, package_length) == 32);
    assert!(core::mem::offset_of!(Journal, manifest_checksum) == 40);
    assert!(core::mem::offset_of!(Journal, component_digest) == 44);
    assert!(core::mem::offset_of!(Journal, component_count) == 48);
    assert!(core::mem::offset_of!(Journal, components_bound) == 52);
    assert!(core::mem::offset_of!(Journal, components) == 56);
    assert!(core::mem::offset_of!(Journal, foundation) == 37176);
    assert!(core::mem::offset_of!(Journal, package_version) == 37177);
    assert!(core::mem::offset_of!(Journal, package_version_len) == 37216);
    assert!(core::mem::offset_of!(Journal, source_path) == 37224);
    assert!(core::mem::offset_of!(Journal, source_len) == 38256);
    assert!(core::mem::offset_of!(Journal, source_version) == 38264);
    assert!(core::mem::offset_of!(Journal, source_version_len) == 38304);
    assert!(core::mem::offset_of!(Journal, target_version) == 38312);
    assert!(core::mem::offset_of!(Journal, target_version_len) == 38352);
    assert!(core::mem::offset_of!(Journal, package_payload_count) == 38360);
    assert!(core::mem::offset_of!(Journal, payload_count) == 38364);
    assert!(core::mem::offset_of!(Journal, committed_count) == 38368);
    assert!(core::mem::offset_of!(Journal, rollback_count) == 38372);
    assert!(core::mem::offset_of!(Journal, reboot) == 38376);
    assert!(core::mem::offset_of!(Journal, batch) == 38377);
    assert!(core::mem::offset_of!(Journal, rollback_markers_present) == 38378);
    assert!(core::mem::offset_of!(Journal, payloads) == 38384);
};

const _: () = {
    assert!(core::mem::size_of::<ValueArgs>() == 88);
    assert!(core::mem::offset_of!(ValueArgs, generations) == 72);
};
