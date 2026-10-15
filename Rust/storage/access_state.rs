//! Exact bounded device/mount/use/claim admission state from the original owner.
//! Caller owns resident storage and serializes metadata; no wait, I/O or allocation.
use core::ptr;
pub const MAX_DEVICES: usize = 16;
pub const MAX_MOUNTS: usize = 27;
pub const MAX_USES: usize = 256;
pub const MAX_CLAIMS: usize = 16;
#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct Owner {
    pub task: u32,
    pub task_generation: u64,
    pub program: u32,
    pub program_generation: u64,
}
impl Owner {
    fn valid(self) -> bool {
        self.task != 0
            && self.task_generation != 0
            && (self.program == 0) == (self.program_generation == 0)
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct Reference {
    pub slot: u32,
    pub generation: u64,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Region {
    pub device: Reference,
    pub first: u64,
    pub count: u64,
}
impl Region {
    pub fn overlaps(self, other: Self) -> bool {
        if self.device != other.device || self.count == 0 || other.count == 0 {
            return false;
        }
        if self.first <= other.first {
            other.first - self.first < self.count
        } else {
            self.first - other.first < other.count
        }
    }
    pub fn contains(self, inner: Self) -> bool {
        self.device == inner.device
            && inner.count != 0
            && inner.first >= self.first
            && inner.first - self.first <= self.count
            && inner.count <= self.count - (inner.first - self.first)
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Device {
    pub active: bool,
    pub retiring: bool,
    pub generation: u64,
    pub sectors: u64,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Mount {
    pub active: bool,
    pub generation: u64,
    pub region: Region,
    pub runtime_required: bool,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Use {
    pub id: u64,
    pub owner: Owner,
    pub region: Region,
    pub kind: u8,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Claim {
    pub id: u64,
    pub owner: Owner,
    pub region: Region,
    pub phase: u8,
}
#[repr(C)]
pub struct State {
    pub devices: [Device; MAX_DEVICES],
    pub mounts: [Mount; MAX_MOUNTS],
    pub uses: [Use; MAX_USES],
    pub claims: [Claim; MAX_CLAIMS],
    pub sequence: u64,
    pub topology_generation: u64,
}
#[derive(Clone, Copy)]
#[repr(i32)]
pub enum Error {
    Invalid = 1,
    Stale = 2,
    Busy = 3,
    Protected = 4,
    Capacity = 5,
    WrongOwner = 6,
}
type Result<T> = core::result::Result<T, Error>;
impl State {
    pub const EMPTY: Self = {
        const OWNER: Owner = Owner {
            task: 0,
            task_generation: 0,
            program: 0,
            program_generation: 0,
        };
        const REGION: Region = Region {
            device: Reference {
                slot: 0,
                generation: 0,
            },
            first: 0,
            count: 0,
        };
        Self {
            devices: [Device {
                active: false,
                retiring: false,
                generation: 0,
                sectors: 0,
            }; MAX_DEVICES],
            mounts: [Mount {
                active: false,
                generation: 0,
                region: REGION,
                runtime_required: false,
            }; MAX_MOUNTS],
            uses: [Use {
                id: 0,
                owner: OWNER,
                region: REGION,
                kind: 0,
            }; MAX_USES],
            claims: [Claim {
                id: 0,
                owner: OWNER,
                region: REGION,
                phase: 0,
            }; MAX_CLAIMS],
            sequence: 0,
            topology_generation: 1,
        }
    };
    pub fn changed(&mut self) -> Result<()> {
        if self.topology_generation == u64::MAX {
            return Err(Error::Capacity);
        }
        self.topology_generation += 1;
        Ok(())
    }
    fn next(&mut self) -> Result<u64> {
        if self.sequence == u64::MAX {
            return Err(Error::Capacity);
        }
        self.sequence += 1;
        Ok(self.sequence)
    }
    fn register_device(&mut self, slot: u32, sectors: u64) -> Result<u64> {
        let i = slot as usize;
        if i >= MAX_DEVICES || sectors == 0 {
            return Err(Error::Invalid);
        }
        if self.devices[i].active {
            return Err(Error::Busy);
        }
        let generation = self.next()?;
        self.changed()?;
        self.devices[i] = Device {
            active: true,
            generation,
            sectors,
            ..Device::default()
        };
        Ok(generation)
    }
    fn can_retire(&self, r: Reference) -> Result<usize> {
        let i = r.slot as usize;
        if i >= MAX_DEVICES || r.generation == 0 {
            return Err(Error::Stale);
        }
        let d = self.devices[i];
        if !d.active || d.generation != r.generation {
            return Err(Error::Stale);
        }
        let all = Region {
            device: r,
            first: 0,
            count: d.sectors,
        };
        if self.region_used(all) || self.region_claimed(all) {
            return Err(Error::Busy);
        }
        for mount in &self.mounts {
            if mount.active && mount.region.overlaps(all) {
                return Err(Error::Busy);
            }
        }
        Ok(i)
    }
    fn device_index(&self, r: Reference) -> Result<usize> {
        let i = r.slot as usize;
        if i >= MAX_DEVICES || r.generation == 0 {
            return Err(Error::Stale);
        }
        let d = self.devices[i];
        if !d.active || d.retiring || d.generation != r.generation {
            return Err(Error::Stale);
        }
        Ok(i)
    }
    pub fn validate_region(&self, r: Region) -> Result<()> {
        let d = self.devices[self.device_index(r.device)?];
        if r.count == 0 || r.first >= d.sectors || r.count > d.sectors - r.first {
            return Err(Error::Invalid);
        }
        Ok(())
    }
    fn bind_mount(&mut self, slot: u32, r: Region, required: bool) -> Result<u64> {
        self.validate_region(r)?;
        let i = slot as usize;
        if i >= MAX_MOUNTS {
            return Err(Error::Invalid);
        }
        if self.mounts[i].active {
            return Err(Error::Busy);
        }
        let generation = self.next()?;
        self.changed()?;
        self.mounts[i] = Mount {
            active: true,
            generation,
            region: r,
            runtime_required: required,
        };
        Ok(generation)
    }
    fn mount_index(&self, r: Reference) -> Result<usize> {
        let i = r.slot as usize;
        if i >= MAX_MOUNTS || r.generation == 0 {
            return Err(Error::Stale);
        }
        let m = self.mounts[i];
        if !m.active || m.generation != r.generation {
            return Err(Error::Stale);
        }
        self.validate_region(m.region)?;
        Ok(i)
    }
    fn unbind_mount(&mut self, r: Reference) -> Result<()> {
        let i = self.mount_index(r)?;
        let m = self.mounts[i];
        if m.runtime_required {
            return Err(Error::Protected);
        }
        if self.region_used(m.region) {
            return Err(Error::Busy);
        }
        self.changed()?;
        self.mounts[i].active = false;
        Ok(())
    }
    fn reserve_use(&mut self, region: Region, owner: Owner, kind: u8) -> Result<u64> {
        if !owner.valid() {
            return Err(Error::Invalid);
        }
        for i in 0..MAX_USES {
            if self.uses[i].id != 0 {
                continue;
            }
            let id = self.next()?;
            self.uses[i] = Use {
                id,
                region,
                owner,
                kind,
            };
            return Ok(id);
        }
        Err(Error::Capacity)
    }
    fn begin_use(&mut self, r: Reference, owner: Owner, kind: u8) -> Result<u64> {
        if kind == 2 {
            return Err(Error::Invalid);
        }
        let region = self.mounts[self.mount_index(r)?].region;
        if self.region_claimed(region) {
            return Err(Error::Busy);
        }
        self.reserve_use(region, owner, kind)
    }
    fn begin_raw_read(&mut self, r: Region, owner: Owner) -> Result<u64> {
        self.validate_region(r)?;
        if self.region_claimed(r) {
            return Err(Error::Busy);
        }
        self.reserve_use(r, owner, 2)
    }
    fn end_use(&mut self, id: u64, owner: Owner) -> Result<()> {
        if id == 0 {
            return Err(Error::Stale);
        }
        for use_record in &mut self.uses {
            if use_record.id != id {
                continue;
            }
            if use_record.owner != owner {
                return Err(Error::WrongOwner);
            }
            use_record.id = 0;
            return Ok(());
        }
        Err(Error::Stale)
    }
    fn prepare_claim(&mut self, r: Region, owner: Owner) -> Result<u64> {
        self.validate_region(r)?;
        if !owner.valid() {
            return Err(Error::Invalid);
        }
        for m in &self.mounts {
            if m.active && m.runtime_required && m.region.overlaps(r) {
                return Err(Error::Protected);
            }
        }
        if self.region_used(r) || self.region_claimed(r) {
            return Err(Error::Busy);
        }
        for i in 0..MAX_CLAIMS {
            if self.claims[i].id != 0 {
                continue;
            }
            self.changed()?;
            let id = self.next()?;
            self.claims[i] = Claim {
                id,
                owner,
                region: r,
                phase: 0,
            };
            return Ok(id);
        }
        Err(Error::Capacity)
    }
    fn claim_index(&self, id: u64, owner: Owner) -> Result<usize> {
        if id == 0 {
            return Err(Error::Stale);
        }
        for i in 0..MAX_CLAIMS {
            let c = self.claims[i];
            if c.id != id {
                continue;
            }
            if c.owner != owner {
                return Err(Error::WrongOwner);
            }
            self.validate_region(c.region)?;
            return Ok(i);
        }
        Err(Error::Stale)
    }
    fn activate_claim(&mut self, id: u64, owner: Owner) -> Result<()> {
        let i = self.claim_index(id, owner)?;
        if self.claims[i].phase != 0 {
            return Err(Error::Invalid);
        }
        self.claims[i].phase = 1;
        Ok(())
    }
    fn begin_claim_io(&mut self, id: u64, owner: Owner, r: Region) -> Result<u64> {
        let c = self.claims[self.claim_index(id, owner)?];
        if c.phase != 1 {
            return Err(Error::Busy);
        }
        if !c.region.contains(r) {
            return Err(Error::Invalid);
        }
        self.reserve_use(r, owner, 2)
    }
    fn finish_claim(&mut self, id: u64, owner: Owner) -> Result<()> {
        let i = self.claim_index(id, owner)?;
        self.claims[i].phase = 2;
        if self.region_used(self.claims[i].region) {
            return Err(Error::Busy);
        }
        Ok(())
    }
    fn release_claim(&mut self, id: u64, owner: Owner) -> Result<()> {
        let i = self.claim_index(id, owner)?;
        if self.claims[i].phase != 2 || self.region_used(self.claims[i].region) {
            return Err(Error::Busy);
        }
        let _ = self.changed();
        self.claims[i].id = 0;
        Ok(())
    }
    pub fn region_used(&self, r: Region) -> bool {
        self.uses.iter().any(|u| u.id != 0 && u.region.overlaps(r))
    }
    pub fn region_claimed(&self, r: Region) -> bool {
        self.claims
            .iter()
            .any(|c| c.id != 0 && c.region.overlaps(r))
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Input {
    pub owner: Owner,
    pub region: Region,
    pub reference: Reference,
    pub id: u64,
    pub value: u64,
    pub kind: u8,
}
impl State {
    pub fn apply(&mut self, op: u8, a: Input) -> Result<u64> {
        match op {
            0 => {
                self.changed()?;
                Ok(0)
            }
            1 => self.register_device(a.reference.slot, a.value),
            2 => {
                let i = self.can_retire(a.reference)?;
                if self.devices[i].retiring {
                    return Err(Error::Busy);
                }
                self.devices[i].retiring = true;
                Ok(0)
            }
            3 => {
                let i = a.reference.slot as usize;
                if i < MAX_DEVICES && self.devices[i].generation == a.reference.generation {
                    self.devices[i].retiring = false;
                }
                Ok(0)
            }
            4 => {
                let i = self.can_retire(a.reference)?;
                self.changed()?;
                self.devices[i].active = false;
                Ok(0)
            }
            5 => Ok(self.device_index(a.reference)? as u64),
            6 => {
                self.validate_region(a.region)?;
                Ok(0)
            }
            7 => self.bind_mount(a.reference.slot, a.region, a.value != 0),
            8 => Ok(self.mount_index(a.reference)? as u64),
            9 => {
                self.unbind_mount(a.reference)?;
                Ok(0)
            }
            10 => self.begin_use(a.reference, a.owner, a.kind),
            11 => self.begin_raw_read(a.region, a.owner),
            12 => {
                self.end_use(a.id, a.owner)?;
                Ok(0)
            }
            13 => self.prepare_claim(a.region, a.owner),
            14 => Ok(self.claim_index(a.id, a.owner)? as u64),
            15 => {
                self.activate_claim(a.id, a.owner)?;
                Ok(0)
            }
            16 => self.begin_claim_io(a.id, a.owner, a.region),
            17 => {
                self.finish_claim(a.id, a.owner)?;
                Ok(0)
            }
            18 => {
                self.release_claim(a.id, a.owner)?;
                Ok(0)
            }
            _ => panic!("Invalid storage admission operation"),
        }
    }
}
/// # Safety
/// Both owners are readable initialized C values; no references escape.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_storage_owner_equal(a: *const Owner, b: *const Owner) -> bool {
    unsafe { a.read() == b.read() }
}
/// # Safety
/// Both regions are readable initialized C values; no references escape.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_storage_region(
    op: u8,
    a: *const Region,
    b: *const Region,
) -> bool {
    let a = unsafe { a.read() };
    let b = unsafe { b.read() };
    match op {
        0 => a.overlaps(b),
        1 => a.contains(b),
        _ => unreachable!(),
    }
}
/// # Safety
/// State is initialized, aligned and exclusively admitted by its original
/// caller owner. Input/output are disjoint, resident C records. No I/O/waits.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_storage_access(
    state: *mut State,
    op: u8,
    input: *const Input,
    output: *mut u64,
) -> i32 {
    let a = unsafe { input.read() };
    match unsafe { (&mut *state).apply(op, a) } {
        Ok(value) => {
            unsafe { ptr::write(output, value) };
            0
        }
        Err(e) => e as i32,
    }
}
/// # Safety
/// Passive query: initialized resident state held shared for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_storage_access_region(
    state: *const State,
    claimed: bool,
    r: *const Region,
) -> bool {
    let s = unsafe { &*state };
    let r = unsafe { r.read() };
    if claimed {
        s.region_claimed(r)
    } else {
        s.region_used(r)
    }
}
const _: () = {
    assert!(core::mem::size_of::<Owner>() == 32);
    assert!(core::mem::size_of::<Reference>() == 16);
    assert!(core::mem::size_of::<Region>() == 32);
    assert!(core::mem::size_of::<Device>() == 24);
    assert!(core::mem::size_of::<Mount>() == 56);
    assert!(core::mem::size_of::<Use>() == 80);
    assert!(core::mem::size_of::<Claim>() == 80);
    assert!(core::mem::size_of::<State>() == 23672);
    assert!(core::mem::offset_of!(State, topology_generation) == 23664);
    assert!(core::mem::size_of::<Input>() == 104);
};
