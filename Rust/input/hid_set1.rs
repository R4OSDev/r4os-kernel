//! Original Set-1/HID identity tables and bounded report release collection.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Code {
    pub make: u8,
    pub extended: bool,
}
pub fn usage_to_code(v: u8) -> Option<Code> {
    match v {
        0x04 => Some(Code {
            make: 0x1E,
            extended: false,
        }),
        0x05 => Some(Code {
            make: 0x30,
            extended: false,
        }),
        0x06 => Some(Code {
            make: 0x2E,
            extended: false,
        }),
        0x07 => Some(Code {
            make: 0x20,
            extended: false,
        }),
        0x08 => Some(Code {
            make: 0x12,
            extended: false,
        }),
        0x09 => Some(Code {
            make: 0x21,
            extended: false,
        }),
        0x0A => Some(Code {
            make: 0x22,
            extended: false,
        }),
        0x0B => Some(Code {
            make: 0x23,
            extended: false,
        }),
        0x0C => Some(Code {
            make: 0x17,
            extended: false,
        }),
        0x0D => Some(Code {
            make: 0x24,
            extended: false,
        }),
        0x0E => Some(Code {
            make: 0x25,
            extended: false,
        }),
        0x0F => Some(Code {
            make: 0x26,
            extended: false,
        }),
        0x10 => Some(Code {
            make: 0x32,
            extended: false,
        }),
        0x11 => Some(Code {
            make: 0x31,
            extended: false,
        }),
        0x12 => Some(Code {
            make: 0x18,
            extended: false,
        }),
        0x13 => Some(Code {
            make: 0x19,
            extended: false,
        }),
        0x14 => Some(Code {
            make: 0x10,
            extended: false,
        }),
        0x15 => Some(Code {
            make: 0x13,
            extended: false,
        }),
        0x16 => Some(Code {
            make: 0x1F,
            extended: false,
        }),
        0x17 => Some(Code {
            make: 0x14,
            extended: false,
        }),
        0x18 => Some(Code {
            make: 0x16,
            extended: false,
        }),
        0x19 => Some(Code {
            make: 0x2F,
            extended: false,
        }),
        0x1A => Some(Code {
            make: 0x11,
            extended: false,
        }),
        0x1B => Some(Code {
            make: 0x2D,
            extended: false,
        }),
        0x1C => Some(Code {
            make: 0x15,
            extended: false,
        }),
        0x1D => Some(Code {
            make: 0x2C,
            extended: false,
        }),
        0x1E => Some(Code {
            make: 0x02,
            extended: false,
        }),
        0x1F => Some(Code {
            make: 0x03,
            extended: false,
        }),
        0x20 => Some(Code {
            make: 0x04,
            extended: false,
        }),
        0x21 => Some(Code {
            make: 0x05,
            extended: false,
        }),
        0x22 => Some(Code {
            make: 0x06,
            extended: false,
        }),
        0x23 => Some(Code {
            make: 0x07,
            extended: false,
        }),
        0x24 => Some(Code {
            make: 0x08,
            extended: false,
        }),
        0x25 => Some(Code {
            make: 0x09,
            extended: false,
        }),
        0x26 => Some(Code {
            make: 0x0A,
            extended: false,
        }),
        0x27 => Some(Code {
            make: 0x0B,
            extended: false,
        }),
        0x28 => Some(Code {
            make: 0x1C,
            extended: false,
        }),
        0x29 => Some(Code {
            make: 0x01,
            extended: false,
        }),
        0x2A => Some(Code {
            make: 0x0E,
            extended: false,
        }),
        0x2B => Some(Code {
            make: 0x0F,
            extended: false,
        }),
        0x2C => Some(Code {
            make: 0x39,
            extended: false,
        }),
        0x2D => Some(Code {
            make: 0x0C,
            extended: false,
        }),
        0x2E => Some(Code {
            make: 0x0D,
            extended: false,
        }),
        0x2F => Some(Code {
            make: 0x1A,
            extended: false,
        }),
        0x30 => Some(Code {
            make: 0x1B,
            extended: false,
        }),
        0x31 => Some(Code {
            make: 0x2B,
            extended: false,
        }),
        0x33 => Some(Code {
            make: 0x27,
            extended: false,
        }),
        0x34 => Some(Code {
            make: 0x28,
            extended: false,
        }),
        0x35 => Some(Code {
            make: 0x29,
            extended: false,
        }),
        0x36 => Some(Code {
            make: 0x33,
            extended: false,
        }),
        0x37 => Some(Code {
            make: 0x34,
            extended: false,
        }),
        0x38 => Some(Code {
            make: 0x35,
            extended: false,
        }),
        0x3C => Some(Code {
            make: 0x3D,
            extended: false,
        }),
        0x3D => Some(Code {
            make: 0x3E,
            extended: false,
        }),
        0x4A => Some(Code {
            make: 0x47,
            extended: true,
        }),
        0x4B => Some(Code {
            make: 0x49,
            extended: true,
        }),
        0x4C => Some(Code {
            make: 0x53,
            extended: true,
        }),
        0x4D => Some(Code {
            make: 0x4F,
            extended: true,
        }),
        0x4E => Some(Code {
            make: 0x51,
            extended: true,
        }),
        0x4F => Some(Code {
            make: 0x4D,
            extended: true,
        }),
        0x50 => Some(Code {
            make: 0x4B,
            extended: true,
        }),
        0x51 => Some(Code {
            make: 0x50,
            extended: true,
        }),
        0x52 => Some(Code {
            make: 0x48,
            extended: true,
        }),
        0x59 => Some(Code {
            make: 0x4F,
            extended: false,
        }),
        0x5A => Some(Code {
            make: 0x50,
            extended: false,
        }),
        0x5B => Some(Code {
            make: 0x51,
            extended: false,
        }),
        0x5C => Some(Code {
            make: 0x4B,
            extended: false,
        }),
        0x5D => Some(Code {
            make: 0x4C,
            extended: false,
        }),
        0x5E => Some(Code {
            make: 0x4D,
            extended: false,
        }),
        0x5F => Some(Code {
            make: 0x47,
            extended: false,
        }),
        0x60 => Some(Code {
            make: 0x48,
            extended: false,
        }),
        0x61 => Some(Code {
            make: 0x49,
            extended: false,
        }),
        0x62 => Some(Code {
            make: 0x52,
            extended: false,
        }),
        0x63 => Some(Code {
            make: 0x53,
            extended: false,
        }),
        _ => None,
    }
}
pub fn modifier_to_code(v: u8) -> Option<Code> {
    match v {
        0x01 => Some(Code {
            make: 0x1D,
            extended: false,
        }),
        0x02 => Some(Code {
            make: 0x2A,
            extended: false,
        }),
        0x04 => Some(Code {
            make: 0x38,
            extended: false,
        }),
        0x08 => Some(Code {
            make: 0x5B,
            extended: true,
        }),
        0x10 => Some(Code {
            make: 0x1D,
            extended: true,
        }),
        0x20 => Some(Code {
            make: 0x36,
            extended: false,
        }),
        0x40 => Some(Code {
            make: 0x38,
            extended: true,
        }),
        0x80 => Some(Code {
            make: 0x5C,
            extended: true,
        }),
        _ => None,
    }
}
pub fn collect_released(
    previous: &[u8],
    po: usize,
    current: &[u8],
    co: usize,
    output: &mut [u8],
) -> usize {
    let mut count = 0;
    let mut index = (po + 2).min(previous.len());
    while index < previous.len() && count < output.len() {
        let usage = previous[index];
        index += 1;
        if usage <= 1 || current[(co + 2).min(current.len())..].contains(&usage) {
            continue;
        }
        output[count] = usage;
        count += 1;
    }
    count
}
#[unsafe(no_mangle)]
/// # Safety
/// Caller retains readable scan-code input and owns the aligned output record for this synchronous bounded conversion.
pub unsafe extern "C" fn r4os_rust_input_hid_code(v: u8, modifier: bool, o: *mut Code) -> bool {
    let Some(c) = (if modifier {
        modifier_to_code(v)
    } else {
        usage_to_code(v)
    }) else {
        return false;
    };
    unsafe { o.write(c) }
    true
}
#[unsafe(no_mangle)]
/// # Safety
/// Caller retains readable scan-code input and owns the aligned output record for this synchronous bounded conversion.
pub unsafe extern "C" fn r4os_rust_input_hid_released(
    p: *const u8,
    pl: usize,
    po: usize,
    c: *const u8,
    cl: usize,
    co: usize,
    o: *mut u8,
    ol: usize,
) -> usize {
    let previous = if pl == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(p, pl) }
    };
    let current = if cl == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(c, cl) }
    };
    let output = if ol == 0 {
        &mut []
    } else {
        unsafe { core::slice::from_raw_parts_mut(o, ol) }
    };
    collect_released(previous, po, current, co, output)
}

const _: () = {
    assert!(core::mem::size_of::<Code>() == 2);
    assert!(core::mem::align_of::<Code>() == 1);
    assert!(core::mem::offset_of!(Code, make) == 0);
    assert!(core::mem::offset_of!(Code, extended) == 1);
};
