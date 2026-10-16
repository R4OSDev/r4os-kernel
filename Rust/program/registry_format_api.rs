//! Stateless C transports for the kernel-private registry codec. Byte spans
//! retain caller ownership; neither language exposes its native slice layout.
use super::{
    registry_format as format, registry_format_builder as builder, registry_format_types::*,
};

/// # Safety
/// Input is immutable and output is exclusive resident storage for this call;
/// the input lifetime also covers every subsequent use of the returned view.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_registry_parse(input: Bytes, out: *mut View) -> u32 {
    unsafe {
        match format::parse(input) {
            Ok(view) => {
                out.write(view);
                0
            }
            Err(error) => error as u32,
        }
    }
}
/// # Safety
/// Caller owns exclusive, disjoint output and scratch; value/name spans remain
/// immutable through this call and subsequent uses of returned scratch names.
/// Optional callbacks only marshal fields in those exact caller-owned arrays.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_registry_build(input: *const BuildInput, out: *mut View) -> u32 {
    unsafe {
        match builder::build(input.read()) {
            Ok(view) => {
                out.write(view);
                0
            }
            Err(error) => error as u32,
        }
    }
}
/// # Safety
/// Path remains readable for this call and for use of the returned suffix.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_registry_root(path: Bytes, out: *mut ParsedRoot) -> bool {
    unsafe {
        if let Some(root) = format::parse_root(path.slice()) {
            out.write(root);
            true
        } else {
            false
        }
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_registry_enum(value: u16, kind: bool) -> bool {
    if kind {
        format::kind_valid(value)
    } else {
        format::type_valid(value)
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_registry_root_name(kind: u16) -> Bytes {
    Bytes::from_slice(format::root_name(kind))
}
/// # Safety
/// View is a validated live immutable input view and index is in its selected
/// table. The selected output is exclusive fixed-layout storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_registry_record(
    view: *const View,
    index: u32,
    key: bool,
    out_key: *mut KeyRecord,
    out_value: *mut ValueRecord,
) {
    unsafe {
        if key {
            out_key.write(format::key_at(view.read(), index));
        } else {
            out_value.write(format::value_at(view.read(), index));
        }
    }
}
/// # Safety
/// View and the selected record belong to the same validated, live immutable
/// input; its heap storage outlives the returned span.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_registry_heap(
    view: *const View,
    operation: u8,
    key: *const KeyRecord,
    value: *const ValueRecord,
) -> Bytes {
    unsafe {
        let view = view.read();
        Bytes::from_slice(match operation {
            0 => format::key_name(view, key.read()),
            1 => format::value_name(view, value.read()),
            2 => format::value_data(view, value.read()),
            _ => panic!("Invalid registry heap operation"),
        })
    }
}
/// # Safety
/// Validated live view and immutable path/name spans last through this call.
/// Success writes the exclusive index output; failure leaves it untouched.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_registry_find(
    view: *const View,
    operation: u8,
    owner: u32,
    name: Bytes,
    out: *mut u32,
) -> bool {
    unsafe {
        let view = view.read();
        let name = name.slice();
        let result = match operation {
            0 => format::find_key(view, name),
            1 => format::find_child(view, owner, name),
            2 => format::find_value(view, owner, name),
            _ => panic!("Invalid registry lookup operation"),
        };
        if let Some(index) = result {
            out.write(index);
            true
        } else {
            false
        }
    }
}
/// # Safety
/// View/path/name retain their immutable input lifetimes. Success writes an
/// exclusive output whose data continues to borrow the original view storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_registry_get(
    view: *const View,
    path: Bytes,
    name: Bytes,
    out: *mut Value,
) -> bool {
    unsafe {
        if let Some(value) = format::get_value(view.read(), path.slice(), name.slice()) {
            out.write(value);
            true
        } else {
            false
        }
    }
}
/// # Safety
/// Value is readable caller-owned storage; selected outputs are exclusive.
/// String spans retain the original immutable value lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_registry_scalar(
    value: *const Value,
    operation: u8,
    out: *mut u64,
    bytes: *mut Bytes,
) -> bool {
    unsafe {
        let value = value.read();
        let data = value.data.slice();
        match operation {
            0 => {
                if value.value_type != 1 {
                    return false;
                }
                bytes.write(value.data);
            }
            1 => {
                if value.value_type != 2 || data.len() != 4 {
                    return false;
                }
                out.write(u64::from(format::read32(data, 0)));
            }
            2 => {
                if value.value_type != 3 || data.len() != 8 {
                    return false;
                }
                out.write(format::read64(data, 0));
            }
            3 => {
                if value.value_type != 4 || data.len() != 1 || data[0] > 1 {
                    return false;
                }
                out.write(u64::from(data[0]));
            }
            _ => panic!("Invalid registry scalar operation"),
        }
        true
    }
}
