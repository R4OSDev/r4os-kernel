//! Private pure C codec dispatch; no module ABI or NTFS wire layout changes.
use super::{
    ntfs_format_lznt1 as l, ntfs_format_names as n, ntfs_format_records as r,
    ntfs_format_runs as runs, ntfs_format_types::*,
};
#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct Result {
    pub value: u64,
    pub code: i32,
    pub present: bool,
}
impl Result {
    fn boolean(v: bool) -> Self {
        Self {
            value: u64::from(v),
            code: 0,
            present: v,
        }
    }
    fn optional(v: Option<usize>) -> Self {
        v.map_or(Self::default(), |v| Self {
            value: v as u64,
            code: 0,
            present: true,
        })
    }
    fn value(v: u64) -> Self {
        Self {
            value: v,
            code: 0,
            present: true,
        }
    }
}
/// # Safety
/// Resident readable argument records; typed disjoint aligned output and
/// state records for the selected operation. Output byte ranges are writable
/// and disjoint from source bytes. Pure borrowed views never escape a call,
/// except returned slices into the caller's original immutable byte record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_ntfs_format_run(op: u8, args: *const Args) -> Result {
    unsafe {
        let a = args.read();
        let s = a.bytes.get();
        match op {
            0 => Result::boolean(publish(a.output, Some(FileReference::parse(a.value)))),
            1 => Result::value(a.input.cast::<FileReference>().read().pack()),
            2 => {
                let mut out = BootSector::default();
                let code = r::boot(s, &mut out);
                if code == 0 {
                    a.output.cast::<BootSector>().write(out)
                }
                Result {
                    value: 0,
                    code: i32::from(code),
                    present: code == 0,
                }
            }
            3 | 4 => {
                let code = r::fixup(
                    a.buffer.get(),
                    if op == 4 {
                        Some(u16::try_from(a.value).expect("NTFS fixup USN"))
                    } else {
                        None
                    },
                );
                Result {
                    value: 0,
                    code: i32::from(code),
                    present: code == 0,
                }
            }
            5 => Result::boolean(publish(a.output, r::file_header(s))),
            6 => Result::boolean(publish(a.output, r::attribute(s, &mut (*a.state).offset))),
            7 => {
                let mut offset = usize::from((*a.input.cast::<FileRecordHeader>()).attrs_offset);
                loop {
                    let Some(attr) = r::attribute(s, &mut offset) else {
                        break Result::default();
                    };
                    if attr.attr_type == u32::try_from(a.value).expect("NTFS attribute type")
                        && attr.name.get() == a.second.get()
                    {
                        break Result::boolean(publish(a.output, Some(attr)));
                    }
                }
            }
            8 => Result::boolean(publish(a.output, runs::next(s, &mut *a.state))),
            9 => Result::optional(runs::encode(
                a.buffer.get(),
                a.value,
                if a.flag { Some(a.signed) } else { None },
            )),
            10 => Result::boolean(publish(a.output, r::standard(s))),
            11 => Result::boolean(publish(a.output, r::file_name(s))),
            12 => Result::boolean(publish(a.output, r::attr_list(s, &mut (*a.state).offset))),
            13 => Result::boolean(publish(a.output, r::index_header(s))),
            14 => Result::boolean(publish(a.output, r::index_root(s))),
            15 => Result::boolean(publish(a.output, r::index_block(s))),
            16 => Result::boolean(publish(a.output, r::index_entry(s, &mut *a.state))),
            17 => Result::value(u64::from(n::upcase(
                s,
                u16::try_from(a.value).expect("NTFS unit"),
            ))),
            18 => Result {
                value: 0,
                code: i32::from(n::compare(s, a.second.get(), a.third.get())),
                present: true,
            },
            19 => Result::boolean(n::ascii_matches(s, a.second.get(), a.third.get())),
            20 => Result::optional(n::to_ascii(s, a.buffer.get())),
            21 => Result::optional(n::from_ascii(s, a.buffer.get())),
            22 => Result::optional(n::to_utf16(s, a.buffer.get())),
            23 => Result::optional(n::to_utf8(s, a.buffer.get())),
            24 => Result::optional(l::decompress(s, a.buffer.get())),
            25 => Result::boolean(publish(a.output, r::volume_info(s))),
            _ => unreachable!(),
        }
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_ntfs_format_flag(value: u64, mask: u64) -> bool {
    value & mask != 0
}
