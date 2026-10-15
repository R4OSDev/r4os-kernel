//! Native installation manifest owner. Generic Zig std JSON decoding remains
//! a temporary stateless substrate; every R4OS rule and match is owned here.
use crate::storage::{
    guid,
    partition_table::{self, Label, Partition},
};
use core::{ffi::c_void, ptr, slice};
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Part {
    pub partition_guid: [u8; 16],
    pub type_guid: [u8; 16],
    pub first_lba: u64,
    pub sector_count: u64,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Manifest {
    pub installation_id: [u8; 16],
    pub disk_guid: [u8; 16],
    pub partitions: [Part; 5],
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct TableView {
    pub valid: bool,
    pub kind: u8,
    pub disk_guid: [u8; 16],
    pub partitions: *const Partition,
    pub count: usize,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct JsonMeta {
    pub found: bool,
    pub kind: u8,
    pub child: *const c_void,
    pub text: *const u8,
    pub len: usize,
}
impl JsonMeta {
    const EMPTY: Self = Self {
        found: false,
        kind: 0,
        child: ptr::null(),
        text: ptr::null(),
        len: 0,
    };
}
type JsonReader = unsafe extern "C" fn(*const c_void, u8, *const u8, usize, usize, *mut JsonMeta);
#[derive(Clone, Copy)]
struct Node {
    handle: *const c_void,
    read: JsonReader,
}
#[repr(i32)]
#[derive(Clone, Copy)]
enum Error {
    ManifestSize = 1,
    UnsupportedSchema = 2,
    UnsupportedSectorSize = 3,
    ExpectedObject = 4,
    MissingField = 5,
    ExpectedString = 6,
    InvalidGuid = 7,
    ExpectedInteger = 8,
    IntegerOverflow = 9,
    PartitionRoles = 10,
    PartitionRange = 11,
    PartitionType = 12,
    DuplicatePartitionGuid = 13,
    PartitionOverlap = 14,
    BootFiles = 15,
    BootPath = 16,
    DuplicateBootPath = 17,
    Version = 18,
}
impl Node {
    fn query(self, op: u8, key: &[u8], index: usize) -> JsonMeta {
        let mut out = JsonMeta::EMPTY;
        unsafe { (self.read)(self.handle, op, key.as_ptr(), key.len(), index, &mut out) };
        out
    }
    fn meta(self) -> JsonMeta {
        self.query(0, &[], 0)
    }
    fn field(self, key: &[u8]) -> Result<Self, Error> {
        if self.meta().kind != 1 {
            return Err(Error::ExpectedObject);
        }
        let meta = self.query(1, key, 0);
        if !meta.found {
            return Err(Error::MissingField);
        }
        Ok(Self {
            handle: meta.child,
            read: self.read,
        })
    }
    fn at(self, index: usize) -> Self {
        let m = self.query(2, &[], index);
        assert!(m.found);
        Self {
            handle: m.child,
            read: self.read,
        }
    }
    fn string<'a>(self) -> Result<&'a [u8], Error> {
        let m = self.meta();
        if m.kind != 3 {
            return Err(Error::ExpectedString);
        }
        Ok(if m.len == 0 {
            &[]
        } else {
            unsafe { slice::from_raw_parts(m.text, m.len) }
        })
    }
    fn uuid(self) -> Result<[u8; 16], Error> {
        let text = self.string()?;
        let mut out = [0; 16];
        if !unsafe {
            guid::r4os_rust_storage_guid_parse(text.as_ptr(), text.len(), out.as_mut_ptr())
        } || out == [0; 16]
        {
            return Err(Error::InvalidGuid);
        }
        Ok(out)
    }
    fn uint(self) -> Result<u64, Error> {
        let m = self.meta();
        if m.kind != 4 || m.len == 0 {
            return Err(Error::ExpectedInteger);
        }
        let text = unsafe { slice::from_raw_parts(m.text, m.len) };
        // Original validation checks every byte before parsing the number;
        // a suffix error takes precedence over overflow in an earlier prefix.
        if !text.iter().all(u8::is_ascii_digit) {
            return Err(Error::ExpectedInteger);
        }
        let mut value = 0u64;
        for digit in text {
            value = value
                .checked_mul(10)
                .and_then(|n| n.checked_add(u64::from(*digit - b'0')))
                .ok_or(Error::IntegerOverflow)?;
        }
        Ok(value)
    }
}
fn version(text: &[u8]) -> bool {
    if text.is_empty() || text.len() > 63 {
        return false;
    }
    let mut dots = 0;
    let mut digits = 0;
    for c in text {
        if *c == b'.' {
            if digits == 0 {
                return false;
            }
            dots += 1;
            digits = 0;
        } else if c.is_ascii_digit() {
            digits += 1;
        } else {
            return false;
        }
    }
    dots == 2 && digits != 0
}
fn boot_path(path: &[u8]) -> bool {
    if path.is_empty() || path.len() > 255 {
        return false;
    }
    for part in path.split(|b| *b == b'/') {
        if part.is_empty() || part == b"." || part == b".." {
            return false;
        }
        if part.iter().any(|c| *c < 32 || *c == b':' || *c == b'\\') {
            return false;
        }
    }
    true
}
fn parse(root: Node) -> Result<Manifest, Error> {
    if root.field(b"schema")?.uint()? != 1 {
        return Err(Error::UnsupportedSchema);
    }
    if root.field(b"logicalSectorBytes")?.uint()? != 512 {
        return Err(Error::UnsupportedSectorSize);
    }
    let mut out = Manifest {
        installation_id: root.field(b"installationId")?.uuid()?,
        disk_guid: root.field(b"diskGuid")?.uuid()?,
        ..Manifest::default()
    };
    let parts = root.field(b"partitions")?;
    let meta = parts.meta();
    if meta.kind != 1 || meta.len != 5 {
        return Err(Error::PartitionRoles);
    }
    for (i, name) in [
        b"BIOSBOOT".as_slice(),
        b"BOOT",
        b"SYSTEM",
        b"RECOVERY",
        b"DATA",
    ]
    .into_iter()
    .enumerate()
    {
        let p = parts.field(name)?;
        out.partitions[i] = Part {
            partition_guid: p.field(b"partitionGuid")?.uuid()?,
            type_guid: p.field(b"typeGuid")?.uuid()?,
            first_lba: p.field(b"firstLba")?.uint()?,
            sector_count: p.field(b"sectorCount")?.uint()?,
        };
        let p = out.partitions[i];
        if p.first_lba == 0 || p.sector_count == 0 || p.sector_count > u64::MAX - p.first_lba {
            return Err(Error::PartitionRange);
        }
        let expected = match i {
            0 => partition_table::BIOS,
            1 => crate::storage::gpt::ESP,
            _ => crate::storage::gpt::BASIC,
        };
        if p.type_guid != expected {
            return Err(Error::PartitionType);
        }
    }
    for (i, p) in out.partitions.iter().enumerate() {
        for other in &out.partitions[..i] {
            if p.partition_guid == other.partition_guid {
                return Err(Error::DuplicatePartitionGuid);
            }
            if p.first_lba < other.first_lba + other.sector_count
                && other.first_lba < p.first_lba + p.sector_count
            {
                return Err(Error::PartitionOverlap);
            }
        }
    }
    let files = root.field(b"bootFiles")?;
    let meta = files.meta();
    if meta.kind != 2 || meta.len == 0 || meta.len > 64 {
        return Err(Error::BootFiles);
    }
    for i in 0..meta.len {
        let path = files.at(i).string()?;
        if !boot_path(path) {
            return Err(Error::BootPath);
        }
        for j in 0..i {
            if path.eq_ignore_ascii_case(files.at(j).string()?) {
                return Err(Error::DuplicateBootPath);
            }
        }
    }
    if !version(root.field(b"releaseVersion")?.string()?)
        || !version(root.field(b"kernelVersion")?.string()?)
    {
        return Err(Error::Version);
    }
    Ok(out)
}
impl Manifest {
    /// # Safety
    /// View's initialized C partition array stays readable through this
    /// non-waiting match. The caller owns boot/maintenance admission.
    pub unsafe fn matches(&self, table: TableView, boot: [u8; 16]) -> bool {
        if !table.valid
            || table.kind != 2
            || self.disk_guid != table.disk_guid
            || self.partitions[1].partition_guid != boot
        {
            return false;
        }
        let items = if table.count == 0 {
            &[]
        } else {
            unsafe { slice::from_raw_parts(table.partitions, table.count) }
        };
        for part in &self.partitions {
            let mut found = false;
            for p in items {
                if part.partition_guid != p.unique_guid {
                    continue;
                }
                if part.type_guid != p.type_guid
                    || part.first_lba != p.first_lba
                    || part.sector_count != p.sector_count
                {
                    return false;
                }
                found = true;
            }
            if !found {
                return false;
            }
        }
        true
    }
}
/// # Safety
/// Input owns len readable resident bytes; output is one exclusive C byte
/// view into that same input, retained only through generic JSON decoding.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_installation_text(
    input: *const u8,
    len: usize,
    out: *mut Label,
) -> i32 {
    if len == 0 || len > 16384 {
        return Error::ManifestSize as i32;
    }
    let bytes = unsafe { slice::from_raw_parts(input, len) };
    let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
    unsafe {
        out.write(Label {
            data: bytes.as_ptr(),
            len: bytes.len(),
        })
    };
    0
}
/// # Safety
/// Root and all nodes/strings remain alive in the immutable generic JSON
/// document until this call returns. Reader copies only fixed C metadata,
/// never waits or mutates the DOM; output is exclusive aligned Manifest.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_installation_parse(
    root: *const c_void,
    read: JsonReader,
    out: *mut Manifest,
) -> i32 {
    match parse(Node { handle: root, read }) {
        Ok(m) => {
            unsafe { out.write(m) };
            0
        }
        Err(e) => e as i32,
    }
}
/// # Safety
/// Initialized manifest/table/UUID are readable and disjoint from mutation;
/// table partition storage retains the exact boot/maintenance lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_installation_matches(
    manifest: *const Manifest,
    table: *const TableView,
    boot: *const u8,
) -> bool {
    unsafe { (&*manifest).matches(table.read(), boot.cast::<[u8; 16]>().read()) }
}
const _: () = {
    assert!(core::mem::size_of::<Part>() == 48);
    assert!(core::mem::size_of::<Manifest>() == 272);
    assert!(core::mem::size_of::<TableView>() == 40);
    assert!(core::mem::size_of::<JsonMeta>() == 32);
};
