//! Original canonical A-Z request-lane selection; no admission or wait here.
use core::ptr;

const LANE_COUNT: usize = 26;

#[repr(C)]
pub struct Plan {
    pub lanes: [u8; LANE_COUNT],
    pub count: u8,
}

pub(crate) fn drive_code(letter: u8) -> u32 {
    u32::from(if letter.is_ascii_lowercase() {
        letter - (b'a' - b'A')
    } else {
        letter
    })
}

fn lane(letter: u8) -> Option<u8> {
    let code = drive_code(letter);
    if !(u32::from(b'A')..=u32::from(b'Z')).contains(&code) {
        return None;
    }
    Some((code - u32::from(b'A')) as u8)
}

fn all() -> Plan {
    let mut result = Plan {
        lanes: [0; LANE_COUNT],
        count: 0,
    };
    while usize::from(result.count) < LANE_COUNT {
        result.lanes[usize::from(result.count)] = result.count;
        result.count += 1;
    }
    result
}

#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_fs_drive_code(letter: u8) -> u32 {
    drive_code(letter)
}

/// # Safety
/// Output is an exclusive aligned private C plan owned by the synchronous
/// caller. Inputs are copied scalars; no reference or owner spans a wait.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_fs_scope_plan(
    operation: u8,
    first: u8,
    second: u8,
    output: *mut Plan,
) {
    let mut result = Plan {
        lanes: [0; LANE_COUNT],
        count: 0,
    };
    match operation {
        0 => match lane(first) {
            Some(first) => {
                result.lanes[0] = first;
                result.count = 1;
            }
            None => result = all(),
        },
        1 => match (lane(first), lane(second)) {
            (Some(first), Some(second)) if first == second => {
                result.lanes[0] = first;
                result.count = 1;
            }
            (Some(first), Some(second)) => {
                result.lanes[0] = first.min(second);
                result.lanes[1] = first.max(second);
                result.count = 2;
            }
            _ => result = all(),
        },
        2 => result = all(),
        _ => unreachable!(),
    }
    unsafe { ptr::write(output, result) };
}

/// # Safety
/// Plan is an initialized, readable private C record for this synchronous
/// caller; no concurrent mutation is admitted by its request owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_fs_scope_global(plan: *const Plan) -> bool {
    unsafe { usize::from((*plan).count) == LANE_COUNT }
}
