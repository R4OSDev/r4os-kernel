//! Original bounded protocol status registry. A single runtime owner protects
//! mutation and copied snapshots; no pointer into mutable slot bytes escapes.
use crate::arch::runtime;
use core::{cell::UnsafeCell, mem, slice};
pub const MAX_PROTOCOLS: usize = 64;
pub(crate) const MAX_ROLE: usize = 64;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Entry {
    pub used: bool,
    pub source: u8,
    pub state: u8,
    pub category: u16,
    pub version: u16,
    pub api_version: u32,
    pub last_error: i32,
    pub name: [u8; 32],
    pub name_len: usize,
    pub role: [u8; 64],
    pub role_len: usize,
    pub note: [u8; 96],
    pub note_len: usize,
}
impl Entry {
    const EMPTY: Self = Self {
        used: false,
        source: 0,
        state: 0,
        category: 0,
        version: 0,
        api_version: 0,
        last_error: 0,
        name: [0; 32],
        name_len: 0,
        role: [0; 64],
        role_len: 0,
        note: [0; 96],
        note_len: 0,
    };
}
#[repr(C)]
pub struct Registration {
    pub name: *const u8,
    pub name_len: usize,
    pub role: *const u8,
    pub role_len: usize,
    pub category: u16,
    pub version: u16,
    pub api_version: u32,
}
struct Registry(UnsafeCell<[Entry; MAX_PROTOCOLS]>);
unsafe impl Sync for Registry {}
static ENTRIES: Registry = Registry(UnsafeCell::new([Entry::EMPTY; MAX_PROTOCOLS]));
const _: () = {
    assert!(mem::size_of::<Entry>() == 232);
    assert!(mem::offset_of!(Entry, name_len) == 48);
    assert!(mem::offset_of!(Entry, role_len) == 120);
    assert!(mem::offset_of!(Entry, note_len) == 224);
    assert!(mem::size_of::<Registration>() == 40);
};
fn row(slot: usize) -> *mut Entry {
    ENTRIES.0.get().cast::<Entry>().wrapping_add(slot)
}
unsafe fn bytes<'a>(pointer: *const u8, len: usize) -> &'a [u8] {
    if len == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(pointer, len) }
    }
}
unsafe fn find(role: &[u8], mode: u8) -> Option<usize> {
    assert!(matches!(mode, 1 | 2), "Invalid protocol lookup mode");
    for slot in 0..MAX_PROTOCOLS {
        let entry = unsafe { row(slot).read() };
        if entry.used
            && matches!(entry.source, 1 | 2)
            && entry.role[..entry.role_len].eq_ignore_ascii_case(role)
            && (mode == 1 || entry.state == 2)
        {
            return Some(slot);
        }
    }
    None
}
fn copy(source: &[u8], output: &mut [u8]) -> usize {
    output.fill(0);
    let len = source.len().min(output.len() - 1);
    output[..len].copy_from_slice(&source[..len]);
    len
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_protocol_registry_reset() {
    let flags = runtime::save_runtime();
    for slot in 0..MAX_PROTOCOLS {
        unsafe { row(slot).write(Entry::EMPTY) };
    }
    runtime::restore_runtime(flags);
}

/// # Safety
/// Registration and input bytes are readable exact C storage for this bounded
/// call. Loader/ProtocolApi retain their existing module execution admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_protocol_registry_register(
    operation: u8,
    input: *const Registration,
) -> usize {
    assert!(matches!(operation, 1..=3), "Invalid protocol registration");
    let flags = runtime::save_runtime();
    let result = unsafe {
        let r = &*input;
        let role = bytes(r.role, r.role_len);
        if find(role, 1).is_some() {
            usize::MAX
        } else {
            let mut slot = None;
            for index in 0..MAX_PROTOCOLS {
                if !(*row(index)).used {
                    slot = Some(index);
                    break;
                }
            }
            if let Some(slot) = slot {
                let mut entry = Entry {
                    used: true,
                    source: match operation {
                        1 | 2 => 1,
                        3 => 2,
                        _ => panic!("Invalid protocol registration"),
                    },
                    state: 1,
                    category: r.category,
                    version: r.version,
                    api_version: r.api_version,
                    ..Entry::EMPTY
                };
                entry.name_len = copy(bytes(r.name, r.name_len), &mut entry.name);
                entry.role_len = copy(role, &mut entry.role);
                entry.note_len = copy(
                    match operation {
                        2 => b"installed; lazy",
                        _ => b"loaded",
                    },
                    &mut entry.note,
                );
                row(slot).write(entry);
                slot
            } else {
                usize::MAX
            }
        }
    };
    runtime::restore_runtime(flags);
    result
}

/// # Safety
/// Exact readable registration storage and protected module execution context.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_protocol_registry_select(
    slot: usize,
    input: *const Registration,
) -> bool {
    if slot >= MAX_PROTOCOLS {
        return false;
    }
    let flags = runtime::save_runtime();
    let result = unsafe {
        let r = &*input;
        let entry = row(slot);
        if !(*entry).used
            || (*entry).source != 1
            || !(&(*entry).role)[..(*entry).role_len]
                .eq_ignore_ascii_case(bytes(r.role, r.role_len))
        {
            false
        } else {
            (*entry).name_len = copy(bytes(r.name, r.name_len), &mut (*entry).name);
            (*entry).category = r.category;
            (*entry).version = r.version;
            (*entry).api_version = r.api_version;
            (*entry).last_error = 0;
            (*entry).state = 1;
            (*entry).note_len = copy(b"loading on demand", &mut (*entry).note);
            true
        }
    };
    runtime::restore_runtime(flags);
    result
}

/// # Safety
/// Note bytes remain readable throughout the bounded mutation. No callback,
/// wait or foreign reference is retained while the runtime owner is held.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_protocol_registry_mutate(
    slot: usize,
    operation: u8,
    value: u32,
    error: i32,
    note: *const u8,
    len: usize,
) {
    if slot >= MAX_PROTOCOLS {
        return;
    }
    let flags = runtime::save_runtime();
    unsafe {
        let entry = row(slot);
        if (*entry).used {
            match operation {
                0 => {
                    assert!(value < 7, "Invalid protocol state");
                    (*entry).state = value as u8;
                }
                1 | 2 => {
                    (*entry).state = if operation == 1 { 5 } else { 4 };
                    (*entry).last_error = error;
                }
                3 => {}
                4 => {
                    (*entry).state = match value {
                        1..=6 => value as u8,
                        _ => 1,
                    }
                }
                _ => panic!("Invalid protocol mutation"),
            }
            if operation != 0 {
                (*entry).note_len = copy(bytes(note, len), &mut (*entry).note);
            }
        }
    }
    runtime::restore_runtime(flags);
}

/// # Safety
/// Exclusive aligned output and exact readable role bytes. The copied bytes
/// belong to the caller after the original runtime admission is released.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_protocol_registry_entry(
    role: *const u8,
    len: usize,
    mode: u8,
    out: *mut Entry,
) -> bool {
    let flags = runtime::save_runtime();
    let selected = unsafe { find(bytes(role, len), mode) };
    if let Some(slot) = selected {
        unsafe { out.write(row(slot).read()) };
    }
    runtime::restore_runtime(flags);
    selected.is_some()
}
/// # Safety
/// Exclusive aligned C output; no mutable registry pointer escapes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_protocol_registry_get(slot: usize, out: *mut Entry) -> bool {
    if slot >= MAX_PROTOCOLS {
        return false;
    }
    let flags = runtime::save_runtime();
    let entry = unsafe { row(slot).read() };
    if entry.used {
        unsafe { out.write(entry) };
    }
    runtime::restore_runtime(flags);
    entry.used
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_protocol_registry_count() -> usize {
    let flags = runtime::save_runtime();
    let mut count = 0;
    for slot in 0..MAX_PROTOCOLS {
        if unsafe { (*row(slot)).used } {
            count += 1;
        }
    }
    runtime::restore_runtime(flags);
    count
}

/// # Safety
/// Readable role bytes through this synchronous owner-protected query.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_protocol_registry_query(
    operation: u8,
    slot: usize,
    role: *const u8,
    len: usize,
) -> i32 {
    let flags = runtime::save_runtime();
    let result = unsafe {
        let role = bytes(role, len);
        match operation {
            0 => {
                if let Some(slot) = find(role, 2) {
                    i32::from((*row(slot)).state == 2)
                } else {
                    0
                }
            }
            1 => {
                if let Some(slot) = find(role, 1) {
                    i32::from((*row(slot)).state == 2)
                } else {
                    0
                }
            }
            2 => {
                if let Some(slot) = find(role, 2) {
                    match (*row(slot)).state {
                        2 => 1,
                        1 => 0,
                        4 => -5,
                        5 => -1,
                        6 => -6,
                        _ => 0,
                    }
                } else {
                    -5
                }
            }
            3 => i32::from(
                slot < MAX_PROTOCOLS
                    && (*row(slot)).used
                    && (&(*row(slot)).role)[..(*row(slot)).role_len].eq_ignore_ascii_case(role),
            ),
            _ => panic!("Invalid protocol query"),
        }
    };
    runtime::restore_runtime(flags);
    result
}

/// Installed/preloaded R4P state, including failed or disabled registrations.
pub(crate) fn diagnostic_entry(role: &[u8]) -> Option<Entry> {
    let _owner = runtime::RuntimeGuard::acquire();
    unsafe { find(role, 1).map(|slot| row(slot).read()) }
}

pub(crate) fn source_name(source: u8) -> &'static [u8] {
    match source {
        0 => b"builtin",
        1 => b"r4p",
        2 => b"preload",
        _ => unreachable!(),
    }
}
pub(crate) fn category_name(category: u16) -> &'static [u8] {
    match category {
        1 => b"net",
        2 => b"usb",
        3 => b"audio",
        4 => b"data",
        255 => b"misc",
        _ => b"unknown",
    }
}
pub(crate) fn state_name(state: u8) -> &'static [u8] {
    match state {
        0 => b"empty",
        1 => b"loaded",
        2 => b"active",
        3 => b"fallback",
        4 => b"blocked",
        5 => b"error",
        6 => b"disabled",
        _ => unreachable!(),
    }
}
pub(crate) fn get(slot: usize) -> Option<Entry> {
    let mut out = mem::MaybeUninit::uninit();
    unsafe { r4os_rust_protocol_registry_get(slot, out.as_mut_ptr()).then(|| out.assume_init()) }
}
pub(crate) fn dump_status() {
    use crate::boot::log_route::{dec, puts};
    puts(b"Protocol registry\r\n");
    let mut shown = 0;
    for slot in 0..MAX_PROTOCOLS {
        let Some(e) = get(slot) else { continue };
        shown += 1;
        puts(b"  ");
        puts(&e.role[..e.role_len]);
        puts(b" name=");
        puts(&e.name[..e.name_len]);
        puts(b" src=");
        puts(source_name(e.source));
        puts(b" cat=");
        puts(category_name(e.category));
        puts(b" state=");
        puts(state_name(e.state));
        if e.last_error != 0 {
            puts(b" err=");
            if e.last_error < 0 {
                puts(b"-");
                dec(i64::from(e.last_error).unsigned_abs());
            } else {
                dec(e.last_error as u64);
            }
        }
        if e.note_len > 0 {
            puts(b" note=");
            puts(&e.note[..e.note_len]);
        }
        puts(b"\r\n");
    }
    if shown == 0 {
        puts(b"  none\r\n");
    }
}
