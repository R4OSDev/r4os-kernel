//! Original producer/link transition: publish old PCS before toggling producer.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Advance {
    pub next_enqueue: u16,
    pub next_producer_cycle: u8,
    pub link_cycle: u8,
    pub wrapped: bool,
}
pub fn published_link_control(control: u32, cycle: u8, continues: bool) -> u32 {
    let mut out = control & !((1 << 0) | (1 << 4));
    if cycle & 1 != 0 {
        out |= 1;
    }
    if continues {
        out |= 1 << 4;
    }
    out
}
pub fn after_submission(enqueue: u16, producer: u8, count: usize) -> Advance {
    let pcs = producer & 1;
    let unchanged = Advance {
        next_enqueue: enqueue,
        next_producer_cycle: pcs,
        link_cycle: pcs,
        wrapped: false,
    };
    if !(2..=65_536).contains(&count) {
        return unchanged;
    }
    let next = usize::from(enqueue) + 1;
    let link = count - 1;
    if next < link {
        return Advance {
            next_enqueue: u16::try_from(next).expect("USB ring index"),
            ..unchanged
        };
    }
    if next == link {
        return Advance {
            next_enqueue: 0,
            next_producer_cycle: pcs ^ 1,
            link_cycle: pcs,
            wrapped: true,
        };
    }
    unchanged
}
pub fn self_test() -> bool {
    let (mut enqueue, mut producer, mut link_cycle, mut wraps) = (0u16, 1u8, 0u8, 0u64);
    for _ in 0..800 {
        let submitted = producer;
        let advance = after_submission(enqueue, producer, 256);
        if advance.wrapped {
            if advance.link_cycle != submitted {
                return false;
            }
            link_cycle = advance.link_cycle;
            if advance.next_producer_cycle == submitted {
                return false;
            }
            wraps += 1;
        } else if advance.next_producer_cycle != submitted {
            return false;
        }
        enqueue = advance.next_enqueue;
        producer = advance.next_producer_cycle;
    }
    wraps == 3 && enqueue == 35 && producer == 0 && link_cycle == 1
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_usb_link_control(control: u32, cycle: u8, continues: bool) -> u32 {
    published_link_control(control, cycle, continues)
}
/// # Safety
/// Exclusive fixed-layout transition output; no ring memory is borrowed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_ring_advance(
    enqueue: u16,
    producer: u8,
    count: usize,
    out: *mut Advance,
) {
    unsafe { out.write(after_submission(enqueue, producer, count)) };
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_usb_ring_self_test() -> bool {
    self_test()
}
const _: () = {
    assert!(core::mem::size_of::<Advance>() == 6);
    assert!(core::mem::align_of::<Advance>() == 2);
    assert!(core::mem::offset_of!(Advance, next_enqueue) == 0);
    assert!(core::mem::offset_of!(Advance, next_producer_cycle) == 2);
    assert!(core::mem::offset_of!(Advance, link_cycle) == 3);
    assert!(core::mem::offset_of!(Advance, wrapped) == 4);
};
