//! Original bootstrap drive table and bounded DOS working directories.
//! The original VFS/bootstrap callers retain serialization and mount lifetime.
use core::{
    cell::UnsafeCell,
    mem::{offset_of, size_of},
    ptr,
};
pub const DRIVE_COUNT: usize = 26;
pub const MAX_PATH: usize = 1024;
#[derive(Clone, Copy)]
#[repr(C)]
pub struct Label {
    pub data: *const u8,
    pub len: usize,
}
#[repr(C)]
pub struct Drive {
    pub mounted: bool,
    pub letter: u8,
    pub kind: u8,
    pub role: u8,
    pub name: Label,
    pub bytes: usize,
    pub block_index: usize,
    pub has_block: bool,
    pub cwd: [u8; MAX_PATH],
    pub cwd_len: usize,
}
impl Drive {
    const EMPTY: Self = Self {
        mounted: false,
        letter: 0,
        kind: 0,
        role: 0,
        name: Label {
            data: b"".as_ptr(),
            len: 0,
        },
        bytes: 0,
        block_index: 0,
        has_block: false,
        cwd: [0; MAX_PATH],
        cwd_len: 0,
    };
}
struct State {
    drives: [Drive; DRIVE_COUNT],
    current: usize,
    initialized: bool,
}
struct Canonical(UnsafeCell<State>);
// SAFETY: The same original BSP/bootstrap and VFS callers serialize this
// table. Published pointers have the original stable slot lifetime. Rust
// creates no reference spanning a C caller or display callback.
unsafe impl Sync for Canonical {}
static STATE: Canonical = Canonical(UnsafeCell::new(State {
    drives: [const { Drive::EMPTY }; DRIVE_COUNT],
    current: 2,
    initialized: false,
}));
unsafe extern "C" {
    fn r4os_zig_fs_drive_text(data: *const u8, len: usize);
    fn r4os_zig_fs_drive_decimal(value: usize);
}
fn upper(letter: u8) -> u8 {
    letter.to_ascii_uppercase()
}
fn letter_index(letter: u8) -> Option<usize> {
    let value = upper(letter);
    if value.is_ascii_uppercase() {
        Some(usize::from(value - b'A'))
    } else {
        None
    }
}
fn label(text: &'static [u8]) -> Label {
    Label {
        data: text.as_ptr(),
        len: text.len(),
    }
}
fn kind_name(kind: u8) -> Label {
    label(match kind {
        0 => b"NONE",
        1 => b"RAM",
        2 => b"FAT32",
        3 => b"NTFS",
        _ => unreachable!(),
    })
}
fn role_name(role: u8) -> Label {
    label(match role {
        0 => b"general",
        1 => b"system",
        2 => b"data",
        3 => b"ram",
        _ => unreachable!(),
    })
}
unsafe fn reset(state: *mut State) {
    let drives = unsafe { ptr::addr_of_mut!((*state).drives).cast::<Drive>() };
    for index in 0..DRIVE_COUNT {
        unsafe { drives.add(index).write(Drive::EMPTY) };
    }
    unsafe {
        (*state).current = 2;
        (*state).initialized = true;
    }
}
unsafe fn drive(state: *mut State, index: usize) -> *mut Drive {
    unsafe {
        ptr::addr_of_mut!((*state).drives)
            .cast::<Drive>()
            .add(index)
    }
}

/// # Safety
/// Original bootstrap/VFS table boundary. No reference or owner crosses the
/// call. Unmount is admitted only after the VFS owner has drained exact I/O.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_fs_drive_control(op: u8, letter: u8) -> usize {
    let state = STATE.0.get();
    match op {
        0 => {
            unsafe { reset(state) };
            0
        }
        1 => {
            let Some(index) = letter_index(letter) else {
                return 0;
            };
            if !unsafe { (*drive(state, index)).mounted } {
                return 0;
            }
            unsafe { (*state).current = index };
            1
        }
        2 => {
            if !letter.is_ascii_uppercase() {
                return 0;
            }
            let index = usize::from(letter - b'A');
            unsafe {
                drive(state, index).write(Drive::EMPTY);
                if (*state).current == index {
                    (*state).current = 2;
                }
            }
            0
        }
        3 => {
            if !unsafe { (*state).initialized } {
                return 0;
            }
            let current = unsafe { drive(state, (*state).current) };
            if unsafe { (*current).mounted } {
                usize::from(unsafe { (*current).letter })
            } else {
                0
            }
        }
        _ => unreachable!(),
    }
}

/// # Safety
/// Same table serialization and stable-slot lifetime as the original get,
/// current and atIndex. Caller must not use a pointer after slot replacement.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_fs_drive_lookup(op: u8, value: usize) -> *mut Drive {
    let state = STATE.0.get();
    if !unsafe { (*state).initialized } {
        return ptr::null_mut();
    }
    let index = match op {
        0 => unsafe { (*state).current },
        1 => {
            let Some(index) = letter_index(value as u8) else {
                return ptr::null_mut();
            };
            index
        }
        2 => {
            if value >= DRIVE_COUNT {
                return ptr::null_mut();
            }
            value
        }
        _ => unreachable!(),
    };
    let result = unsafe { drive(state, index) };
    if unsafe { (*result).mounted } {
        result
    } else {
        ptr::null_mut()
    }
}

/// # Safety
/// Mount is serialized by the original caller. The borrowed name bytes remain
/// valid for this drive's entire mounted lifetime, exactly as the Zig slice.
/// Block presence is explicit and does not reserve the device or mount.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_fs_drive_mount(
    letter: u8,
    kind: u8,
    role: u8,
    derive_role: bool,
    name: *const u8,
    name_len: usize,
    bytes: usize,
    block: usize,
    has_block: bool,
) -> bool {
    let state = STATE.0.get();
    if !unsafe { (*state).initialized } {
        unsafe { reset(state) };
    }
    let Some(index) = letter_index(letter) else {
        return false;
    };
    let letter = upper(letter);
    let role = if derive_role {
        if letter == b'C' && kind == 2 {
            1
        } else if letter == b'D' && kind == 2 {
            2
        } else {
            0
        }
    } else {
        role
    };
    let target = unsafe { drive(state, index) };
    unsafe {
        target.write(Drive::EMPTY);
        (*target).mounted = true;
        (*target).letter = letter;
        (*target).kind = kind;
        (*target).role = role;
        (*target).name = Label {
            data: name,
            len: name_len,
        };
        (*target).bytes = bytes;
        (*target).block_index = block;
        (*target).has_block = has_block;
        (*target).cwd_len = 1;
        (*target).cwd[0] = b'\\';
        if !(*drive(state, (*state).current)).mounted {
            (*state).current = index;
        }
    }
    true
}

/// # Safety
/// Resident drive and caller bytes have the original lifetime/serialization.
/// Raw byte access deliberately preserves the original forward-copy behavior
/// even when the caller's path/segment aliases this drive's own CWD storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_fs_drive_cwd(
    state: *mut Drive,
    op: u8,
    data: *const u8,
    len: usize,
) -> bool {
    let cwd = unsafe { ptr::addr_of_mut!((*state).cwd).cast::<u8>() };
    match op {
        0 => unsafe {
            ptr::write_bytes(cwd, 0, MAX_PATH);
            cwd.write(b'\\');
            (*state).cwd_len = 1;
        },
        1 => {
            if len == 0 || unsafe { data.read() } != b'\\' || len >= MAX_PATH {
                return false;
            }
            unsafe { ptr::write_bytes(cwd, 0, MAX_PATH) };
            for index in 0..len {
                unsafe { cwd.add(index).write(upper(data.add(index).read())) };
            }
            unsafe { (*state).cwd_len = len };
        }
        2 => {
            if len == 0 {
                return true;
            }
            let mut end = unsafe { (*state).cwd_len };
            let mut needed = end + len;
            if end > 1 {
                needed += 1;
            }
            if needed >= MAX_PATH {
                return false;
            }
            if end > 1 {
                unsafe { cwd.add(end).write(b'\\') };
                end += 1;
                unsafe { (*state).cwd_len = end };
            }
            for index in 0..len {
                unsafe { cwd.add(end).write(upper(data.add(index).read())) };
                end += 1;
                unsafe { (*state).cwd_len = end };
            }
            unsafe { cwd.add(end).write(0) };
        }
        3 => {
            let mut end = unsafe { (*state).cwd_len };
            if end <= 1 {
                return true;
            }
            assert!(end <= MAX_PATH);
            while end > 1 && unsafe { cwd.add(end - 1).read() } != b'\\' {
                end -= 1;
            }
            end = if end <= 1 { 1 } else { end - 1 };
            unsafe {
                (*state).cwd_len = end;
                cwd.add(end).write(0);
            }
        }
        _ => unreachable!(),
    }
    true
}
fn text(value: Label) {
    unsafe { r4os_zig_fs_drive_text(value.data, value.len) };
}
fn puts(value: &'static [u8]) {
    text(label(value));
}
fn character(value: u8) {
    unsafe { r4os_zig_fs_drive_text(&value, 1) };
}
fn decimal(value: usize) {
    unsafe { r4os_zig_fs_drive_decimal(value) };
}

/// # Safety
/// Original synchronous bootstrap/diagnostic display context. The drive table
/// and borrowed labels remain valid through printing. No mutable native table
/// borrow spans the existing display/log leaf callbacks.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_fs_drive_print(op: u8, block_index: usize) {
    let state = STATE.0.get();
    match op {
        0 => {
            puts(b"  Drives:\r\n");
            for index in 0..DRIVE_COUNT {
                let d = unsafe { drive(state, index) };
                if !unsafe { (*d).mounted } {
                    continue;
                }
                puts(b"    ");
                character(unsafe { (*d).letter });
                puts(b": ");
                text(kind_name(unsafe { (*d).kind }));
                puts(b" ");
                text(role_name(unsafe { (*d).role }));
                puts(b" ");
                text(unsafe { ptr::addr_of!((*d).name).read() });
                puts(b" ");
                decimal(unsafe { (*d).bytes } / 1024);
                puts(b" KB");
                if unsafe { (*d).has_block } {
                    puts(b" block=#");
                    decimal(unsafe { (*d).block_index });
                }
                puts(b"\r\n");
            }
        }
        1 => {
            let mut printed = false;
            for index in 0..DRIVE_COUNT {
                let d = unsafe { drive(state, index) };
                if !unsafe { (*d).mounted }
                    || !unsafe { (*d).has_block }
                    || unsafe { (*d).block_index } != block_index
                {
                    continue;
                }
                if printed {
                    puts(b",");
                }
                character(unsafe { (*d).letter });
                puts(b":");
                printed = true;
            }
            if !printed {
                puts(b"none");
            }
        }
        2 => {
            let d = unsafe { r4os_rust_fs_drive_lookup(0, 0) };
            if d.is_null() {
                puts(b"?:\\>");
            } else {
                character(unsafe { (*d).letter });
                puts(b":");
                assert!(unsafe { (*d).cwd_len } <= MAX_PATH);
                text(Label {
                    data: unsafe { ptr::addr_of!((*d).cwd).cast() },
                    len: unsafe { (*d).cwd_len },
                });
                puts(b">");
            }
        }
        _ => unreachable!(),
    }
}
/// # Safety
/// Output is an exclusive aligned Label. Returned storage is immutable static
/// text; no second table or locale policy exists in the Zig facade.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_fs_drive_label(role: bool, value: u8, output: *mut Label) {
    unsafe {
        output.write(if role {
            role_name(value)
        } else {
            kind_name(value)
        })
    };
}
const _: () = {
    assert!(size_of::<Label>() == 16);
    assert!(offset_of!(Drive, name) == 8);
    assert!(offset_of!(Drive, has_block) == 40);
    assert!(offset_of!(Drive, cwd) == 41);
    assert!(offset_of!(Drive, cwd_len) == 1072);
    assert!(size_of::<Drive>() == 1080);
};
