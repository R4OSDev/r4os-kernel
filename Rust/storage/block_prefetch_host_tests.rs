//! Standalone checks of actual queue selectors and mounted-use state.
//! Exclusive arrays replace IRQ/task admission; no backend model claims I/O.
#![allow(dead_code)]
mod memory {
    pub mod owner {
        pub struct Guard;
        impl Guard {
            pub fn acquire(_: u8) -> Self {
                Self
            }
        }
    }
}
#[path = "access_state.rs"]
mod access;
#[path = "dispatch.rs"]
pub mod dispatch;
#[path = "block_requests.rs"]
mod requests;
mod storage {
    pub use crate::dispatch;
    pub mod block_observation {
        pub struct Subscription;
    }
}
fn slots() -> Box<[requests::Slot; 16]> {
    unsafe {
        {
            let mut b = Box::<[requests::Slot; 16]>::new_uninit();
            core::ptr::write_bytes(b.as_mut_ptr(), 0, 1);
            b.assume_init()
        }
    }
}
#[test]
fn older_speculative_id_cannot_pass_queued_demand_fifo() {
    let mut q = slots();
    for (i, id) in [1, 9, 5].into_iter().enumerate() {
        q[i].state = 1;
        q[i].id = id;
    }
    assert_eq!(
        unsafe { requests::r4os_rust_block_select(q.as_ptr(), 1, 0) },
        0
    );
    assert_eq!(unsafe { requests::select_demand(q.as_ptr(), 1) }, 2);
    q[2].state = 3;
    assert_eq!(unsafe { requests::select_demand(q.as_ptr(), 1) }, 1);
    q[1].state = 3;
    assert_eq!(
        unsafe { requests::select_demand(q.as_ptr(), 1) },
        usize::MAX
    );
}
#[test]
fn active_demand_blocks_speculation_but_its_consumed_completion_does_not() {
    let mut q = slots();
    q[0].state = 1;
    q[0].id = 2;
    q[1].state = 2;
    q[1].id = 3;
    assert!(unsafe { requests::has_demand(q.as_ptr(), 2) });
    q[1].state = 3;
    assert!(!unsafe { requests::has_demand(q.as_ptr(), 2) });
    q[1].state = 0;
    assert!(!unsafe { requests::has_demand(q.as_ptr(), 2) });
    assert!(unsafe { requests::has_demand(q.as_ptr(), 0) });
}
fn mounted() -> (
    Box<access::State>,
    access::Reference,
    access::Region,
    access::Owner,
) {
    let mut s = Box::new(access::State::EMPTY);
    s.devices[0] = access::Device {
        active: true,
        generation: 7,
        sectors: 10000,
        ..access::Device::default()
    };
    let region = access::Region {
        device: access::Reference {
            slot: 0,
            generation: 7,
        },
        first: 64,
        count: 8192,
    };
    let generation = s
        .apply(
            7,
            access::Input {
                reference: access::Reference {
                    slot: 3,
                    generation: 0,
                },
                region,
                ..access::Input::default()
            },
        )
        .ok()
        .expect("valid mounted-use setup");
    (
        s,
        access::Reference {
            slot: 3,
            generation,
        },
        region,
        access::Owner {
            task: 11,
            task_generation: 4,
            ..access::Owner::default()
        },
    )
}
#[test]
fn mounted_use_blocks_unmount_and_claim_until_exact_owner_release() {
    let (mut s, reference, region, owner) = mounted();
    let id = s
        .apply(
            10,
            access::Input {
                reference,
                owner,
                kind: 0,
                ..access::Input::default()
            },
        )
        .ok()
        .expect("valid mounted-use setup");
    assert!(matches!(
        s.apply(
            9,
            access::Input {
                reference,
                ..access::Input::default()
            }
        ),
        Err(access::Error::Busy)
    ));
    assert!(matches!(
        s.apply(
            13,
            access::Input {
                region,
                owner,
                ..access::Input::default()
            }
        ),
        Err(access::Error::Busy)
    ));
    let wrong = access::Owner {
        task_generation: 5,
        ..owner
    };
    assert!(
        s.apply(
            12,
            access::Input {
                id,
                owner: wrong,
                ..access::Input::default()
            }
        )
        .is_err()
    );
    assert!(
        s.apply(
            12,
            access::Input {
                id,
                owner,
                ..access::Input::default()
            }
        )
        .is_ok()
    );
    assert!(
        s.apply(
            9,
            access::Input {
                reference,
                ..access::Input::default()
            }
        )
        .is_ok()
    );
    let fresh = s
        .apply(
            7,
            access::Input {
                reference: access::Reference {
                    slot: 3,
                    generation: 0,
                },
                region,
                ..access::Input::default()
            },
        )
        .ok()
        .expect("valid mounted-use setup");
    assert_ne!(fresh, reference.generation);
    assert!(
        s.apply(
            10,
            access::Input {
                reference,
                owner,
                kind: 0,
                ..access::Input::default()
            }
        )
        .is_err()
    );
}
