comptime {
    if (@import("builtin").is_test) _ = @import("rust_owner_test");
}
// Native USB owner: stateless compatibility facade.
extern fn r4os_rust_usb_link_control(u32, u8, bool) u32;
extern fn r4os_rust_usb_ring_advance(u16, u8, usize, *Advance) void;
extern fn r4os_rust_usb_ring_self_test() bool;
pub const Advance = extern struct {
    next_enqueue: u16,
    next_producer_cycle: u8,
    link_cycle: u8,
    wrapped: bool,
};

const trb_cycle: u32 = 1 << 0;
const trb_chain: u32 = 1 << 4;

// Publishes the two producer-owned Link-TRB bits together. A transfer TD
// which wraps from the last usable ring entry to entry zero must carry CH
// across the Link TRB; otherwise the xHC terminates the TD at the wrap.
pub fn publishedLinkControl(control: u32, cycle: u8, continues_td: bool) u32 {
    return r4os_rust_usb_link_control(control, cycle, continues_td);
}

// Plant die Producer-Transition nach genau einem geschriebenen normalen TRB.
// Bei einem Wrap muss der Link-TRB noch mit dem alten PCS an den Consumer
// uebergeben werden. Erst danach wechselt der Producer auf das PCS des neuen
// Umlaufs.
pub fn afterSubmission(enqueue: u16, producer_cycle: u8, trb_count: usize) Advance {
    var out: Advance = undefined;
    r4os_rust_usb_ring_advance(enqueue, producer_cycle, trb_count, &out);
    return out;
}

pub fn selfTest() bool {
    return r4os_rust_usb_ring_self_test();
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

test "link publication preserves a TD across wrap and clears stale chain" {
    const testing = @import("std").testing;
    const base: u32 = (6 << 10) | (1 << 1) | trb_cycle;
    const continued = publishedLinkControl(base, 1, true);
    try testing.expect((continued & trb_cycle) != 0);
    try testing.expect((continued & trb_chain) != 0);

    const terminal = publishedLinkControl(continued, 0, false);
    try testing.expect((terminal & trb_cycle) == 0);
    try testing.expect((terminal & trb_chain) == 0);
    try testing.expectEqual(base & ~(trb_cycle | trb_chain), terminal);
}

test "ring-cycle self test" {
    const testing = @import("std").testing;
    try testing.expect(selfTest());
}

comptime {
    if (@sizeOf(Advance) != 6 or
        @alignOf(Advance) != 2 or
        @offsetOf(Advance, "next_enqueue") != 0 or
        @offsetOf(Advance, "next_producer_cycle") != 2 or
        @offsetOf(Advance, "link_cycle") != 3 or
        @offsetOf(Advance, "wrapped") != 4) @compileError("USB Advance private C layout");
}
