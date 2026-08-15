pub const Advance = struct {
    next_enqueue: u16,
    next_producer_cycle: u8,
    link_cycle: u8,
    wrapped: bool,
};

// Plant die Producer-Transition nach genau einem geschriebenen normalen TRB.
// Bei einem Wrap muss der Link-TRB noch mit dem alten PCS an den Consumer
// uebergeben werden. Erst danach wechselt der Producer auf das PCS des neuen
// Umlaufs.
pub fn afterSubmission(enqueue: u16, producer_cycle: u8, trb_count: usize) Advance {
    const pcs = normalizeCycle(producer_cycle);
    if (trb_count < 2 or trb_count > 65_536) {
        return .{
            .next_enqueue = enqueue,
            .next_producer_cycle = pcs,
            .link_cycle = pcs,
            .wrapped = false,
        };
    }

    const next = @as(usize, enqueue) + 1;
    const link_index = trb_count - 1;
    if (next < link_index) {
        return .{
            .next_enqueue = @intCast(next),
            .next_producer_cycle = pcs,
            .link_cycle = pcs,
            .wrapped = false,
        };
    }
    if (next == link_index) {
        return .{
            .next_enqueue = 0,
            .next_producer_cycle = pcs ^ 1,
            .link_cycle = pcs,
            .wrapped = true,
        };
    }

    return .{
        .next_enqueue = enqueue,
        .next_producer_cycle = pcs,
        .link_cycle = pcs,
        .wrapped = false,
    };
}

pub fn selfTest() bool {
    const trb_count: usize = 256;
    var enqueue: u16 = 0;
    var producer_cycle: u8 = 1;
    var link_cycle: u8 = 0;
    var wraps: u64 = 0;
    var submission: usize = 0;

    while (submission < 800) : (submission += 1) {
        const submitted_cycle = producer_cycle;
        const advance = afterSubmission(enqueue, producer_cycle, trb_count);
        if (advance.wrapped) {
            // Der Link gehoert noch zum gerade abgeschlossenen Umlauf.
            if (advance.link_cycle != submitted_cycle) return false;
            link_cycle = advance.link_cycle;
            if (advance.next_producer_cycle == submitted_cycle) return false;
            wraps += 1;
        } else if (advance.next_producer_cycle != submitted_cycle) {
            return false;
        }
        enqueue = advance.next_enqueue;
        producer_cycle = advance.next_producer_cycle;
    }

    return wraps == 3 and
        enqueue == 35 and
        producer_cycle == 0 and
        link_cycle == 1;
}

fn normalizeCycle(cycle: u8) u8 {
    return cycle & 1;
}

test "wrap publishes the old PCS before producer toggle" {
    const testing = @import("std").testing;
    const advance = afterSubmission(254, 1, 256);
    try testing.expect(advance.wrapped);
    try testing.expectEqual(@as(u8, 1), advance.link_cycle);
    try testing.expectEqual(@as(u8, 0), advance.next_producer_cycle);
    try testing.expectEqual(@as(u16, 0), advance.next_enqueue);
}

test "800 submissions cross three complete ring wraps" {
    const testing = @import("std").testing;
    var enqueue: u16 = 0;
    var producer_cycle: u8 = 1;
    var link_cycle: u8 = 0;
    var wraps: u64 = 0;
    var submission: usize = 0;

    while (submission < 800) : (submission += 1) {
        const submitted_cycle = producer_cycle;
        const advance = afterSubmission(enqueue, producer_cycle, 256);
        if (advance.wrapped) {
            try testing.expectEqual(submitted_cycle, advance.link_cycle);
            try testing.expectEqual(submitted_cycle ^ 1, advance.next_producer_cycle);
            link_cycle = advance.link_cycle;
            wraps += 1;
        }
        enqueue = advance.next_enqueue;
        producer_cycle = advance.next_producer_cycle;
    }

    try testing.expectEqual(@as(u64, 3), wraps);
    try testing.expectEqual(@as(u16, 35), enqueue);
    try testing.expectEqual(@as(u8, 0), producer_cycle);
    try testing.expectEqual(@as(u8, 1), link_cycle);
}

test "ring-cycle self test" {
    const testing = @import("std").testing;
    try testing.expect(selfTest());
}
