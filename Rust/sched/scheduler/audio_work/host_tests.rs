use super::*;
fn hint(ticket: u32) -> Hint {
    Hint {
        worker: Identity {
            id: 12,
            generation: 93,
        },
        ticket,
        owner: 4,
        serial_key: 0x4321,
        deadline: 10,
        budget: 2,
        ..Hint::EMPTY
    }
}
#[test]
fn due_and_expired_deadlines_keep_the_original_budget() {
    let h = hint(17);
    assert!(!h.eligible(7));
    assert!(h.eligible(8));
    assert!(h.eligible(100));
    let h = Hint {
        running: true,
        started: 8,
        ..h
    };
    assert!(h.eligible(9));
    assert!(!h.eligible(10));
    assert!(!h.eligible(100));
}
#[test]
fn missing_identity_or_ticket_never_boosts() {
    for h in [
        Hint::EMPTY,
        Hint {
            ticket: 0,
            ..hint(1)
        },
        Hint {
            worker: Identity {
                id: 12,
                generation: 0,
            },
            ..hint(1)
        },
        Hint {
            serial_key: 0,
            ..hint(1)
        },
        Hint {
            budget: 0,
            ..hint(1)
        },
    ] {
        assert!(!h.eligible(100));
    }
}
#[test]
fn flood_of_generations_cannot_replenish_a_reservation() {
    let mut p = Policy::EMPTY;
    p.adopt(2, hint(1));
    p.selected(true, 8, 2, 10);
    for ticket in 2..200 {
        p.adopt(ticket as u64 * 2, hint(ticket));
        p.selected(true, 9, 2, 10);
        assert!(p.protected(9));
        assert!(p.giveback(10));
        assert!(!p.eligible(10));
        assert!(!p.eligible(17));
    }
    assert!(p.eligible(18));
}
#[test]
fn normal_selection_pays_fifo_without_replenishing_the_window() {
    let mut p = Policy::EMPTY;
    p.adopt(2, hint(1));
    p.selected(true, 8, 2, 10);
    assert!(p.giveback(10));
    p.selected(false, 10, 2, 10);
    assert!(!p.giveback(10));
    assert!(!p.eligible(10));
    assert!(p.eligible(18));
}
#[test]
fn cancellation_removes_urgency_but_retains_cooldown() {
    let mut p = Policy::EMPTY;
    p.adopt(2, hint(1));
    p.selected(true, 8, 2, 10);
    p.adopt(
        4,
        Hint {
            worker: hint(1).worker,
            ..Hint::EMPTY
        },
    );
    assert!(!p.eligible(9));
    assert!(!p.protected(9));
    assert!(p.giveback(9));
    p.adopt(6, hint(2));
    assert!(!p.eligible(10));
    assert!(p.eligible(18));
}
#[test]
fn start_keeps_callback_elapsed_time_and_same_ticket_grant() {
    let mut p = Policy::EMPTY;
    p.adopt(2, hint(1));
    p.selected(true, 8, 2, 10);
    p.adopt(
        4,
        Hint {
            running: true,
            started: 8,
            ..hint(1)
        },
    );
    assert!(p.protected(9));
    assert!(!p.protected(10));
    assert!(!p.eligible(18));
}
#[test]
fn complete_record_is_published_and_superseded_without_retry() {
    let m = Mailbox::new();
    assert!(matches!(m.read_after(0), Update::Unchanged));
    unsafe {
        m.publish(hint(1));
    }
    let Update::Changed(seq, h) = m.read_after(0) else {
        panic!();
    };
    assert_eq!(h, hint(1));
    assert!(m.still_current(seq));
    unsafe {
        m.publish(Hint {
            worker: h.worker,
            ..Hint::EMPTY
        });
    }
    assert!(!m.still_current(seq));
    let Update::Changed(_, clear) = m.read_after(seq) else {
        panic!();
    };
    assert_eq!(clear.ticket, 0);
}
#[test]
fn writer_in_progress_is_declined_and_sequence_wrap_is_visible() {
    let m = Mailbox::new();
    m.sequence.store(u64::MAX, Ordering::Relaxed);
    assert!(matches!(m.read_after(0), Update::Busy));
    m.sequence.store(u64::MAX - 1, Ordering::Relaxed);
    unsafe {
        m.publish(hint(7));
    }
    let Update::Changed(0, h) = m.read_after(u64::MAX - 1) else {
        panic!();
    };
    assert_eq!(h, hint(7));
}
#[test]
fn concurrent_publications_never_mix_ticket_and_device_or_budget() {
    use std::sync::Arc;
    let m = Arc::new(Mailbox::new());
    let writer = Arc::clone(&m);
    let t = std::thread::spawn(move || {
        for ticket in 1..=50000 {
            unsafe {
                writer.publish(Hint {
                    serial_key: ticket as u64 * 17,
                    deadline: ticket as u64 * 23,
                    started: ticket as u64 * 31,
                    ..hint(ticket)
                });
            }
        }
    });
    let mut seen = 0;
    while !t.is_finished() || !m.still_current(seen) {
        if let Update::Changed(seq, h) = m.read_after(seen) {
            seen = seq;
            assert_eq!(h.serial_key, h.ticket as u64 * 17);
            assert_eq!(h.deadline, h.ticket as u64 * 23);
            assert_eq!(h.started, h.ticket as u64 * 31);
        }
    }
    t.join().unwrap();
    assert_eq!(seen, 100000);
}

#[test]
fn one_pass_reuses_positive_identity_but_revalidates_each_fresh_decision() {
    let mailbox = Mailbox::new();
    unsafe { mailbox.publish(hint(1)) };
    let mut pass = Pass::new();
    let copies = core::cell::Cell::new(0);
    let lookups = core::cell::Cell::new(0);
    let checks = core::cell::Cell::new(0);
    for now in [8, 8, 9] {
        let value = pass.sample(
            now,
            &mailbox,
            || {
                copies.set(copies.get() + 1);
                Policy::EMPTY
            },
            |key| {
                lookups.set(lookups.get() + 1);
                assert_eq!(key, hint(1).worker);
                Some(71)
            },
            |task, key| {
                checks.set(checks.get() + 1);
                task == 71 && key == hint(1).worker
            },
        );
        assert_eq!(value.worker, Some(71));
        assert!(value.eligible && !value.protected && !value.giveback);
    }
    assert_eq!((copies.get(), lookups.get(), checks.get()), (1, 1, 3));
    assert_eq!(pass.take_update().unwrap().seen, 2);
    assert!(pass.take_update().is_none());
}

#[test]
fn cached_pass_recomputes_budget_expiry_and_selected_fifo_debt() {
    let mailbox = Mailbox::new();
    unsafe { mailbox.publish(hint(1)) };
    let mut pass = Pass::new();
    assert!(
        pass.sample(8, &mailbox, || Policy::EMPTY, |_| Some(1), |_, _| true)
            .eligible
    );
    pass.take_update();
    pass.selected(true, 8, 2, 10);
    let selected = pass.take_update().unwrap();
    assert!(selected.protected(9));
    let active = pass.sample(
        9,
        &mailbox,
        || panic!("no second policy copy"),
        |_| panic!("no second identity lookup"),
        |_, _| true,
    );
    assert!(active.eligible && active.protected && !active.giveback);
    let expired = pass.sample(
        10,
        &mailbox,
        || unreachable!(),
        |_| unreachable!(),
        |_, _| true,
    );
    assert!(!expired.eligible && !expired.protected && expired.giveback);
    pass.selected(false, 10, 2, 10);
    assert!(!pass.take_update().unwrap().giveback(10));
    assert!(
        !pass
            .sample(
                17,
                &mailbox,
                || unreachable!(),
                |_| unreachable!(),
                |_, _| true
            )
            .eligible
    );
    assert!(
        pass.sample(
            18,
            &mailbox,
            || unreachable!(),
            |_| unreachable!(),
            |_, _| true
        )
        .eligible
    );
}

#[test]
fn cancelled_and_replaced_hints_never_replenish_or_escape_as_cached_work() {
    let mailbox = Mailbox::new();
    unsafe { mailbox.publish(hint(1)) };
    let mut pass = Pass::new();
    pass.sample(8, &mailbox, || Policy::EMPTY, |_| Some(1), |_, _| true);
    pass.selected(true, 8, 2, 10);
    unsafe {
        mailbox.publish(Hint {
            ticket: 0,
            ..hint(1)
        })
    };
    let cancelled = pass.sample(
        9,
        &mailbox,
        || unreachable!(),
        |_| unreachable!(),
        |_, _| true,
    );
    assert!(!cancelled.eligible && !cancelled.protected && cancelled.giveback);
    for ticket in 2..16 {
        unsafe { mailbox.publish(hint(ticket)) };
        let value = pass.sample(
            10,
            &mailbox,
            || unreachable!(),
            |_| unreachable!(),
            |_, _| true,
        );
        assert!(!value.eligible && !value.protected && value.giveback);
    }
    let replacement = Hint {
        worker: Identity {
            id: 13,
            generation: 88,
        },
        ..hint(20)
    };
    unsafe { mailbox.publish(replacement) };
    let value = pass.sample(
        10,
        &mailbox,
        || unreachable!(),
        |key| {
            assert_eq!(key, replacement.worker);
            Some(2)
        },
        |task, key| task == 2 && key == replacement.worker,
    );
    assert_eq!(value.worker, Some(2));
    assert!(value.eligible && !value.protected && !value.giveback);
}

#[test]
fn busy_or_concurrently_superseded_publication_declines_without_poisoning_next_read() {
    let mailbox = Mailbox::new();
    unsafe { mailbox.publish(hint(1)) };
    let mut pass = Pass::new();
    pass.sample(8, &mailbox, || Policy::EMPTY, |_| Some(1), |_, _| true);
    mailbox.sequence.fetch_add(1, Ordering::AcqRel);
    assert!(
        pass.sample(
            8,
            &mailbox,
            || unreachable!(),
            |_| unreachable!(),
            |_, _| true
        )
        .worker
        .is_none()
    );
    mailbox.sequence.fetch_add(1, Ordering::Release);
    unsafe { mailbox.publish(hint(2)) };
    assert_eq!(
        pass.sample(
            8,
            &mailbox,
            || unreachable!(),
            |_| unreachable!(),
            |_, _| true
        )
        .worker,
        Some(1)
    );
    let key = Identity {
        id: 44,
        generation: 99,
    };
    let once = core::cell::Cell::new(false);
    let value = pass.sample(
        8,
        &mailbox,
        || unreachable!(),
        |identity| {
            assert_eq!(identity, key);
            Some(2)
        },
        |_, _| {
            if !once.replace(true) {
                unsafe {
                    mailbox.publish(Hint {
                        worker: key,
                        ..hint(3)
                    })
                };
            }
            true
        },
    );
    assert_eq!(value.worker, Some(2));
    assert_eq!(pass.take_update().unwrap().hint.worker, key);
}

#[test]
fn a_second_change_during_refresh_cannot_publish_the_superseded_candidate() {
    let mailbox = Mailbox::new();
    unsafe { mailbox.publish(hint(1)) };
    let mut pass = Pass::new();
    let value = pass.sample(
        8,
        &mailbox,
        || Policy::EMPTY,
        |_| Some(1),
        |_, _| {
            unsafe {
                mailbox.publish(Hint {
                    ticket: 0,
                    ..hint(2)
                })
            };
            true
        },
    );
    assert!(value.worker.is_none());
    let current = pass.sample(8, &mailbox, || unreachable!(), |_| Some(1), |_, _| true);
    assert!(current.worker.is_none() && !current.eligible);
    assert_eq!(pass.take_update().unwrap().hint.ticket, 0);
}

#[test]
fn failed_identity_or_changed_task_generation_is_not_a_permanent_negative() {
    let mailbox = Mailbox::new();
    unsafe { mailbox.publish(hint(1)) };
    let mut pass = Pass::new();
    assert!(
        pass.sample(
            8,
            &mailbox,
            || Policy::EMPTY,
            |_| None::<u32>,
            |_, _| unreachable!()
        )
        .worker
        .is_none()
    );
    assert_eq!(
        pass.sample(8, &mailbox, || unreachable!(), |_| Some(7), |_, _| true)
            .worker,
        Some(7)
    );
    assert!(
        pass.sample(
            8,
            &mailbox,
            || unreachable!(),
            |_| unreachable!(),
            |_, _| false
        )
        .worker
        .is_none()
    );
    assert_eq!(
        pass.sample(
            8,
            &mailbox,
            || unreachable!(),
            |_| Some(8),
            |task, _| task == 8
        )
        .worker,
        Some(8)
    );
}

#[test]
fn separate_admissions_never_reuse_policy_or_descriptor_even_with_the_same_hint() {
    let mailbox = Mailbox::new();
    unsafe { mailbox.publish(hint(1)) };
    let copies = core::cell::Cell::new(0);
    let finds = core::cell::Cell::new(0);
    for task in [3, 4] {
        let mut pass = Pass::new();
        for _ in 0..2 {
            assert_eq!(
                pass.sample(
                    8,
                    &mailbox,
                    || {
                        copies.set(copies.get() + 1);
                        Policy::EMPTY
                    },
                    |_| {
                        finds.set(finds.get() + 1);
                        Some(task)
                    },
                    |_, _| true
                )
                .worker,
                Some(task)
            );
        }
    }
    assert_eq!((copies.get(), finds.get()), (2, 2));
}
