//! Package-local AMD Data Fabric mechanism. CPUPOWER supplies its qualified
//! model and DRAM events. Two clean selectors are borrowed only while observed.
use super::{cpu_capabilities as caps, cpu_topology, monotonic};
use crate::arch::{percpu, safe_msr};
use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicBool, AtomicU32, Ordering},
};

const CONTROL_BASE: u32 = 0xc001_0240;
const ENABLE: u64 = 1 << 22;
use super::dram_accounting::{COUNTER_MASK as MASK, interval_bytes};
const BUSY: i32 = -3;
const UNAVAILABLE: i32 = -2;
const INVALID: i32 = -1;
type Result<T> = core::result::Result<T, i32>;
pub(crate) type Owner = (u32, u64);

#[derive(Clone, Copy)]
struct Profile {
    cookie: u64,
    signature: u32,
    events: [u32; 2],
}
#[derive(Clone, Copy, Default)]
struct Channel {
    control: u32,
    select: u64,
    saved: u64,
    touched: bool,
}
struct State {
    profile: Option<Profile>,
    owner: Option<Owner>,
    package: u32,
    channels: [Channel; 2],
    previous: Option<([u64; 2], u64, u32)>,
    bytes: u64,
    epoch: u64,
    retiring: bool,
}
struct Storage(UnsafeCell<State>);
// Sampling tries the leaf lock once. Teardown may wait for the current
// bounded register operation to finish, so hiding the window returns the loan.
// No allocation, dispatch, wait or callback runs inside the critical section.
unsafe impl Sync for Storage {}
static STATE: Storage = Storage(UnsafeCell::new(State {
    profile: None,
    owner: None,
    package: 0,
    channels: [Channel {
        control: 0,
        select: 0,
        saved: 0,
        touched: false,
    }; 2],
    previous: None,
    bytes: 0,
    epoch: 0,
    retiring: false,
}));
static LOCK: AtomicBool = AtomicBool::new(false);
static OWNER_ID: AtomicU32 = AtomicU32::new(0);

fn with<T>(wait: bool, f: impl FnOnce(&mut State) -> Result<T>) -> Result<T> {
    let _irq = caps::LocalIrq::acquire();
    while LOCK
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        if !wait {
            return Err(BUSY);
        }
        core::hint::spin_loop();
    }
    let result = f(unsafe { &mut *STATE.0.get() });
    LOCK.store(false, Ordering::Release);
    result
}
pub(crate) fn register(cookie: u64, signature: u32, first: u32, second: u32) -> i32 {
    if cookie == 0 || signature == 0 || first == second || (first | second) > 0xffff {
        return INVALID;
    }
    with(false, |s| {
        if s.profile.is_some() {
            return Err(BUSY);
        }
        s.profile = Some(Profile {
            cookie,
            signature,
            events: [first, second],
        });
        Ok(())
    })
    .map_or_else(|e| e, |()| 0)
}
fn local(profile: Profile) -> Result<u32> {
    let cpu = percpu::current_index();
    let snap = caps::snapshot(cpu).map_err(|_| UNAVAILABLE)?;
    let raw = &snap.raw;
    let one = raw.get(1, 0).ok_or(UNAVAILABLE)?;
    if raw.vendor() != *b"AuthenticAMD"
        || one.eax != profile.signature
        || one.ecx & (1 << 31) != 0
        || one.edx & (1 << 5) == 0
        || !raw
            .get(0x80000001, 0)
            .is_some_and(|l| l.ecx & (1 << 24) != 0)
    {
        return Err(UNAVAILABLE);
    }
    package(cpu)
}
fn package(cpu: u32) -> Result<u32> {
    let t = cpu_topology::snapshot(cpu).ok_or(UNAVAILABLE)?;
    if !t.trusted {
        return Err(UNAVAILABLE);
    }
    t.data
        .and_then(|d| d.geometry.ok())
        .map(|g| g.package)
        .ok_or(UNAVAILABLE)
}
fn read(reg: u32) -> Result<u64> {
    unsafe { safe_msr::read(reg) }.map_err(|_| UNAVAILABLE)
}
fn write(reg: u32, value: u64) -> Result<()> {
    unsafe { safe_msr::write(reg, value) }.map_err(|_| UNAVAILABLE)
}

impl State {
    fn restore(&mut self) -> Result<()> {
        if self.owner.is_none() {
            return Ok(());
        }
        self.retiring = true;
        if local(self.profile.ok_or(UNAVAILABLE)?)? != self.package {
            return Err(UNAVAILABLE);
        }
        for channel in &mut self.channels {
            if !channel.touched {
                continue;
            }
            let ctl = read(channel.control)?;
            if ctl == channel.select {
                write(channel.control, 0)?;
            }
            if read(channel.control)? != 0 {
                // An independent collector has replaced our selector. It now owns
                // both registers. Leave its configuration and running count intact.
                channel.touched = false;
                continue;
            }
            write(channel.control + 1, channel.saved)?;
            if read(channel.control + 1)? != channel.saved {
                return Err(UNAVAILABLE);
            }
            channel.touched = false;
        }
        self.owner = None;
        self.previous = None;
        self.retiring = false;
        OWNER_ID.store(0, Ordering::Release);
        Ok(())
    }
    fn open(&mut self, owner: Owner, p: Profile, package_id: u32) -> Result<()> {
        // The qualified configuration has one socket and at most one 8-core CCD.
        let count = cpu_topology::count();
        if count == 0 || count > 16 || !(0..count).all(|c| package(c) == Ok(package_id)) {
            return Err(UNAVAILABLE);
        }
        let mut slots = [Channel::default(); 2];
        let mut n = 0;
        for idx in 0..4 {
            let reg = CONTROL_BASE + 2 * idx;
            if read(reg)? != 0 {
                continue;
            }
            let saved = read(reg + 1)?;
            if saved & !MASK != 0 {
                continue;
            }
            slots[n] = Channel {
                control: reg,
                select: ENABLE | u64::from(p.events[n]),
                saved,
                touched: false,
            };
            n += 1;
            if n == 2 {
                break;
            }
        }
        if n != 2 {
            return Err(BUSY);
        }
        self.epoch = self.epoch.checked_add(1).ok_or(UNAVAILABLE)?;
        self.channels = slots;
        self.owner = Some(owner);
        self.package = package_id;
        self.previous = None;
        self.bytes = 0;
        self.retiring = false;
        OWNER_ID.store(owner.0, Ordering::Release);
        for i in 0..2 {
            let ch = &mut self.channels[i];
            if read(ch.control)? != 0 {
                return Err(BUSY);
            }
            ch.touched = true;
            write(ch.control, ch.select)?;
            if read(ch.control)? != ch.select {
                return Err(UNAVAILABLE);
            }
        }
        Ok(())
    }
    fn sample(&mut self, owner: Owner) -> Result<(u64, u64, u64)> {
        let p = self.profile.ok_or(UNAVAILABLE)?;
        let package_id = local(p)?;
        if let Some(existing) = self.owner {
            if existing != owner {
                return Err(BUSY);
            }
            if self.retiring {
                self.restore()?;
            }
        }
        if self.owner.is_none() {
            if let Err(e) = self.open(owner, p, package_id) {
                if self.owner == Some(owner) {
                    let _ = self.restore();
                }
                return Err(e);
            }
        }
        if self.package != package_id {
            return Err(UNAVAILABLE);
        }
        let mut raw = [0; 2];
        for (i, ch) in self.channels.iter().enumerate() {
            if read(ch.control)? != ch.select {
                self.retiring = true;
                return Err(UNAVAILABLE);
            }
            raw[i] = read(ch.control + 1)? & MASK;
        }
        let stamp = monotonic::capture();
        let ns = monotonic::resolve(stamp).ok_or(UNAVAILABLE)?;
        if let Some((last, then, clock)) = self.previous {
            if clock == stamp.generation
                && let Some(dt) = ns.checked_sub(then)
                && let Some(delta) = interval_bytes(last, raw, dt)
                && let Some(next) = self.bytes.checked_add(delta)
            {
                self.bytes = next;
            } else {
                self.epoch = self.epoch.checked_add(1).ok_or(UNAVAILABLE)?;
                self.bytes = 0;
            }
        }
        self.previous = Some((raw, ns, stamp.generation));
        Ok((self.bytes, ns, self.epoch))
    }
}
pub(crate) fn sample(owner: Owner) -> Result<(u64, u64, u64)> {
    with(false, |s| s.sample(owner))
}
pub(crate) fn release(owner: Owner) -> bool {
    if OWNER_ID.load(Ordering::Acquire) != owner.0 {
        return true;
    }
    with(true, |s| {
        if s.owner == Some(owner) {
            s.restore()
        } else {
            Ok(())
        }
    })
    .is_ok()
}
/// Registry IDs cannot be reused until the retiring instance has been reaped.
pub(crate) fn retire_instance(id: u32) -> bool {
    if OWNER_ID.load(Ordering::Acquire) != id {
        return true;
    }
    with(true, |s| {
        if s.owner.is_some_and(|owner| owner.0 == id) {
            s.restore()
        } else {
            Ok(())
        }
    })
    .is_ok()
}
pub(crate) fn withdraw(cookie: u64) -> bool {
    with(true, |s| {
        if s.profile.is_some_and(|p| p.cookie == cookie) {
            s.restore()?;
            s.profile = None;
        }
        Ok(())
    })
    .is_ok()
}
