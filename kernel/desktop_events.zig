// 0.56.28: Gemeinsames Desktop-Aktivitaets-Event. Input- und
// Fenster-Ereignisquellen (PS/2-Keyboard/-Mouse, USB-HID via
// injectScancode/Mouse-Packets, RDP-remoteInputPush, GUI-/Console-
// Revision-Bumps in r4x.zig) signalisieren hier; der Desktop wartet
// mit desktopActivityWait statt in einem festen Sleep-Poll-Raster.
// Sequenznummer + waitUnless-Praedikat machen den Wait Lost-Wakeup-
// sicher (Muster 0.56.19/0.56.22). signal() ist IRQ-tauglich:
// WaitQueue.wakeAll arbeitet unter dem Scheduler-Runtime-Owner und weckt den
// durch seinen intrusiven WaitNode generationstreu gebundenen Task wie der
// Timer-Tick.
const sync = @import("../sched/sync.zig");
const timer = @import("timer.zig");

var activity: sync.WaitQueue = sync.WaitQueue.init();
var signal_seq: u64 = 0;

pub fn signal() void {
    _ = activity.bumpSequenceAndWakeAll(&signal_seq);
}

pub fn sequence() u64 {
    return activity.readSequence(&signal_seq);
}

const WaitCtx = struct {
    last_seq: u64,
};

fn predStillWaitActivity(raw: *anyopaque) bool {
    const c: *WaitCtx = @ptrCast(@alignCast(raw));
    return signal_seq == c.last_seq;
}

// 0.56.40: hz-neutral (250-ms-Slice; bei 100 Hz wie zuvor 25 Ticks).
const WAIT_SLICE_TICKS: u64 = @max(1, (250 * @as(u64, timer.DEFAULT_HZ)) / 1000);

// 1 = neue Aktivitaet seit last_seq, 0 = Timeout. out_seq erhaelt den
// aktuellen Sequenzstand fuer den naechsten Aufruf.
pub fn wait(last_seq: u64, timeout_ticks: u64, out_seq: *u64) i32 {
    var waited: u64 = 0;
    while (true) {
        const current_seq = activity.readSequence(&signal_seq);
        if (current_seq != last_seq) {
            out_seq.* = current_seq;
            return 1;
        }
        if (waited >= timeout_ticks) break;
        const slice = @min(WAIT_SLICE_TICKS, timeout_ticks - waited);
        var ctx = WaitCtx{ .last_seq = last_seq };
        _ = activity.waitUnless(slice, "desk-activity", predStillWaitActivity, &ctx);
        waited += slice;
    }
    out_seq.* = activity.readSequence(&signal_seq);
    return 0;
}
