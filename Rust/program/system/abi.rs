//! Every field of the private R4SYS transports is checked in both compilers.
use super::super::registry_format_types::Bytes;
use super::{
    atomic::UpdateInput,
    context::{StreamOwner, Target},
    io::Failure,
    stream_publish::ClaimInput,
};
const _: () = {
    assert!(core::mem::size_of::<Bytes>() == 16);
    assert!(core::mem::align_of::<Bytes>() == 8);
    assert!(core::mem::offset_of!(Bytes, data) == 0);
    assert!(core::mem::offset_of!(Bytes, len) == 8);
    assert!(core::mem::size_of::<Target>() == 24);
    assert!(core::mem::align_of::<Target>() == 8);
    assert!(core::mem::offset_of!(Target, drive_ref) == 0);
    assert!(core::mem::offset_of!(Target, path) == 8);
    assert!(core::mem::size_of::<StreamOwner>() == 32);
    assert!(core::mem::align_of::<StreamOwner>() == 8);
    assert!(core::mem::offset_of!(StreamOwner, kind) == 0);
    assert!(core::mem::offset_of!(StreamOwner, id) == 4);
    assert!(core::mem::offset_of!(StreamOwner, generation) == 8);
    assert!(core::mem::offset_of!(StreamOwner, program_id) == 16);
    assert!(core::mem::offset_of!(StreamOwner, program_generation) == 24);
    assert!(core::mem::size_of::<Failure>() == 20);
    assert!(core::mem::align_of::<Failure>() == 4);
    assert!(core::mem::offset_of!(Failure, sequence) == 0);
    assert!(core::mem::offset_of!(Failure, task_id) == 4);
    assert!(core::mem::offset_of!(Failure, kind) == 8);
    assert!(core::mem::offset_of!(Failure, result) == 12);
    assert!(core::mem::offset_of!(Failure, lookup_stage) == 16);
    assert!(core::mem::size_of::<ClaimInput>() == 192);
    assert!(core::mem::align_of::<ClaimInput>() == 8);
    assert!(core::mem::offset_of!(ClaimInput, volume) == 0);
    assert!(core::mem::offset_of!(ClaimInput, parent) == 88);
    assert!(core::mem::offset_of!(ClaimInput, staged) == 104);
    assert!(core::mem::offset_of!(ClaimInput, target) == 120);
    assert!(core::mem::offset_of!(ClaimInput, backup) == 136);
    assert!(core::mem::offset_of!(ClaimInput, node) == 152);
    assert!(core::mem::offset_of!(ClaimInput, node_generation) == 160);
    assert!(core::mem::offset_of!(ClaimInput, size) == 168);
    assert!(core::mem::offset_of!(ClaimInput, protocol) == 176);
    assert!(core::mem::offset_of!(ClaimInput, generation) == 184);
    assert!(core::mem::size_of::<UpdateInput>() == 216);
    assert!(core::mem::align_of::<UpdateInput>() == 8);
    assert!(core::mem::offset_of!(UpdateInput, volume) == 0);
    assert!(core::mem::offset_of!(UpdateInput, parent) == 88);
    assert!(core::mem::offset_of!(UpdateInput, target) == 96);
    assert!(core::mem::offset_of!(UpdateInput, staged) == 112);
    assert!(core::mem::offset_of!(UpdateInput, backup) == 128);
    assert!(core::mem::offset_of!(UpdateInput, previous) == 144);
    assert!(core::mem::offset_of!(UpdateInput, forward) == 160);
    assert!(core::mem::offset_of!(UpdateInput, long_stage) == 161);
    assert!(core::mem::offset_of!(UpdateInput, target_existed) == 162);
    assert!(core::mem::offset_of!(UpdateInput, old_known) == 163);
    assert!(core::mem::offset_of!(UpdateInput, previous_known) == 164);
    assert!(core::mem::offset_of!(UpdateInput, new_size) == 168);
    assert!(core::mem::offset_of!(UpdateInput, new_checksum) == 176);
    assert!(core::mem::offset_of!(UpdateInput, old_size) == 184);
    assert!(core::mem::offset_of!(UpdateInput, old_checksum) == 192);
    assert!(core::mem::offset_of!(UpdateInput, previous_size) == 200);
    assert!(core::mem::offset_of!(UpdateInput, previous_checksum) == 208);
};
