use super::*;

unsafe fn enroll(queue: &mut QueueCore, node: &mut Node, generation: u64, rank: u8) -> bool {
    let pointer = core::ptr::from_mut(node);
    unsafe { link_ranked(queue, pointer, pointer.cast(), generation, rank) }
}
fn nodes(count: usize) -> Vec<Box<Node>> {
    (0..count).map(|_| Box::new(Node::default())).collect()
}
unsafe fn empty(queue: &mut QueueCore) {
    unsafe {
        let bulk = begin_drain(queue);
        while !queue.head.is_null() {
            pop_front(queue);
        }
        end_drain(queue, bulk);
    }
}

#[test]
fn ranked_minimum_and_ordinary_fifo_are_independent() {
    let mut queue = QueueCore::default();
    let mut values = nodes(128);
    unsafe {
        for (i, node) in values.iter_mut().enumerate() {
            assert!(enroll(&mut queue, node, 1, if i == 127 { 0 } else { 3 }));
        }
        assert_eq!(first_ranked(&mut queue), &mut *values[127] as *mut _);
        assert_eq!(pop_front(&mut queue), &mut *values[0] as *mut _);
        assert_eq!(first_ranked(&mut queue), &mut *values[127] as *mut _);
        assert_eq!(queue.count, 127);
        assert_eq!(queue.priority.visits, 0);
        empty(&mut queue);
    }
}

#[test]
fn reprioritized_equal_ranks_keep_the_original_enrollment_order() {
    let mut queue = QueueCore::default();
    let mut values = nodes(64);
    unsafe {
        for node in &mut values {
            assert!(enroll(&mut queue, node, 1, 2));
        }
        for node in values.iter_mut().rev() {
            set_rank(&mut **node, 0);
        }
        for node in &mut values {
            set_rank(&mut **node, 2);
        }
        for node in &mut values {
            let selected = first_ranked(&mut queue);
            assert_eq!(selected, &mut **node as *mut _);
            assert!(detach(selected));
        }
        assert!(queue.head.is_null() && queue.tail.is_null() && queue.priority.root.is_null());
    }
}

#[test]
fn repeated_promotion_and_demotion_do_not_wait_for_any_inheritance_pump() {
    let mut queue = QueueCore::default();
    let mut values = nodes(12);
    unsafe {
        for (i, node) in values.iter_mut().enumerate() {
            assert!(enroll(&mut queue, node, 1, if i == 10 { 0 } else { 3 }));
        }
        set_rank(&mut *values[0], 0);
        assert_eq!(first_ranked(&mut queue), &mut *values[0] as *mut _);
        set_rank(&mut *values[0], 3);
        assert_eq!(first_ranked(&mut queue), &mut *values[10] as *mut _);
        set_rank(&mut *values[10], 3);
        assert_eq!(first_ranked(&mut queue), &mut *values[0] as *mut _);
        assert_eq!(queue.priority.visits, 0);
        empty(&mut queue);
    }
}

#[test]
fn mixed_link_rank_cancel_and_reuse_match_an_independent_fifo_oracle() {
    let mut queue = QueueCore::default();
    let mut values = nodes(97);
    let mut oracle: Vec<(usize, u8, u64)> = Vec::new();
    let mut sequence = 0u64;
    let mut random = 0x87ab_0091u64;
    unsafe {
        for _ in 0..4096 {
            random = random.wrapping_mul(6364136223846793005).wrapping_add(1);
            let which = (random >> 17) as usize % values.len();
            let rank = ((random >> 8) & 3) as u8;
            let position = oracle.iter().position(|v| v.0 == which);
            match position {
                None => {
                    sequence += 1;
                    assert!(enroll(&mut queue, &mut values[which], sequence, rank));
                    oracle.push((which, rank, sequence));
                }
                Some(position) if random & 1 == 0 => {
                    set_rank(&mut *values[which], rank);
                    oracle[position].1 = rank;
                }
                Some(position) => {
                    assert!(detach(&mut *values[which]));
                    assert!(clear_owner(&mut *values[which]));
                    oracle.remove(position);
                }
            }
            let expected = oracle
                .iter()
                .min_by_key(|v| (v.1, v.2))
                .map_or(null_mut(), |v| &mut *values[v.0] as *mut Node);
            assert_eq!(first_ranked(&mut queue), expected);
            let fifo = oracle
                .first()
                .map_or(null_mut(), |v| &mut *values[v.0] as *mut Node);
            assert_eq!(queue.head, fifo);
            assert_eq!(queue.count, oracle.len());
            assert_eq!(queue.priority.count, oracle.len());
            assert_eq!(queue.priority.visits, 0);
        }
        empty(&mut queue);
    }
}

#[test]
fn closed_queue_drains_exact_nodes_and_reopens_with_a_fresh_generation() {
    let mut queue = QueueCore::default();
    let mut values = nodes(4);
    unsafe {
        for node in &mut values[..3] {
            assert!(enroll(&mut queue, node, 1, 1));
        }
        let generation = queue.generation;
        close(&mut queue);
        assert_ne!(queue.generation, generation);
        assert!(!enroll(&mut queue, &mut values[3], 2, 0));
        assert!(!clear_owner(&mut *values[0]));
        let bulk = begin_drain(&mut queue);
        for node in &mut values[..3] {
            assert_eq!(pop_front(&mut queue), &mut **node as *mut _);
            assert!(clear_owner(&mut **node));
            assert!(!detach(&mut **node));
        }
        end_drain(&mut queue, bulk);
        assert!(reopen(&mut queue));
        assert!(enroll(&mut queue, &mut values[0], 2, 0));
        assert_eq!(values[0].owner_generation, 2);
        assert_eq!(first_ranked(&mut queue), &mut *values[0] as *mut _);
        empty(&mut queue);
    }
}

#[test]
fn full_broadcast_discards_the_index_once_and_keeps_fifo_removal_linear() {
    let mut queue = QueueCore::default();
    let mut values = nodes(257);
    unsafe {
        for (i, node) in values.iter_mut().enumerate() {
            assert!(enroll(&mut queue, node, 1, (i % 4) as u8));
        }
        let bulk = begin_drain(&mut queue);
        assert!(bulk);
        assert!(first_ranked(&mut queue).is_null());
        assert_eq!(queue.priority.count, 0);
        let mut rejected = Node::default();
        assert!(!enroll(&mut queue, &mut rejected, 1, 0));
        for node in &mut values {
            assert_eq!(pop_front(&mut queue), &mut **node as *mut _);
            assert!(!node.indexed);
        }
        end_drain(&mut queue, bulk);
        assert_eq!(queue.count, 0);
        assert_eq!(queue.priority.visits, 0);
        assert!(enroll(&mut queue, &mut rejected, 2, 0));
        empty(&mut queue);
    }
}

#[test]
fn rare_order_wrap_preserves_both_rank_and_fifo_without_losing_a_waiter() {
    let mut queue = QueueCore::default();
    let mut values = nodes(4);
    unsafe {
        for node in &mut values[..3] {
            assert!(enroll(&mut queue, node, 1, 2));
        }
        set_rank(&mut *values[2], 0);
        queue.next_order = u64::MAX;
        assert!(enroll(&mut queue, &mut values[3], 1, 2));
        assert_eq!(first_ranked(&mut queue), &mut *values[2] as *mut _);
        set_rank(&mut *values[2], 2);
        for node in &mut values {
            assert_eq!(first_ranked(&mut queue), &mut **node as *mut _);
            assert!(detach(&mut **node));
        }
        assert_eq!(queue.priority.visits, 0);
    }
}

#[test]
fn bad_rank_generation_or_double_link_cannot_mutate_an_existing_enrollment() {
    let mut first = QueueCore::default();
    let mut second = QueueCore::default();
    let mut node = Node::default();
    unsafe {
        assert!(!enroll(&mut first, &mut node, 1, 4));
        assert!(!enroll(&mut first, &mut node, 0, 1));
        assert!(enroll(&mut first, &mut node, 7, 1));
        assert!(!enroll(&mut second, &mut node, 9, 0));
        assert_eq!(node.owner_generation, 7);
        assert_eq!(node.queue, &mut first as *mut _);
        assert_eq!(first.count, 1);
        assert_eq!(second.count, 0);
        assert!(detach(&mut node));
        assert!(clear_owner(&mut node));
        assert!(enroll(&mut second, &mut node, 9, 0));
        assert_eq!(node.owner_generation, 9);
        empty(&mut second);
    }
}
