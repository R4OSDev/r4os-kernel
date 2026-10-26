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
