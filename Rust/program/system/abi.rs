//! Every field of the private R4SYS transports is checked in both compilers.
use super::super::registry_format_types::Bytes;
use super::{
    context::{StreamOwner, Target},
    io::Failure,
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
};
