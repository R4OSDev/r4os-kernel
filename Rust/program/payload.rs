//! Payload allocation, exact headers and original storage accounting. The same
//! program-state owner guards counters; failed frees retain their quarantine
//! accounting and exact allocation sizes. No large default lives on a task stack.
use super::{info, payload_types as t, registry, registry_types::ProgramInstance};
use crate::memory::{heap, owner::Guard};
use core::{cell::UnsafeCell, ffi::c_void, mem, ptr};
const MAGIC: u32 = 0x5034_3452;
#[repr(C)]
pub struct State {
    pub stats: t::Stats,
    pub report: t::SelfTestReport,
    pub active_published_bytes: u64,
    pub self_test_peak_bytes: u64,
    pub failure_after_value: u32,
    pub failure_cursor: u32,
    pub release_trace: [u16; 8],
    pub release_trace_len: usize,
    pub failure_after_present: bool,
    pub self_test_active: bool,
    pub self_test_running: bool,
    pub release_trace_enabled: bool,
}
struct Storage(UnsafeCell<State>);
// SAFETY: normal accounting retains program-state admission. Diagnostic flags
// retain the original single-test/boot ownership and have no second copy.
unsafe impl Sync for Storage {}
static STATE: Storage = Storage(UnsafeCell::new(State {
    stats: t::EMPTY_STATS,
    report: t::EMPTY_REPORT,
    active_published_bytes: 0,
    self_test_peak_bytes: 0,
    failure_after_value: 0,
    failure_cursor: 0,
    release_trace: [1; 8],
    release_trace_len: 0,
    failure_after_present: false,
    self_test_active: false,
    self_test_running: false,
    release_trace_enabled: false,
}));
fn state() -> *mut State {
    STATE.0.get()
}
fn stats() -> *mut t::Stats {
    unsafe { ptr::addr_of_mut!((*state()).stats) }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_payload_state() -> *mut State {
    state()
}
pub(super) fn add(offset: usize, width: usize, amount: u64) {
    let _guard = Guard::acquire(0);
    assert!(offset + width <= mem::size_of::<t::Stats>());
    unsafe {
        let field = stats().cast::<u8>().add(offset);
        match width {
            4 => {
                assert!(offset.is_multiple_of(4));
                let amount = u32::try_from(amount).expect("Storage stat increment");
                let field = field.cast::<u32>();
                field.write(field.read().wrapping_add(amount));
            }
            8 => {
                assert!(offset.is_multiple_of(8));
                let field = field.cast::<u64>();
                field.write(field.read().wrapping_add(amount));
            }
            _ => unreachable!("Invalid private storage field width"),
        }
    }
}
macro_rules! add64 {
    ($field:ident,$value:expr) => {
        add(mem::offset_of!(t::Stats, $field), 8, $value)
    };
}
macro_rules! add32 {
    ($field:ident,$value:expr) => {
        add(mem::offset_of!(t::Stats, $field), 4, $value)
    };
}
fn frame_kind(kind: u16) -> bool {
    matches!(kind, 7..=10)
}
unsafe fn count(kind: u16) -> *mut u32 {
    unsafe {
        let s = stats();
        match kind {
            1 => ptr::addr_of_mut!((*s).active_runtime_payloads),
            2 => ptr::addr_of_mut!((*s).active_process_payloads),
            3 => ptr::addr_of_mut!((*s).active_environment_payloads),
            4 => ptr::addr_of_mut!((*s).active_console_payloads),
            5 | 11 => ptr::addr_of_mut!((*s).active_console_output_payloads),
            6 => ptr::addr_of_mut!((*s).active_gui_payloads),
            7 => ptr::addr_of_mut!((*s).active_gui_command_payloads),
            8 => ptr::addr_of_mut!((*s).active_gui_raster_payloads),
            9 => ptr::addr_of_mut!((*s).active_gui_frame_payloads),
            10 => ptr::addr_of_mut!((*s).active_gui_data_payloads),
            _ => unreachable!(),
        }
    }
}
unsafe fn category(kind: u16) -> (*mut u64, *mut u64) {
    unsafe {
        let s = stats();
        match kind {
            1..=3 => (
                ptr::addr_of_mut!((*s).current_runtime_bytes),
                ptr::addr_of_mut!((*s).peak_runtime_bytes),
            ),
            4 | 5 | 11 => (
                ptr::addr_of_mut!((*s).current_console_bytes),
                ptr::addr_of_mut!((*s).peak_console_bytes),
            ),
            6..=10 => (
                ptr::addr_of_mut!((*s).current_gui_bytes),
                ptr::addr_of_mut!((*s).peak_gui_bytes),
            ),
            _ => unreachable!(),
        }
    }
}
pub(super) fn refresh() {
    let _guard = Guard::acquire(0);
    unsafe {
        let s = stats();
        (*s).live_core_bytes =
            u64::from((*s).active_instances) * mem::size_of::<ProgramInstance>() as u64;
        (*s).active_instance_bytes = (*s).live_core_bytes + (*state()).active_published_bytes;
        (*s).reserved_instance_bytes =
            (*s).registry_reserved_core_bytes + (*s).current_payload_bytes;
        if (*s).active_instance_bytes > (*s).peak_active_instance_bytes {
            (*s).peak_active_instance_bytes = (*s).active_instance_bytes;
        }
        if (*s).reserved_instance_bytes > (*s).peak_reserved_instance_bytes {
            (*s).peak_reserved_instance_bytes = (*s).reserved_instance_bytes;
        }
    }
}
fn note_allocation(kind: u16, bytes: usize) {
    let _guard = Guard::acquire(0);
    unsafe {
        let c = count(kind);
        c.write(c.read().wrapping_add(1));
        let (current, peak) = category(kind);
        current.write(current.read().wrapping_add(bytes as u64));
        if current.read() > peak.read() {
            peak.write(current.read());
        }
        add64!(current_payload_bytes, bytes as u64);
        let s = stats();
        if frame_kind(kind) {
            add64!(current_gui_frame_bytes, bytes as u64);
            add64!(current_gui_frame_nodes, 1);
            if (*s).current_gui_frame_bytes > (*s).peak_gui_frame_bytes {
                (*s).peak_gui_frame_bytes = (*s).current_gui_frame_bytes;
            }
            if (*s).current_gui_frame_nodes > (*s).peak_gui_frame_nodes {
                (*s).peak_gui_frame_nodes = (*s).current_gui_frame_nodes;
            }
        }
        if (*s).current_payload_bytes > (*s).peak_payload_bytes {
            (*s).peak_payload_bytes = (*s).current_payload_bytes;
        }
        if (*state()).self_test_active
            && (*s).current_payload_bytes > (*state()).self_test_peak_bytes
        {
            (*state()).self_test_peak_bytes = (*s).current_payload_bytes;
        }
        refresh();
    }
}
pub(super) fn note_release(kind: u16, bytes: usize) {
    let _guard = Guard::acquire(0);
    unsafe {
        let c = count(kind);
        if c.read() > 0 {
            c.write(c.read() - 1);
        } else {
            add64!(header_errors, 1);
        }
        let (current, _) = category(kind);
        let bytes = bytes as u64;
        if current.read() >= bytes {
            current.write(current.read() - bytes);
        } else {
            add64!(header_errors, 1);
        }
        let s = stats();
        if (*s).current_payload_bytes >= bytes {
            (*s).current_payload_bytes -= bytes;
        } else {
            add64!(header_errors, 1);
        }
        if frame_kind(kind) {
            if (*s).current_gui_frame_bytes >= bytes {
                (*s).current_gui_frame_bytes -= bytes;
            } else {
                (*s).current_gui_frame_bytes = 0;
                add64!(header_errors, 1);
            }
            if (*s).current_gui_frame_nodes > 0 {
                (*s).current_gui_frame_nodes -= 1;
            } else {
                add64!(header_errors, 1);
            }
        }
        refresh();
    }
}
pub(super) fn note_active(bytes: usize, release: bool) {
    let _guard = Guard::acquire(0);
    unsafe {
        if !release {
            (*state()).active_published_bytes =
                (*state()).active_published_bytes.wrapping_add(bytes as u64);
        } else if (*state()).active_published_bytes >= bytes as u64 {
            (*state()).active_published_bytes -= bytes as u64;
        } else {
            (*state()).active_published_bytes = 0;
            add64!(header_errors, 1);
        }
        refresh();
    }
}
fn note_commands(count: u64, release: bool) {
    let _guard = Guard::acquire(0);
    unsafe {
        let s = stats();
        if !release {
            add64!(current_gui_frame_commands, count);
            if (*s).current_gui_frame_commands > (*s).peak_gui_frame_commands {
                (*s).peak_gui_frame_commands = (*s).current_gui_frame_commands;
            }
        } else if (*s).current_gui_frame_commands >= count {
            (*s).current_gui_frame_commands -= count;
        } else {
            (*s).current_gui_frame_commands = 0;
            add64!(header_errors, 1);
        }
    }
}
fn instance_count(class: u8) -> *mut u32 {
    unsafe {
        let s = stats();
        match class {
            0 => ptr::addr_of_mut!((*s).active_console_instances),
            1 => ptr::addr_of_mut!((*s).active_gui_instances),
            2 => ptr::addr_of_mut!((*s).active_service_instances),
            _ => unreachable!(),
        }
    }
}
fn note_instance(class: u8, bytes: u64, release: bool) {
    let _guard = Guard::acquire(0);
    unsafe {
        if !release {
            add32!(active_instances, 1);
            (*state()).active_published_bytes =
                (*state()).active_published_bytes.wrapping_add(bytes);
            let c = instance_count(class);
            c.write(c.read().wrapping_add(1));
        } else {
            let s = stats();
            if (*s).active_instances > 0 {
                (*s).active_instances -= 1;
            } else {
                add64!(header_errors, 1);
            }
            let c = instance_count(class);
            if c.read() > 0 {
                c.write(c.read() - 1);
            } else {
                add64!(header_errors, 1);
            }
        }
        refresh();
    }
}
pub(super) fn quarantine(kind: u16, bytes: usize) {
    let _guard = Guard::acquire(0);
    unsafe {
        let c = count(kind);
        if c.read() > 0 {
            c.write(c.read() - 1);
        } else {
            add64!(header_errors, 1);
        }
    }
    add64!(quarantined_payloads, 1);
    add64!(quarantined_bytes, bytes as u64);
    refresh();
}
fn injected() -> bool {
    unsafe {
        if !(*state()).failure_after_present {
            return false;
        }
        let target = (*state()).failure_after_value;
        let cursor = (*state()).failure_cursor;
        (*state()).failure_cursor = cursor.wrapping_add(1);
        cursor == target
    }
}
fn note_frame_release(kind: u16) {
    unsafe {
        if !(*state()).release_trace_enabled || !frame_kind(kind) {
            return;
        }
        if (*state()).release_trace_len < 8 {
            let index = (*state()).release_trace_len;
            (*state()).release_trace[index] = kind;
            (*state()).release_trace_len += 1;
        }
    }
}
pub(super) unsafe fn valid(header: *const t::Header, owner: u32, kind: u16, bytes: usize) -> bool {
    unsafe {
        if bytes > u32::MAX as usize
            || (*header).magic != MAGIC
            || (*header).requested_bytes as usize != bytes
            || (*header).kind != kind
            || (*header).reserved != 0
        {
            add64!(header_errors, 1);
            return false;
        }
        if (*header).owner_id != owner {
            add64!(owner_mismatches, 1);
            return false;
        }
        true
    }
}
fn allocate(type_id: u8, owner: u32, kind: u16) -> *mut c_void {
    add64!(allocation_attempts, 1);
    if injected() {
        add64!(allocation_failures, 1);
        return ptr::null_mut();
    }
    let shape = t::shape(type_id);
    let memory = heap::r4os_rust_heap_alloc(shape.size, shape.align);
    if memory.is_null() {
        add64!(allocation_failures, 1);
        return ptr::null_mut();
    }
    unsafe {
        t::initialize(type_id, memory);
        memory
            .add(shape.header)
            .cast::<t::Header>()
            .write(t::Header {
                magic: MAGIC,
                owner_id: owner,
                requested_bytes: u32::try_from(shape.size).expect("Payload size"),
                kind,
                reserved: 0,
            });
    }
    add64!(payload_allocations, 1);
    note_allocation(kind, shape.size);
    if !unsafe { info::r4os_rust_program_info_by_id(owner) }.is_null() {
        note_active(shape.size, false);
    }
    memory.cast()
}
unsafe fn release(type_id: u8, payload: *mut c_void, owner: u32, kind: u16) -> bool {
    let shape = t::shape(type_id);
    unsafe {
        if !valid(
            payload.cast::<u8>().add(shape.header).cast(),
            owner,
            kind,
            shape.size,
        ) {
            quarantine(kind, shape.size);
            return false;
        }
        if registry::r4os_rust_program_registry_predicate(4, owner, ptr::null()) {
            note_active(shape.size, true);
        }
        note_frame_release(kind);
        if heap::r4os_rust_heap_free(payload.cast(), shape.size) != 0 {
            add64!(free_failures, 1);
            quarantine(kind, shape.size);
            return false;
        }
    }
    add64!(payload_releases, 1);
    note_release(kind, shape.size);
    true
}
pub(super) fn command_bytes(capacity: u32) -> Option<usize> {
    if capacity == 0 {
        return None;
    }
    let bytes = (capacity as usize).checked_mul(mem::size_of::<t::GuiCommand>())?;
    let bytes = mem::size_of::<t::Commands>().checked_add(bytes)?;
    (bytes <= u32::MAX as usize).then_some(bytes)
}
pub(super) fn resource_bytes(count: usize) -> Option<usize> {
    if count == 0 {
        return None;
    }
    let bytes = mem::size_of::<t::Resource>().checked_add(count)?;
    (bytes <= u32::MAX as usize).then_some(bytes)
}
pub(super) fn resource_kind(kind: u16) -> Option<u16> {
    match kind {
        2 | 3 => Some(8),
        1 | 4 | 5 | 6 | 7 => Some(10),
        0 => None,
        _ => unreachable!(),
    }
}
pub(super) fn allocate_commands(owner: u32, offset: u64, capacity: u32) -> *mut t::Commands {
    let Some(bytes) = command_bytes(capacity) else {
        return ptr::null_mut();
    };
    add64!(allocation_attempts, 1);
    if injected() {
        add64!(allocation_failures, 1);
        return ptr::null_mut();
    }
    let memory = heap::r4os_rust_heap_alloc(bytes, mem::align_of::<t::Commands>());
    if memory.is_null() {
        add64!(allocation_failures, 1);
        return ptr::null_mut();
    }
    unsafe {
        ptr::write_bytes(memory, 0, bytes);
        let payload = memory.cast::<t::Commands>();
        (*payload).header = t::Header {
            magic: MAGIC,
            owner_id: owner,
            requested_bytes: bytes as u32,
            kind: 7,
            reserved: 0,
        };
        (*payload).logical_offset = offset;
        (*payload).capacity = capacity;
        add64!(payload_allocations, 1);
        note_allocation(7, bytes);
        if !info::r4os_rust_program_info_by_id(owner).is_null() {
            note_active(bytes, false);
        }
        payload
    }
}
pub(super) fn allocate_resource(
    owner: u32,
    offset: u64,
    raster_offset: u64,
    count: usize,
    kind: u16,
) -> *mut t::Resource {
    let Some(payload_kind) = resource_kind(kind) else {
        return ptr::null_mut();
    };
    let Some(bytes) = resource_bytes(count) else {
        return ptr::null_mut();
    };
    add64!(allocation_attempts, 1);
    if injected() {
        add64!(allocation_failures, 1);
        return ptr::null_mut();
    }
    let memory = heap::r4os_rust_heap_alloc(bytes, mem::align_of::<t::Resource>());
    if memory.is_null() {
        add64!(allocation_failures, 1);
        return ptr::null_mut();
    }
    unsafe {
        ptr::write_bytes(memory, 0, bytes);
        let payload = memory.cast::<t::Resource>();
        (*payload).header = t::Header {
            magic: MAGIC,
            owner_id: owner,
            requested_bytes: bytes as u32,
            kind: payload_kind,
            reserved: 0,
        };
        (*payload).logical_offset = offset;
        (*payload).raster_word_offset = raster_offset;
        (*payload).byte_count = u32::try_from(count).expect("Resource byte count");
        (*payload).resource_kind = kind;
        add64!(payload_allocations, 1);
        note_allocation(payload_kind, bytes);
        if !info::r4os_rust_program_info_by_id(owner).is_null() {
            note_active(bytes, false);
        }
        payload
    }
}
pub(super) unsafe fn release_commands(payload: *mut t::Commands, owner: u32) -> bool {
    unsafe {
        let Some(bytes) = command_bytes((*payload).capacity) else {
            quarantine(7, mem::size_of::<t::Commands>());
            return false;
        };
        if !valid(ptr::addr_of!((*payload).header), owner, 7, bytes) {
            quarantine(7, bytes);
            return false;
        }
        if registry::r4os_rust_program_registry_predicate(4, owner, ptr::null()) {
            note_active(bytes, true);
        }
        note_commands(u64::from((*payload).command_count), true);
        note_frame_release(7);
        if heap::r4os_rust_heap_free(payload.cast(), bytes) != 0 {
            add64!(free_failures, 1);
            quarantine(7, bytes);
            return false;
        }
        add64!(payload_releases, 1);
        note_release(7, bytes);
        true
    }
}
pub(super) unsafe fn release_resource(payload: *mut t::Resource, owner: u32) -> bool {
    unsafe {
        let Some(kind) = resource_kind((*payload).resource_kind) else {
            quarantine(10, mem::size_of::<t::Resource>());
            return false;
        };
        let Some(bytes) = resource_bytes((*payload).byte_count as usize) else {
            quarantine(kind, mem::size_of::<t::Resource>());
            return false;
        };
        if !valid(ptr::addr_of!((*payload).header), owner, kind, bytes) {
            quarantine(kind, bytes);
            return false;
        }
        if registry::r4os_rust_program_registry_predicate(4, owner, ptr::null()) {
            note_active(bytes, true);
        }
        note_frame_release(kind);
        if heap::r4os_rust_heap_free(payload.cast(), bytes) != 0 {
            add64!(free_failures, 1);
            quarantine(kind, bytes);
            return false;
        }
        add64!(payload_releases, 1);
        note_release(kind, bytes);
        true
    }
}
#[repr(C)]
pub struct Span {
    pub data: *mut c_void,
    pub len: usize,
}
/// # Safety
/// Original internal private type/kind pair and allocation caller lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_payload_allocate(
    type_id: u8,
    owner: u32,
    kind: u16,
) -> *mut c_void {
    allocate(type_id, owner, kind)
}
/// # Safety
/// Exact retained allocation and its original owner/kind/type, with no held
/// registry owner over heap release. Failed frees retain their quarantine edge.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_payload_release(
    type_id: u8,
    payload: *mut c_void,
    owner: u32,
    kind: u16,
) -> bool {
    unsafe { release(type_id, payload, owner, kind) }
}
/// # Safety
/// Exact resident header; requested bytes is its original allocation extent.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_payload_validate(
    header: *const t::Header,
    owner: u32,
    kind: u16,
    bytes: usize,
) -> bool {
    unsafe { valid(header, owner, kind, bytes) }
}
/// # Safety
/// Offset/width are compiler-derived fields of the same guarded private Stats.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_payload_stat(offset: u32, width: u8, amount: u64) {
    add(offset as usize, width as usize, amount);
}
/// # Safety
/// Same diagnostic boot/single-test ownership as the previous failure seam.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_payload_failure(present: bool, after: u32) {
    unsafe {
        (*state()).failure_after_present = present;
        (*state()).failure_after_value = after;
        (*state()).failure_cursor = 0;
    }
}
/// # Safety
/// Original boot/single-test ownership of the diagnostic failure cursor.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_payload_injected() -> bool {
    injected()
}
/// # Safety
/// Original self-test trace ownership; the trace never grows beyond eight kinds.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_payload_release_trace(kind: u16) {
    note_frame_release(kind);
}
/// # Safety
/// Internal valid kind/class. op0=allocation,1=release,2/3=active bytes,
/// 4/5=commands,6=published instance,7=retired instance,8=quarantine,9=refresh.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_payload_account(op: u8, kind: u16, bytes: u64) {
    match op {
        0 => note_allocation(kind, bytes as usize),
        1 => note_release(kind, bytes as usize),
        2 => note_active(bytes as usize, false),
        3 => note_active(bytes as usize, true),
        4 => note_commands(bytes, false),
        5 => note_commands(bytes, true),
        6 => note_instance(kind as u8, bytes, false),
        7 => note_instance(kind as u8, 0, true),
        8 => quarantine(kind, bytes as usize),
        9 => refresh(),
        _ => unreachable!(),
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_payload_is_frame(kind: u16) -> bool {
    frame_kind(kind)
}
/// # Safety
/// Original private sizing input; only a successful result writes exclusive out.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_payload_bytes(
    resource: bool,
    value: usize,
    out: *mut usize,
) -> bool {
    let bytes = if resource {
        resource_bytes(value)
    } else {
        command_bytes(u32::try_from(value).expect("Command capacity"))
    };
    if let Some(bytes) = bytes {
        unsafe { out.write(bytes) };
        true
    } else {
        false
    }
}
/// # Safety
/// Resident original allocation header. op0 returns command elements,1 resource
/// bytes,2 resource words. Const/mutable permission remains with the caller.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_payload_span(op: u8, payload: *const c_void) -> Span {
    unsafe {
        match op {
            0 => Span {
                data: payload
                    .cast::<u8>()
                    .add(mem::size_of::<t::Commands>())
                    .cast_mut()
                    .cast(),
                len: (*payload.cast::<t::Commands>()).capacity as usize,
            },
            1 | 2 => {
                let data = payload
                    .cast::<u8>()
                    .add(mem::size_of::<t::Resource>())
                    .cast_mut();
                let len = (*payload.cast::<t::Resource>()).byte_count as usize;
                if op == 2 {
                    assert!(data.addr().is_multiple_of(mem::align_of::<u32>()));
                }
                Span {
                    data: data.cast(),
                    len: if op == 2 {
                        len / mem::size_of::<u32>()
                    } else {
                        len
                    },
                }
            }
            _ => unreachable!(),
        }
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_payload_resource_kind(kind: u16) -> u16 {
    resource_kind(kind).unwrap_or(0)
}
/// # Safety
/// Original allocation owner lifetime; logical/raster offsets are value metadata.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_payload_commands(
    owner: u32,
    offset: u64,
    capacity: u32,
) -> *mut t::Commands {
    allocate_commands(owner, offset, capacity)
}
/// # Safety
/// Original allocation owner lifetime and admitted resource-kind byte.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_payload_resource(
    owner: u32,
    offset: u64,
    raster_offset: u64,
    count: usize,
    kind: u16,
) -> *mut t::Resource {
    allocate_resource(owner, offset, raster_offset, count, kind)
}
/// # Safety
/// Exact variable-sized allocation and its original owner identity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_payload_variable_release(
    resource: bool,
    payload: *mut c_void,
    owner: u32,
) -> bool {
    unsafe {
        if resource {
            release_resource(payload.cast(), owner)
        } else {
            release_commands(payload.cast(), owner)
        }
    }
}
/// # Safety
/// Exact resident instance. Required payload projections preserve the original
/// unreachable-on-missing invariant; caller determines const/mutable permission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_payload_pointer(
    op: u8,
    instance: *const ProgramInstance,
) -> *mut c_void {
    unsafe {
        let p = match op {
            0 => (*instance).runtime_payload,
            1 => (*instance).process_payload,
            2 => (*instance).console_payload,
            3 => (*instance).gui_payload,
            _ => unreachable!(),
        };
        assert!(!p.is_null(), "Missing required program payload");
        p
    }
}
/// # Safety
/// Exclusive resident output; passive stats retain original owner/refresh order.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_payload_stats(out: *mut t::Stats) {
    let _guard = Guard::acquire(0);
    refresh();
    unsafe { out.write(stats().read()) };
}
const _: () = {
    assert!(mem::size_of::<State>() == 416);
    assert!(mem::align_of::<State>() == 8);
    assert!(mem::offset_of!(State, stats) == 0);
    assert!(mem::offset_of!(State, report) == 336);
    assert!(mem::offset_of!(State, active_published_bytes) == 360);
    assert!(mem::offset_of!(State, self_test_peak_bytes) == 368);
    assert!(mem::offset_of!(State, failure_after_value) == 376);
    assert!(mem::offset_of!(State, failure_cursor) == 380);
    assert!(mem::offset_of!(State, release_trace) == 384);
    assert!(mem::offset_of!(State, release_trace_len) == 400);
    assert!(mem::offset_of!(State, failure_after_present) == 408);
    assert!(mem::offset_of!(State, self_test_active) == 409);
    assert!(mem::offset_of!(State, self_test_running) == 410);
    assert!(mem::offset_of!(State, release_trace_enabled) == 411);
    assert!(mem::size_of::<Span>() == 16);
};
/// # Safety
/// Original diagnostic single-test ownership and exclusive resident report out.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_payload_report(out: *mut t::SelfTestReport) {
    unsafe { out.write(ptr::addr_of!((*state()).report).read()) };
}

/// # Safety
/// Same required resident payload lifetime as the canonical instance projection.
pub(super) unsafe fn runtime(instance: *const ProgramInstance) -> *mut t::Runtime {
    unsafe { r4os_rust_program_payload_pointer(0, instance).cast() }
}
/// # Safety
/// Same required resident payload lifetime as the canonical instance projection.
pub(super) unsafe fn process(instance: *const ProgramInstance) -> *mut t::Process {
    unsafe { r4os_rust_program_payload_pointer(1, instance).cast() }
}
/// # Safety
/// Same required resident payload lifetime as the canonical instance projection.
pub(super) unsafe fn gui(instance: *const ProgramInstance) -> *mut t::Gui {
    unsafe { r4os_rust_program_payload_pointer(3, instance).cast() }
}

pub(super) unsafe fn console(instance: *const ProgramInstance) -> *mut t::Console {
    unsafe {
        let p = (*instance).console_payload;
        assert!(!p.is_null(), "Console payload missing");
        p.cast()
    }
}

pub(super) unsafe fn registry_telemetry(op: u8, instance: *mut ProgramInstance) {
    unsafe {
        match op {
            0 => {
                add64!(
                    registry_reserved_core_bytes,
                    mem::size_of::<super::registry_types::ProgramRegistryChunk>() as u64
                );
                refresh();
            }
            1 => {
                let _guard = Guard::acquire(0);
                let size = mem::size_of::<super::registry_types::ProgramRegistryChunk>() as u64;
                if (*stats()).registry_reserved_core_bytes >= size {
                    (*stats()).registry_reserved_core_bytes -= size;
                }
                refresh();
            }
            2 => note_instance(
                (*instance).app_class,
                super::instance_storage::bytes(instance),
                false,
            ),
            _ => unreachable!(),
        }
    }
}
