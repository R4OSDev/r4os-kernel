use super::{context, notifications, payload_types, waitsets};
use crate::{contract as c, sched::task::task_registry::Core};
use core::ptr;
struct Fixture {
    instance: Box<context::Instance>,
    runtime: Box<payload_types::Runtime>,
    task: Box<Core>,
}
impl Fixture {
    fn new() -> Self {
        crate::LIVE.with(|m| assert!(m.borrow().is_empty()));
        let mut runtime = Box::new(unsafe { core::mem::zeroed::<payload_types::Runtime>() });
        let mut instance = Box::new(context::Instance {
            runtime_payload: ptr::from_mut(runtime.as_mut()).cast(),
            notifications: unsafe { core::mem::zeroed() },
        });
        let mut task = Box::new(Core {
            id: 41,
            generation: 900,
            preempt_disable_depth: 0,
            held_lock_count: 0,
        });
        crate::INSTANCE.with(|i| i.set(instance.as_mut()));
        crate::CORE.with(|i| i.set(task.as_mut()));
        crate::CLOCK.with(|c| {
            c.set(crate::platform::monotonic::Snapshot {
                flags: 1,
                source: 1,
                generation: 1,
                event_backend: 1,
                instant_ns: 10,
                event: crate::platform::monotonic::Event { requested_hz: 1000 },
            })
        });
        Self {
            instance,
            runtime,
            task,
        }
    }
    fn notification(&mut self) -> u64 {
        let mut out = 0;
        assert_eq!(self.notify(0, 0, 0, &mut out), 0);
        out
    }
    fn notify(&mut self, op: u8, handle: u64, value: u64, out: *mut u64) -> i32 {
        unsafe {
            notifications::r4os_rust_notifications_call(
                op,
                &mut self.instance.notifications,
                handle,
                value,
                0,
                out,
            )
        }
    }
    fn set(&self, capacity: u32) -> u64 {
        let mut out = 0;
        assert_eq!(unsafe { waitsets::create(capacity, &mut out) }, 0);
        out
    }
    fn source(handle: u64, tag: u64) -> c::WaitSource {
        c::WaitSource {
            kind: c::waitset_source_notification,
            object: handle,
            user_tag: tag,
            ..Default::default()
        }
    }
    fn enroll(&self, set: u64, source: c::WaitSource) -> u64 {
        let mut out = 0;
        assert_eq!(unsafe { waitsets::enroll(set, &source, &mut out) }, 0);
        out
    }
    fn poll(&self, set: u64, capacity: u32) -> (c::WaitSetBatch, [c::WaitSetEvent; 32]) {
        let mut batch = c::WaitSetBatch::default();
        let mut rows = [c::WaitSetEvent::default(); 32];
        assert_eq!(
            unsafe {
                waitsets::wait(
                    set,
                    &c::WaitSetDeadline::default(),
                    rows.as_mut_ptr(),
                    capacity,
                    &mut batch,
                )
            },
            0
        );
        (batch, rows)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        unsafe {
            assert!(waitsets::cleanup(&mut self.runtime.wait_sets, None));
            assert!(notifications::r4os_rust_notifications_cleanup(
                &mut self.instance.notifications
            ));
            crate::LIVE.with(|m| assert!(m.borrow().is_empty()));
            crate::DEPTH.with(|d| assert_eq!(d.get(), 0));
            crate::PINS.with(|c| assert_eq!(c.get(), 0));
        }
    }
}
// The actual embedded channel and Set algorithms run here. Only admission to
// a canonical IO/Process/Service owner is a seam; native owner proof is separate.
struct Resource(Option<Box<super::wait_sources::Channel>>);
impl Resource {
    fn new() -> Self {
        let mut result = Self(Some(Box::new(super::wait_sources::Channel::EMPTY)));
        crate::SOURCE.with(|p| p.set(result.0.as_mut().unwrap().as_mut()));
        crate::SOURCE_READY.with(|v| v.set(false));
        result
    }
    fn prepare(&self) -> c::WaitSource {
        let mut source = c::WaitSource::default();
        let request = c::WaitSourceRequest {
            kind: c::waitset_source_io,
            object: 17,
            ..Default::default()
        };
        assert_eq!(
            unsafe { super::wait_sources::prepare(&request, &mut source) },
            0
        );
        assert_ne!(source.generation, 0);
        source
    }
    fn publish(&mut self, ready: bool) {
        crate::SOURCE_READY.with(|v| v.set(ready));
        unsafe {
            super::wait_sources::Channel::publish(self.0.as_mut().unwrap().as_mut(), ready);
        }
    }
    fn release(&mut self) {
        if let Some(mut channel) = self.0.take() {
            unsafe {
                super::wait_sources::Channel::close(channel.as_mut());
            }
            crate::SOURCE.with(|p| p.set(ptr::null_mut()));
            drop(channel);
        }
    }
}
impl Drop for Resource {
    fn drop(&mut self) {
        self.release();
    }
}
#[test]
fn embedded_epoch_rejects_prepare_close_replacement_before_enroll() {
    let f = Fixture::new();
    let mut old = Resource::new();
    let prepared = old.prepare();
    old.release();
    let current = Resource::new();
    let replacement = current.prepare();
    assert_ne!(prepared.generation, replacement.generation);
    let set = f.set(1);
    let mut output = 0xfeed;
    assert_eq!(
        unsafe { waitsets::enroll(set, &prepared, &mut output) },
        c::waitset_error_stale
    );
    assert_eq!(output, 0xfeed);
    f.enroll(set, replacement);
}
#[test]
fn embedded_level_readiness_survives_rearm_but_owner_consumption_clears_it() {
    let mut f = Fixture::new();
    let mut resource = Resource::new();
    let source = resource.prepare();
    let set = f.set(2);
    let id = f.enroll(set, source);
    let notification = f.notification();
    f.enroll(set, Fixture::source(notification, 23));
    assert_eq!(f.poll(set, 2).0.count, 0);
    resource.publish(true);
    assert_eq!(f.notify(2, notification, 1, ptr::null_mut()), 0);
    let (batch, rows) = f.poll(set, 1);
    assert_eq!(batch.count, 1);
    assert_ne!(batch.flags & c::waitset_flag_more, 0);
    assert_eq!(rows[0].registration, id);
    assert_eq!(
        unsafe {
            waitsets::rearm(
                set,
                id,
                &c::WaitSource {
                    observed_revision: rows[0].revision,
                    ..source
                },
            )
        },
        0
    );
    assert_eq!(
        f.poll(set, 2).0.count,
        2,
        "Rearm does not consume terminal work"
    );
    resource.publish(false);
    assert_eq!(
        f.poll(set, 2).0.count,
        1,
        "only actual owner consumption clears level"
    );
    assert_eq!(unsafe { waitsets::remove(set, id) }, 0);
    resource.publish(true);
    assert_eq!(f.poll(set, 2).0.count, 1);
}
#[test]
fn embedded_source_close_during_park_frees_backing_before_wait_resumes() {
    let f = Fixture::new();
    let mut resource = Resource::new();
    let source = resource.prepare();
    let set = f.set(1);
    let id = f.enroll(set, source);
    crate::PARK.with(|p| {
        *p.borrow_mut() = Some(Box::new(move |ticks| {
            assert_eq!(ticks, u64::MAX);
            resource.release();
        }))
    });
    let mut batch = c::WaitSetBatch::default();
    let mut row = c::WaitSetEvent::default();
    assert_eq!(
        unsafe {
            waitsets::wait(
                set,
                &c::WaitSetDeadline {
                    kind: 2,
                    ..Default::default()
                },
                &mut row,
                1,
                &mut batch,
            )
        },
        0
    );
    assert_eq!(row.status, c::waitset_event_source_closed as i32);
    assert_eq!(
        unsafe { waitsets::rearm(set, id, &source) },
        c::waitset_error_closed
    );
    assert_eq!(unsafe { waitsets::remove(set, id) }, 0);
}
#[test]
fn embedded_source_subscriber_cap_unlink_and_failed_set_free() {
    let f = Fixture::new();
    let resource = Resource::new();
    let source = resource.prepare();
    let mut sets = [0; 32];
    for set in &mut sets {
        *set = f.set(1);
        f.enroll(*set, source);
    }
    let extra = f.set(1);
    let mut output = 0x1234;
    assert_eq!(
        unsafe { waitsets::enroll(extra, &source, &mut output) },
        c::waitset_error_capacity
    );
    assert_eq!(output, 0x1234);
    crate::FAIL_FREE.with(|v| v.set(true));
    assert_eq!(
        unsafe { waitsets::close(sets[0]) },
        c::waitset_error_release
    );
    f.enroll(extra, source);
    assert_eq!(unsafe { waitsets::close(sets[0]) }, 0);
}
#[test]
fn actual_owner_coalesces_rotates_and_prioritizes_stop() {
    let mut f = Fixture::new();
    let notification = f.notification();
    let set = f.set(4);
    let first = f.enroll(set, Fixture::source(notification, 10));
    let second = f.enroll(set, Fixture::source(notification, 20));
    assert_eq!(f.poll(set, 1).0.outcome, c::waitset_outcome_timeout);
    for _ in 0..3 {
        assert_eq!(f.notify(2, notification, 1, ptr::null_mut()), 0);
    }
    for expected in [first, second, first] {
        let (batch, rows) = f.poll(set, 1);
        assert_eq!(rows[0].registration, expected);
        assert_eq!(rows[0].revision, 3);
        assert_eq!(
            batch.flags & (c::waitset_flag_more | c::waitset_flag_coalesced),
            c::waitset_flag_more | c::waitset_flag_coalesced
        );
    }
    let stop = f.enroll(
        set,
        c::WaitSource {
            flags: c::waitset_source_flag_stop,
            ..Fixture::source(notification, 30)
        },
    );
    let (batch, rows) = f.poll(set, 1);
    assert_eq!(batch.outcome, c::waitset_outcome_cancelled);
    assert_eq!(rows[0].registration, stop);
    assert_eq!(unsafe { waitsets::remove(set, stop) }, 0);
    for id in [first, second] {
        assert_eq!(
            unsafe {
                waitsets::rearm(
                    set,
                    id,
                    &c::WaitSource {
                        observed_revision: 3,
                        ..Fixture::source(notification, if id == first { 10 } else { 20 })
                    },
                )
            },
            0
        );
    }
    assert_eq!(f.poll(set, 4).0.count, 0);
}
#[test]
fn publish_at_actual_enrollment_boundary_wakes_without_renewing_deadline() {
    let mut f = Fixture::new();
    let notification = f.notification();
    let set = f.set(1);
    f.enroll(set, Fixture::source(notification, 0));
    let owner = ptr::from_mut(&mut f.instance.notifications) as usize;
    crate::PARK.with(|p| {
        *p.borrow_mut() = Some(Box::new(move |ticks| {
            assert_eq!(ticks, 2);
            assert_eq!(
                unsafe {
                    notifications::r4os_rust_notifications_call(
                        2,
                        owner as *mut notifications::Owner,
                        notification,
                        1,
                        0,
                        ptr::null_mut(),
                    )
                },
                0
            );
        }))
    });
    let mut batch = c::WaitSetBatch::default();
    let mut rows = [c::WaitSetEvent::default(); 1];
    let deadline = c::WaitSetDeadline {
        kind: 1,
        absolute_ns: 1_000_011,
        last_ns: 10,
        clock_source: 1,
        clock_generation: 1,
        event_backend: 1,
        tick_hz: 1000,
        ..Default::default()
    };
    assert_eq!(
        unsafe { waitsets::wait(set, &deadline, rows.as_mut_ptr(), 1, &mut batch) },
        0
    );
    assert_eq!(batch.count, 1);
    assert_eq!(rows[0].revision, 1);
}
#[test]
fn close_during_park_frees_original_record_before_wait_resumes() {
    let f = Fixture::new();
    let set = f.set(1);
    crate::PARK.with(|p| {
        *p.borrow_mut() = Some(Box::new(move |ticks| {
            assert_eq!(ticks, u64::MAX);
            assert_eq!(unsafe { waitsets::close(set) }, 0);
            crate::LIVE.with(|m| assert!(m.borrow().is_empty()));
        }))
    });
    let mut batch = c::WaitSetBatch::default();
    let mut row = c::WaitSetEvent::default();
    assert_eq!(
        unsafe {
            waitsets::wait(
                set,
                &c::WaitSetDeadline {
                    kind: 2,
                    ..Default::default()
                },
                &mut row,
                1,
                &mut batch,
            )
        },
        0
    );
    assert_eq!(batch.outcome, c::waitset_outcome_closed);
}
#[test]
fn source_close_latches_without_retaining_freed_source_and_release_retries() {
    let mut f = Fixture::new();
    let notification = f.notification();
    let set = f.set(1);
    let id = f.enroll(set, Fixture::source(notification, 8));
    assert_eq!(f.notify(4, notification, 0, ptr::null_mut()), 0);
    let (batch, rows) = f.poll(set, 1);
    assert_eq!(batch.count, 1);
    assert_eq!(rows[0].status, c::waitset_event_source_closed as i32);
    assert_eq!(
        unsafe { waitsets::rearm(set, id, &Fixture::source(notification, 8)) },
        c::waitset_error_closed
    );
    crate::FAIL_FREE.with(|v| v.set(true));
    assert_eq!(unsafe { waitsets::close(set) }, c::waitset_error_release);
    assert_eq!(f.poll(set, 1).0.outcome, c::waitset_outcome_closed);
    assert_eq!(unsafe { waitsets::close(set) }, 0);
}
#[test]
fn exact_retired_creator_unlinks_all_sources_even_when_one_free_fails() {
    let mut f = Fixture::new();
    let n = f.notification();
    for _ in 0..3 {
        let set = f.set(1);
        f.enroll(set, Fixture::source(n, 0));
    }
    f.task.generation += 1;
    let survivor = f.set(1);
    f.enroll(survivor, Fixture::source(n, 0));
    crate::FAIL_FREE.with(|v| v.set(true));
    assert!(!unsafe { waitsets::cleanup(&mut f.runtime.wait_sets, Some((41, 900))) });
    crate::LIVE.with(|m| assert_eq!(m.borrow().len(), 3)); // source, failed closed set, other generation
    assert!(unsafe { waitsets::cleanup(&mut f.runtime.wait_sets, Some((41, 900))) });
    assert_eq!(f.notify(2, n, 1, ptr::null_mut()), 0);
    assert_eq!(f.poll(survivor, 1).0.count, 1);
}
#[test]
fn bounded_source_admission_is_atomic_and_remove_reclaims_its_real_link() {
    let mut f = Fixture::new();
    let n = f.notification();
    let mut sets = Vec::new();
    for _ in 0..32 {
        let set = f.set(1);
        let id = f.enroll(set, Fixture::source(n, 0));
        sets.push((set, id));
    }
    let overflow = f.set(1);
    let mut sentinel = 999;
    assert_eq!(
        unsafe { waitsets::enroll(overflow, &Fixture::source(n, 0), &mut sentinel) },
        c::waitset_error_capacity
    );
    assert_eq!(sentinel, 999);
    assert_eq!(unsafe { waitsets::remove(sets[0].0, sets[0].1) }, 0);
    f.enroll(overflow, Fixture::source(n, 0));
    assert_eq!(f.notify(4, n, 0, ptr::null_mut()), 0);
    for (set, _) in sets.iter().skip(1) {
        assert_eq!(
            f.poll(*set, 1).1[0].status,
            c::waitset_event_source_closed as i32
        );
    }
}
#[test]
fn timers_remain_latched_and_bad_clock_or_output_spans_publish_nothing() {
    let f = Fixture::new();
    let set = f.set(1);
    let timer = c::WaitSource {
        kind: c::waitset_source_timer,
        deadline_ns: 10,
        clock_source: 1,
        clock_generation: 1,
        event_backend: 1,
        tick_hz: 1000,
        ..Default::default()
    };
    let id = f.enroll(set, timer);
    assert_eq!(f.poll(set, 1).0.count, 1);
    assert_eq!(f.poll(set, 1).0.count, 1);
    crate::CLOCK.with(|v| {
        let mut s = v.get();
        s.generation += 1;
        v.set(s);
    });
    assert_eq!(
        f.poll(set, 1).1[0].status,
        c::waitset_event_clock_changed as i32
    );
    crate::CLOCK.with(|v| {
        let mut s = v.get();
        s.generation = 1;
        v.set(s);
    });
    assert_eq!(
        f.poll(set, 1).1[0].status,
        c::waitset_event_clock_changed as i32
    );
    assert_eq!(unsafe { waitsets::rearm(set, id, &timer) }, 0);
    assert_eq!(f.poll(set, 1).1[0].status, c::waitset_event_ready as i32);
    let mut row = c::WaitSetEvent {
        user_tag: 999,
        ..Default::default()
    };
    assert_eq!(
        unsafe {
            waitsets::wait(
                set,
                &c::WaitSetDeadline::default(),
                &mut row,
                1,
                ptr::from_mut(&mut row).cast(),
            )
        },
        c::waitset_error_invalid
    );
    assert_eq!(row.user_tag, 999);
    let mut batch = c::WaitSetBatch {
        count: 999,
        ..Default::default()
    };
    let bad = c::WaitSetDeadline {
        kind: 1,
        absolute_ns: 20,
        last_ns: 10,
        clock_source: 1,
        clock_generation: 99,
        event_backend: 1,
        tick_hz: 1000,
        ..Default::default()
    };
    assert_eq!(
        unsafe { waitsets::wait(set, &bad, &mut row, 1, &mut batch) },
        c::waitset_error_clock_changed
    );
    assert_eq!(batch.count, 999);
    assert_eq!(row.user_tag, 999);
}
