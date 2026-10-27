use super::*;

fn empty() -> Bookkeeping {
    Bookkeeping {
        states: [FREE; CAPACITY],
        classes: [TASK; CAPACITY],
        queue_prev: [NONE; CAPACITY],
        queue_next: [NONE; CAPACITY],
        final_prev: [NONE; CAPACITY],
        final_next: [NONE; CAPACITY],
        irq_head: NONE,
        irq_tail: NONE,
        task_head: NONE,
        task_tail: NONE,
        deadline_head: NONE,
        deadline_tail: NONE,
        final_head: NONE,
        final_tail: NONE,
        free_mask: u16::MAX,
        free_count: CAPACITY as u32,
        queued_count: 0,
        running_count: 0,
        deadline_running_count: 0,
        completed_count: 0,
        cancelled_count: 0,
        irq_queued_count: 0,
        task_queued_count: 0,
        deadline_queued_count: 0,
        queue_high_water: 0,
        used_high_water: 0,
        retained_high_water: 0,
        deadline_queue_high_water: 0,
        current_irq_burst: 0,
    }
}

#[test]
fn resumed_work_yields_fifo_and_deferred_work_does_not_hide_a_new_job() {
    let mut book = empty();
    let a = book.reserve(TASK).unwrap();
    let b = book.reserve(TASK).unwrap();
    assert_eq!(select_ready(&mut book, |_| true).unwrap().slot, a);
    assert!(resume(&mut book, a));
    assert_eq!(select_ready(&mut book, |s| s != a).unwrap().slot, b);
    assert!(book.complete(b));
    assert!(book.release(b));
    assert!(select_ready(&mut book, |_| false).is_none());
    assert_eq!(
        (book.queued_count, book.running_count, book.completed_count),
        (1, 0, 0)
    );
    let c = book.reserve(TASK).unwrap();
    assert_eq!(select_ready(&mut book, |s| s != a).unwrap().slot, c);
    assert!(book.complete(c));
    assert!(book.release(c));
    assert_eq!(select_ready(&mut book, |_| true).unwrap().slot, a);
    assert!(resume(&mut book, a));
    assert!(book.cancel(a));
    assert_eq!(
        (book.queued_count, book.running_count, book.cancelled_count),
        (0, 0, 1)
    );
    assert!(!resume(&mut book, a));
    assert!(book.release(a));
    assert_eq!(book.free_count, CAPACITY as u32);
}

#[test]
fn irq_and_deadline_sources_cannot_become_continuations() {
    let mut book = empty();
    for source in [IRQ, DEADLINE] {
        let slot = book.reserve(source).unwrap();
        let selected = Selection {
            slot,
            source,
            irq_preferred: false,
            task_fairness: false,
        };
        book.start(&selected);
        assert!(!resume(&mut book, slot));
        assert_eq!(book.states[slot], RUNNING);
        assert!(book.complete(slot));
        assert!(book.release(slot));
    }
    assert_eq!(book.free_count, CAPACITY as u32);
}

#[test]
fn deferred_head_does_not_consume_the_four_irq_fairness_opportunity() {
    let mut book = empty();
    let deferred = book.reserve(TASK).unwrap();
    let normal = book.reserve(TASK).unwrap();
    let irq = book.reserve(IRQ).unwrap();
    book.current_irq_burst = 4;
    assert_eq!(
        select_ready(&mut book, |s| s != deferred).unwrap().slot,
        normal
    );
    assert!(book.complete(normal));
    assert_eq!(
        select_ready(&mut book, |s| s != deferred).unwrap().slot,
        irq
    );
}

#[test]
fn only_deferred_tasks_cannot_hide_an_eligible_irq_after_four_irq_turns() {
    let mut book = empty();
    let deferred = book.reserve(TASK).unwrap();
    let irq = book.reserve(IRQ).unwrap();
    book.current_irq_burst = 4;
    assert_eq!(
        select_ready(&mut book, |s| s != deferred).unwrap().slot,
        irq
    );
    assert_eq!(book.states[deferred], QUEUED);
    assert_eq!(book.states[irq], RUNNING);
}

#[test]
fn fifty_thousand_retained_turns_preserve_owner_and_terminal_accounting() {
    let mut book = empty();
    let mut owner: OwnerBookkeeping = unsafe { core::mem::zeroed() };
    let slot = book.reserve(TASK).unwrap();
    unsafe {
        r4os_rust_work_owner_ledger(&mut owner, 0, TASK);
    }
    for _ in 0..50_000 {
        assert_eq!(select_ready(&mut book, |_| true).unwrap().slot, slot);
        unsafe {
            r4os_rust_work_owner_ledger(&mut owner, 1, TASK);
        }
        assert!(resume(&mut book, slot));
        unsafe {
            r4os_rust_work_owner_ledger(&mut owner, 7, TASK);
        }
        assert_eq!(
            (
                owner.used,
                owner.queued,
                owner.running,
                owner.completed,
                owner.cancelled
            ),
            (1, 1, 0, 0, 0)
        );
        assert_eq!(
            (
                book.free_count,
                book.queued_count,
                book.running_count,
                book.retained()
            ),
            (15, 1, 0, 0)
        );
    }
    assert!(book.cancel(slot));
    unsafe {
        r4os_rust_work_owner_ledger(&mut owner, 2, TASK);
    }
    assert_eq!(owner.retained(), 1);
    assert!(book.release(slot));
    unsafe {
        r4os_rust_work_owner_ledger(&mut owner, 4, CANCELLED);
    }
    assert_eq!((owner.used, owner.retained(), book.free_count), (0, 0, 16));
    assert_eq!(book.reserve(TASK), Some(slot));
}
