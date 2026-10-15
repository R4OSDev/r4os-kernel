//! Original resumable and recursive I30 enumeration.
use super::{
    ntfs_format_records as fmt, ntfs_format_types::*, ntfs_volume_index_io as idx,
    ntfs_volume_io as io, ntfs_volume_names as names, ntfs_volume_records as r,
    ntfs_volume_types::*,
};
#[inline(never)]
pub(super) unsafe fn next(
    v: Volume,
    dir: u64,
    c: *mut DirectoryCursor,
    out: *mut Entry,
) -> DirectoryNext {
    unsafe {
        if (*c).state > 2 || (*c).depth > 8 {
            return DirectoryNext::Io;
        }
        if (*c).state == 2 {
            return DirectoryNext::NotFound;
        }
        if (*c).state == 0 {
            c.write(DirectoryCursor {
                state: 1,
                depth: 1,
                ..DirectoryCursor::default()
            })
        }
        if (*c).depth == 0 {
            return DirectoryNext::Io;
        }
        let Some(h) = r::load(v, dir, v.record()) else {
            return DirectoryNext::Io;
        };
        let record = v.record();
        let Some(attr) = r::find(v, record, h, 0x90, View::of(&idx::I30)) else {
            return DirectoryNext::Io;
        };
        let Some(mut root) = fmt::index_root(attr.value.get()) else {
            return DirectoryNext::Io;
        };
        let mut alloc = AttrScratch::EMPTY;
        if root.header.flags & 1 != 0 {
            if r::collect(v, dir, 0xa0, View::of(&idx::I30), &mut alloc) != Lookup::Found {
                return DirectoryNext::Io;
            }
            let Some(h) = r::load(v, dir, v.record()) else {
                return DirectoryNext::Io;
            };
            let Some(attr) = r::find(v, record, h, 0x90, View::of(&idx::I30)) else {
                return DirectoryNext::Io;
            };
            let Some(reloaded) = fmt::index_root(attr.value.get()) else {
                return DirectoryNext::Io;
            };
            root = reloaded
        }
        for _ in 0..256 {
            let level = (*c).depth as usize - 1;
            let frame = core::ptr::addr_of_mut!((*c).frames)
                .cast::<DirectoryFrame>()
                .add(level);
            if (*frame).child_done > 1 {
                return DirectoryNext::Io;
            }
            let mut entries = root.entries;
            if level != 0 {
                if !idx::cached(v, dir, r::attr_runs(&alloc), (*frame).vcn) {
                    return DirectoryNext::Io;
                }
                let Some(b) = fmt::index_block(r::view(idx::block(v))) else {
                    return DirectoryNext::Io;
                };
                entries = b.entries
            }
            if (*frame).offset as usize >= entries.len {
                return DirectoryNext::Io;
            }
            let mut it = IteratorState::default();
            let bytes = entries.get();
            let Some(entry) = fmt::index_entry(&bytes[(*frame).offset as usize..], &mut it) else {
                return DirectoryNext::Io;
            };
            if v.io.work_counters {
                (*v.scratch()).work.directory_entries += 1
            }
            if entry.flags & 1 != 0 && (*frame).child_done == 0 {
                if (*c).depth >= 8 || !entry.has_sub_node_vcn {
                    return DirectoryNext::Io;
                }
                let child = entry.sub_node_vcn;
                for i in 1..(*c).depth as usize {
                    if (*c).frames[i].vcn == child {
                        return DirectoryNext::Io;
                    }
                }
                (*frame).child_done = 1;
                (*c).frames[(*c).depth as usize] = DirectoryFrame {
                    vcn: child,
                    ..DirectoryFrame::default()
                };
                (*c).depth += 1;
                continue;
            }
            if entry.flags & 2 != 0 {
                (*c).depth -= 1;
                if (*c).depth == 0 {
                    (*c).state = 2;
                    return DirectoryNext::NotFound;
                }
                continue;
            }
            (*frame).offset += u32::from(entry.entry_length);
            (*frame).child_done = 0;
            let Some(name) = fmt::file_name(entry.key.get()) else {
                return DirectoryNext::Io;
            };
            if name.namespace == 2 {
                continue;
            }
            let reference = FileReference::parse(entry.file_reference);
            if dir == 5 && reference.record < 16 {
                continue;
            }
            let Some(built) = names::entry_from_name(reference, name) else {
                return DirectoryNext::Io;
            };
            out.write(built);
            return DirectoryNext::Found;
        }
        DirectoryNext::Again
    }
}
pub struct Sink {
    pub out: Option<Buffer>,
    pub cursor: usize,
    pub listed: usize,
    pub max_entries: usize,
    pub wanted: usize,
    pub seen: usize,
    pub found: Option<Entry>,
    pub failed: bool,
    pub count_ref: Option<u64>,
    pub matches: usize,
}
impl Sink {
    pub const EMPTY: Self = Self {
        out: None,
        cursor: 0,
        listed: 0,
        max_entries: 0,
        wanted: 0,
        seen: 0,
        found: None,
        failed: false,
        count_ref: None,
        matches: 0,
    };
}
pub(super) unsafe fn append(out: Buffer, cursor: &mut usize, bytes: &[u8]) -> bool {
    if *cursor + bytes.len() >= out.len {
        return false;
    }
    unsafe { core::ptr::copy_nonoverlapping(bytes.as_ptr(), out.data.add(*cursor), bytes.len()) }
    *cursor += bytes.len();
    true
}
unsafe fn emit(sink: *mut Sink, entry: Entry) {
    unsafe {
        if let Some(wanted) = (*sink).count_ref {
            if (FileReference {
                record: entry.record,
                sequence: entry.sequence,
            })
            .pack()
                == wanted
            {
                (*sink).matches += 1
            }
            return;
        }
        if let Some(out) = (*sink).out {
            let prefix: &[u8] = if entry.is_dir() { b"<DIR> " } else { b"      " };
            if !append(out, &mut (*sink).cursor, prefix)
                || !append(out, &mut (*sink).cursor, &entry.name[..entry.name_len])
                || !append(out, &mut (*sink).cursor, b"\r\n")
            {
                (*sink).failed = true;
                return;
            }
            (*sink).listed += 1;
            return;
        }
        if (*sink).seen == (*sink).wanted {
            (*sink).found = Some(entry);
            return;
        }
        (*sink).seen += 1
    }
}
#[inline(never)]
#[allow(clippy::too_many_arguments)]
unsafe fn walk(
    v: Volume,
    dir: u64,
    runs: io::Runs,
    sink: *mut Sink,
    node: Option<u64>,
    root: View,
    depth: usize,
    hide: bool,
) {
    unsafe {
        if (*sink).failed || (*sink).found.is_some() {
            return;
        }
        if depth >= 8 {
            (*sink).failed = true;
            return;
        }
        let mut offset = 0;
        loop {
            let entries = if let Some(vcn) = node {
                if !idx::cached(v, dir, runs, vcn) {
                    (*sink).failed = true;
                    return;
                }
                let Some(b) = fmt::index_block(r::view(idx::block(v))) else {
                    (*sink).failed = true;
                    return;
                };
                b.entries
            } else {
                root
            };
            if offset >= entries.len {
                return;
            }
            let mut it = IteratorState::default();
            let Some(entry) = fmt::index_entry(&entries.get()[offset..], &mut it) else {
                return;
            };
            offset += usize::from(entry.entry_length);
            let mut pending = None;
            if entry.flags & 2 == 0 {
                let Some(name) = fmt::file_name(entry.key.get()) else {
                    (*sink).failed = true;
                    return;
                };
                if name.namespace != 2 {
                    let reference = FileReference::parse(entry.file_reference);
                    if !(hide && reference.record < 16) {
                        pending = names::entry_from_name(reference, name)
                    }
                }
            }
            let end = entry.flags & 2 != 0;
            if entry.flags & 1 != 0 {
                assert!(entry.has_sub_node_vcn);
                walk(
                    v,
                    dir,
                    runs,
                    sink,
                    Some(entry.sub_node_vcn),
                    root,
                    depth + 1,
                    hide,
                );
                if (*sink).failed || (*sink).found.is_some() {
                    return;
                }
            }
            if end {
                return;
            }
            let Some(built) = pending else { continue };
            emit(sink, built);
            if (*sink).failed || (*sink).found.is_some() {
                return;
            }
            if (*sink).out.is_some() && (*sink).listed >= (*sink).max_entries {
                return;
            }
        }
    }
}
#[inline(never)]
pub(super) unsafe fn enumerate(v: Volume, dir: u64, sink: *mut Sink) -> bool {
    unsafe {
        let Some(h) = r::load(v, dir, v.record()) else {
            return false;
        };
        let record = v.record();
        let Some(attr) = r::find(v, record, h, 0x90, View::of(&idx::I30)) else {
            return false;
        };
        let Some(root) = fmt::index_root(attr.value.get()) else {
            return false;
        };
        let hide = dir == 5;
        if root.header.flags & 1 != 0 {
            let mut alloc = AttrScratch::EMPTY;
            if r::collect(v, dir, 0xa0, View::of(&idx::I30), &mut alloc) != Lookup::Found {
                return false;
            }
            let Some(h) = r::load(v, dir, v.record()) else {
                return false;
            };
            let Some(attr) = r::find(v, record, h, 0x90, View::of(&idx::I30)) else {
                return false;
            };
            let Some(root) = fmt::index_root(attr.value.get()) else {
                return false;
            };
            walk(
                v,
                dir,
                r::attr_runs(&alloc),
                sink,
                None,
                root.entries,
                0,
                hide,
            )
        } else {
            walk(
                v,
                dir,
                io::Runs {
                    data: core::ptr::null(),
                    len: 0,
                },
                sink,
                None,
                root.entries,
                0,
                hide,
            )
        }
        !(*sink).failed
    }
}
