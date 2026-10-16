//! Original atomic replace, declared upload publication and checked SYSUPD
//! admission. Durable claim/update peers retain their own still-assigned owner.
use super::super::registry_format_types::Bytes;
use super::{
    context::{self as ctx, PathBuffers, Target},
    fs, path, registry_state, stream, stream_publish as publish, stream_state as s,
};
use crate::contract as c;
use core::{mem::MaybeUninit, ptr};
macro_rules! some {
    ($value:expr,$error:expr) => {
        match $value {
            Some(value) => value,
            None => return $error,
        }
    };
}
unsafe fn targets(
    buffers: &mut [PathBuffers; 3],
    inputs: [*const u8; 3],
    invalid: i32,
    bad: i32,
) -> Result<[Target; 3], i32> {
    unsafe {
        for (buffer, input) in buffers.iter_mut().zip(inputs) {
            if !buffer.copy(input) {
                return Err(invalid);
            }
        }
        let mut output = [MaybeUninit::uninit(); 3];
        for (slot, buffer) in output.iter_mut().zip(buffers) {
            slot.write(buffer.resolve_copied().ok_or(bad)?);
        }
        Ok(output.map(|slot| slot.assume_init()))
    }
}
unsafe fn same_volume(a: fs::Volume, b: fs::Volume) -> bool {
    unsafe {
        crate::fs::vfs::r4os_rust_vfs_identity(
            0,
            &fs::Args {
                volume: a,
                other: b,
                ..Default::default()
            },
        )
    }
}
unsafe fn short(name: &[u8]) -> bool {
    unsafe {
        fs::run(
            fs::op::SHORT_NAME,
            fs::Args {
                name: fs::bytes(name),
                ..Default::default()
            },
            ptr::null_mut(),
        )
        .present
    }
}
/// # Safety
/// Original three immutable NUL paths and admitted caller lifetime through
/// the exact tuple transaction. External selects only cache invalidation;
/// internal registry commits retain/publish their own candidate themselves.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_file_replace(
    target: *const u8,
    staged: *const u8,
    backup: *const u8,
    flags: u32,
    external: bool,
) -> i32 {
    unsafe {
        if flags == 0 || flags & !7 != 0 {
            return -1;
        }
        let consume = flags & 1 != 0;
        let absent = flags & 2 != 0;
        let require_owned = flags & 4 != 0;
        if absent && !consume || require_owned && (!consume || !absent) {
            return -1;
        }
        let mut buffers = [const { PathBuffers::new() }; 3];
        let targets = match targets(&mut buffers, [target, staged, backup], -1, -4) {
            Ok(value) => value,
            Err(error) => return error,
        };
        let [target, staged, backup] = targets;
        let volume = some!(ctx::volume(target), -2);
        let staged_volume = some!(ctx::volume(staged), -2);
        let backup_volume = some!(ctx::volume(backup), -2);
        if !same_volume(volume, staged_volume) || !same_volume(volume, backup_volume) {
            return -8;
        }
        let paths = [
            target.path.slice(),
            staged.path.slice(),
            backup.path.slice(),
        ];
        if paths.iter().any(|path| path::root(path)) {
            return -4;
        }
        let parent_path = path::parent(paths[0]);
        if parent_path != path::parent(paths[1]) || parent_path != path::parent(paths[2]) {
            return -8;
        }
        let names = paths.map(path::base);
        if !short(names[1]) || !short(names[2]) {
            return -8;
        }
        let mut request = some!(
            fs::Request::volume(23, (*target.drive_ref).letter, volume),
            -7
        );
        let mut parent = 0;
        match fs::parent(volume, parent_path, &mut parent) {
            0 => {}
            1 => return -4,
            2 => return -7,
            _ => unreachable!(),
        }
        let mut entries = [None, None, None];
        let mut statuses = [0; 3];
        for index in 0..3 {
            statuses[index] = fs::optional(volume, paths[index], &mut entries[index]);
        }
        let mut owned = None;
        let mut resume = false;
        if require_owned {
            let slot = some!(
                stream::find(
                    (*staged.drive_ref).letter,
                    ctx::boot_target(staged),
                    parent,
                    if statuses[1] == 0 {
                        entries[1].as_ref()
                    } else {
                        None
                    },
                    names[1],
                    true
                ),
                -6
            );
            if !(*slot).ready || !(*slot).finished {
                return -6;
            }
            if (*slot).publish_started {
                if !publish::tuple_matches(slot, names[0], names[2]) {
                    return -6;
                }
                resume = true;
            } else {
                if statuses.contains(&2) {
                    return -7;
                }
                if entries[0].is_some() || entries[2].is_some() {
                    return -6;
                }
                let first = some!(entries[1].as_ref(), -3);
                if (*slot).size != first.size {
                    return -6;
                }
                if (*slot).publish_declared && !publish::names_match(slot, names[0], names[2]) {
                    return -6;
                }
            }
            owned = Some(slot);
        } else if absent {
            if statuses.contains(&2) {
                return -7;
            }
            if entries[0].is_some() {
                return -6;
            }
        }
        let ignored = owned.map_or(ptr::null(), |slot| slot.cast_const());
        for index in 0..3 {
            if stream::collision(
                (*targets[index].drive_ref).letter,
                ctx::boot_target(targets[index]),
                parent,
                if statuses[index] == 0 {
                    entries[index].as_ref()
                } else {
                    None
                },
                names[index],
                ignored,
            ) {
                return -7;
            }
        }
        if !require_owned {
            for index in [0, 2, 1] {
                if statuses[index] != 2
                    && !stream::invalidate(
                        (*targets[index].drive_ref).letter,
                        ctx::boot_target(targets[index]),
                        parent,
                        entries[index].as_ref(),
                        names[index],
                    )
                {
                    return -7;
                }
            }
        }
        let mut result;
        if let Some(slot) = owned {
            if !resume && (*slot).publish_claim_generation == 0 {
                if statuses[1] != 0 {
                    return -7;
                }
                let parent = some!(publish::parent((*target.drive_ref).letter, paths[0]), -7);
                let entry = entries[1].as_ref().expect("Found owned publish stage");
                let generation = some!(
                    publish::claim(
                        false,
                        volume,
                        parent,
                        names[1],
                        names[0],
                        names[2],
                        entry.node,
                        entry.generation,
                        entry.size,
                        3,
                        0
                    ),
                    -7
                );
                (*slot).publish_claim_generation = generation;
            }
            if !resume && !publish::remember(slot, names[0], names[2]) {
                if (*slot).publish_claim_generation != 0
                    && publish::retire((*slot).publish_claim_generation)
                {
                    (*slot).publish_claim_generation = 0;
                }
                return -8;
            }
            result = publish::run(volume, parent, names[0], names[1], names[2], slot);
            if result == 0 && (*slot).publish_claim_generation != 0 {
                if publish::retire((*slot).publish_claim_generation) {
                    (*slot).publish_claim_generation = 0;
                } else {
                    result = -6;
                }
            }
        } else {
            result = fs::replace(
                volume, parent, names[0], names[1], names[2], absent, consume,
            );
        }
        if result == 0
            && let Some(slot) = owned
        {
            s::clear(slot);
        }
        if external {
            for buffer in &buffers {
                registry_state::r4os_rust_sys_registry_invalidate(Bytes::from_slice(buffer.raw()));
            }
        }
        request.ok = result == 0;
        match result {
            0 => 0,
            -1 => -1,
            -2 => -3,
            -3 => -5,
            -4 | -5 => -6,
            -6 => -7,
            -7 => -8,
            _ => unreachable!(),
        }
    }
}
/// # Safety
/// Original three NUL paths, protocol and exact caller/request lifetime.
/// Only the current logical stream owner can mint a durable publish claim.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_stream_declare(
    staged: *const u8,
    target: *const u8,
    backup: *const u8,
    protocol: u32,
) -> i32 {
    unsafe {
        let protocol = match protocol {
            c::file_stream_publish_protocol_sftp => 0,
            c::file_stream_publish_protocol_scp => 1,
            c::file_stream_publish_protocol_ftp => 2,
            _ => return -1,
        };
        let mut buffers = [const { PathBuffers::new() }; 3];
        let targets = match targets(&mut buffers, [staged, target, backup], -1, -3) {
            Ok(value) => value,
            Err(error) => return error,
        };
        let [staged, target, backup] = targets;
        let paths = [
            staged.path.slice(),
            target.path.slice(),
            backup.path.slice(),
        ];
        if paths.iter().any(|path| path::root(path)) {
            return -3;
        }
        let volume = some!(ctx::volume(staged), -2);
        let tv = some!(ctx::volume(target), -2);
        let bv = some!(ctx::volume(backup), -2);
        if !same_volume(volume, tv) || !same_volume(volume, bv) {
            return -5;
        }
        let parent_path = path::parent(paths[0]);
        if parent_path != path::parent(paths[1]) || parent_path != path::parent(paths[2]) {
            return -5;
        }
        let names = paths.map(path::base);
        if !short(names[0]) || !short(names[2]) {
            return -5;
        }
        let letter = (*staged.drive_ref).letter;
        let mut request = some!(fs::Request::volume(6, letter, volume), -6);
        let mut parent = 0;
        if fs::parent(volume, parent_path, &mut parent) != 0 {
            return -3;
        }
        let mut entry = None;
        if fs::optional(volume, paths[0], &mut entry) == 2 {
            return -6;
        }
        let slot = some!(
            stream::find(
                letter,
                ctx::boot_target(staged),
                parent,
                entry.as_ref(),
                names[0],
                true
            ),
            -4
        );
        if (*slot).publish_declared || (*slot).publish_claim_generation != 0 {
            return -1;
        }
        let parent = some!(publish::parent(letter, paths[0]), -6);
        let generation = some!(
            publish::claim(
                false,
                volume,
                parent,
                names[0],
                names[1],
                names[2],
                (*slot).file_node,
                (*slot).file_node_generation,
                (*slot).size,
                protocol,
                0
            ),
            -6
        );
        (*slot).publish_claim_generation = generation;
        (*slot).publish_declared = true;
        (*slot).publish_protocol = protocol;
        if !publish::remember(slot, names[1], names[2]) {
            if publish::retire(generation) {
                (*slot).publish_claim_generation = 0;
                (*slot).publish_declared = false;
                (*slot).publish_protocol = 0;
            }
            return -5;
        }
        if parent.len() <= 1024 {
            ptr::copy_nonoverlapping(
                parent.as_ptr(),
                ptr::addr_of_mut!((*slot).publish_declared_parent).cast(),
                parent.len(),
            );
            (*slot).publish_declared_parent_len = parent.len();
        }
        (*slot).publish_started = false;
        request.ok = true;
        0
    }
}
#[derive(Clone, Copy)]
struct UpdateInput {
    pub volume: fs::Volume,
    pub parent: u64,
    pub target: Bytes,
    pub staged: Bytes,
    pub backup: Bytes,
    pub previous: Bytes,
    pub forward: bool,
    pub long_stage: bool,
    pub target_existed: bool,
    pub old_known: bool,
    pub previous_known: bool,
    pub new_size: u64,
    pub new_checksum: u32,
    pub old_size: u64,
    pub old_checksum: u32,
    pub previous_size: u64,
    pub previous_checksum: u32,
}
fn update(cleanup: bool, input: &UpdateInput) -> u8 {
    use crate::update::atomic as owner;
    let v = input;
    let span = |b: Bytes| fs::Bytes {
        data: b.data,
        len: b.len,
    };
    if cleanup {
        owner::cleanup(
            v.volume,
            v.parent,
            span(v.target),
            span(v.staged),
            span(v.backup),
            span(v.previous),
            owner::CleanupSpec {
                new_size: v.new_size,
                new_checksum: v.new_checksum,
                target_existed: v.target_existed,
                old_known: v.old_known,
                old_size: v.old_size,
                old_checksum: v.old_checksum,
                previous_known: v.previous_known,
                previous_size: v.previous_size,
                previous_checksum: v.previous_checksum,
            },
        )
    } else {
        owner::transition(
            v.volume,
            v.parent,
            span(v.target),
            span(v.staged),
            span(v.backup),
            if v.forward { 0 } else { 1 },
            owner::Spec {
                long_stage: v.long_stage,
                target_existed: v.target_existed,
                old_known: v.old_known,
                new_size: v.new_size,
                new_checksum: v.new_checksum,
                old_size: v.old_size,
                old_checksum: v.old_checksum,
            },
        )
    }
}
fn update_result(raw: u8) -> i32 {
    match raw {
        0 => 0,
        1 => -4,
        2 => -5,
        3 => -6,
        _ => unreachable!(),
    }
}
/// # Safety
/// Original three paths and caller lifetime through one checked update gate.
/// Native admission binds names/flags and all still-owned publish barriers;
/// the existing update subsystem performs its own exact content transaction.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_update_checked(
    target: *const u8,
    staged: *const u8,
    backup: *const u8,
    new_size: u64,
    new_checksum: u32,
    old_size: u64,
    old_checksum: u32,
    flags: u32,
) -> i32 {
    unsafe {
        if flags & !(15 | c::file_update_atomic_checked_flag_long_stage) != 0 {
            return -1;
        }
        let forward = flags & 1 != 0;
        let rollback = flags & 2 != 0;
        let target_existed = flags & 4 != 0;
        let old_known = flags & 8 != 0;
        let long_stage = flags & c::file_update_atomic_checked_flag_long_stage != 0;
        if forward == rollback || old_known && !target_existed || long_stage && !forward {
            return -1;
        }
        let mut buffers = [const { PathBuffers::new() }; 3];
        let targets = match targets(&mut buffers, [target, staged, backup], -1, -3) {
            Ok(value) => value,
            Err(error) => return error,
        };
        let [target, staged, backup] = targets;
        let volume = some!(ctx::volume(target), -2);
        let sv = some!(ctx::volume(staged), -2);
        let bv = some!(ctx::volume(backup), -2);
        if !same_volume(volume, sv) || !same_volume(volume, bv) {
            return -6;
        }
        let paths = [
            target.path.slice(),
            staged.path.slice(),
            backup.path.slice(),
        ];
        if paths.iter().any(|path| path::root(path)) {
            return -3;
        }
        let parent_path = path::parent(paths[0]);
        if parent_path != path::parent(paths[1]) || parent_path != path::parent(paths[2]) {
            return -6;
        }
        let names = paths.map(path::base);
        if !short(names[1]) && !long_stage || !short(names[2]) || long_stage && volume.kind != 0 {
            return -6;
        }
        let mut request = some!(
            fs::Request::volume(25, (*target.drive_ref).letter, volume),
            -5
        );
        let mut parent = 0;
        match fs::parent(volume, parent_path, &mut parent) {
            0 => {}
            1 => return -3,
            2 => return -5,
            _ => unreachable!(),
        }
        for index in 0..3 {
            if stream::collision(
                (*targets[index].drive_ref).letter,
                ctx::boot_target(targets[index]),
                parent,
                None,
                names[index],
                ptr::null(),
            ) {
                return -5;
            }
        }
        for index in 0..3 {
            stream::invalidate(
                (*targets[index].drive_ref).letter,
                ctx::boot_target(targets[index]),
                parent,
                None,
                names[index],
            );
        }
        let result = update(
            false,
            &UpdateInput {
                volume,
                parent,
                target: Bytes::from_slice(names[0]),
                staged: Bytes::from_slice(names[1]),
                backup: Bytes::from_slice(names[2]),
                previous: Bytes::EMPTY,
                forward,
                long_stage,
                target_existed,
                old_known,
                previous_known: false,
                new_size,
                new_checksum,
                old_size,
                old_checksum,
                previous_size: 0,
                previous_checksum: 0,
            },
        );
        request.ok = result == 0;
        update_result(result)
    }
}
/// # Safety
/// Original four path lifetimes, fingerprints and exact checked-cleanup request.
/// Empty previous remains the original explicit absence of an inherited backup.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_update_cleanup(
    target: *const u8,
    staged: *const u8,
    backup: *const u8,
    previous: *const u8,
    new_size: u64,
    new_checksum: u32,
    old_size: u64,
    old_checksum: u32,
    previous_size: u64,
    previous_checksum: u32,
    flags: u32,
) -> i32 {
    unsafe {
        if flags & !7 != 0 {
            return -1;
        }
        let target_existed = flags & 1 != 0;
        let old_known = flags & 2 != 0;
        let previous_known = flags & 4 != 0;
        if old_known && !target_existed {
            return -1;
        }
        let mut buffers = [const { PathBuffers::new() }; 3];
        let mut previous_buffer = PathBuffers::new();
        // Preserve the original all-four-copy-before-resolve error precedence.
        for (buffer, input) in buffers.iter_mut().zip([target, staged, backup]) {
            if !buffer.copy(input) {
                return -1;
            }
        }
        if !previous_buffer.copy(previous) {
            return -1;
        }
        let mut resolved = [MaybeUninit::uninit(); 3];
        for (slot, buffer) in resolved.iter_mut().zip(&mut buffers) {
            slot.write(some!(buffer.resolve_copied(), -3));
        }
        let targets = resolved.map(|slot| slot.assume_init());
        let [target, staged, backup] = targets;
        let paths = [
            target.path.slice(),
            staged.path.slice(),
            backup.path.slice(),
        ];
        if paths.iter().any(|path| path::root(path)) {
            return -3;
        }
        let volume = some!(ctx::volume(target), -2);
        let sv = some!(ctx::volume(staged), -2);
        let bv = some!(ctx::volume(backup), -2);
        if !same_volume(volume, sv) || !same_volume(volume, bv) {
            return -6;
        }
        let parent_path = path::parent(paths[0]);
        if parent_path != path::parent(paths[1]) || parent_path != path::parent(paths[2]) {
            return -6;
        }
        let previous_name = if !previous_buffer.raw().is_empty() {
            let previous = some!(previous_buffer.resolve_copied(), -3);
            let path = previous.path.slice();
            if path::root(path) {
                return -3;
            }
            let pv = some!(ctx::volume(previous), -2);
            if !same_volume(volume, pv) || parent_path != path::parent(path) {
                return -6;
            }
            path::base(path)
        } else {
            &[]
        };
        let names = paths.map(path::base);
        if !short(names[1]) || !short(names[2]) {
            return -6;
        }
        let mut request = some!(
            fs::Request::volume(25, (*target.drive_ref).letter, volume),
            -5
        );
        let mut parent = 0;
        match fs::parent(volume, parent_path, &mut parent) {
            0 => {}
            1 => return -3,
            2 => return -5,
            _ => unreachable!(),
        }
        for index in [0, 2] {
            if stream::collision(
                (*targets[index].drive_ref).letter,
                ctx::boot_target(targets[index]),
                parent,
                None,
                names[index],
                ptr::null(),
            ) {
                return -5;
            }
        }
        let result = update(
            true,
            &UpdateInput {
                volume,
                parent,
                target: Bytes::from_slice(names[0]),
                staged: Bytes::from_slice(names[1]),
                backup: Bytes::from_slice(names[2]),
                previous: Bytes::from_slice(previous_name),
                forward: false,
                long_stage: false,
                target_existed,
                old_known,
                previous_known,
                new_size,
                new_checksum,
                old_size,
                old_checksum,
                previous_size,
                previous_checksum,
            },
        );
        request.ok = result == 0;
        update_result(result)
    }
}
