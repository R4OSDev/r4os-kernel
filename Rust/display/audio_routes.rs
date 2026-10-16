//! Bounded copied receiver/HDA route metadata. GPU ELD writes, HDA verbs and
//! audio policy remain with their unchanged external owners.
use super::{equal::same, error::Error};
use crate::contract as a;
use core::mem::size_of;
const CAPACITY: usize = a::gfx_output_catalog_capacity as usize;
#[repr(C)]
pub struct Store {
    pub records: [a::GfxAudioRoute; CAPACITY],
}
impl Store {
    pub fn publish(&mut self, value: &a::GfxAudioRoute) -> Result<(), Error> {
        if value.version != 1
            || (value.size as usize) < size_of::<a::GfxAudioRoute>()
            || value.reserved0 != 0
            || value.source.adapter_id == 0
            || value.source.generation == 0
            || value.source.reserved0 != 0
            || value.revision == 0
            || value.connector_id == 0
            || (value.hda_location >> 24 != 1 && value.hda_location >> 24 != 2)
            || value.hda_location & 0x00ff0000 != 0
            || value.hda_device & 0xffff == 0
            || value.hda_device >> 16 == 0
            || value.head_id >= 32
            || value.device_entry > 3
            || value.state > a::gfx_audio_route_failed
        {
            return Err(Error::Invalid);
        }
        if value.state == a::gfx_audio_route_ready {
            if value.receiver_sequence == 0
                || value.eld_bytes < 20
                || value.eld_bytes as usize > value.eld.len()
                || !value.eld_bytes.is_multiple_of(4)
                || value.port_id != value.eld[8..16]
            {
                return Err(Error::Invalid);
            }
        } else if value.eld_bytes != 0 {
            return Err(Error::Invalid);
        }
        if value.eld[value.eld_bytes as usize..]
            .iter()
            .any(|&byte| byte != 0)
        {
            return Err(Error::Invalid);
        }
        let mut chosen = None;
        let mut vacant = None;
        for (index, record) in self.records.iter().enumerate() {
            if record.source.generation == 0 {
                if vacant.is_none() {
                    vacant = Some(index)
                }
                continue;
            }
            if same(&record.source, &value.source) && record.connector_id == value.connector_id {
                if record.hda_location != value.hda_location
                    || record.hda_device != value.hda_device
                    || record.port_id != value.port_id
                {
                    return Err(Error::Routing);
                }
                if value.revision < record.revision {
                    return Err(Error::Stale);
                }
                if value.revision == record.revision {
                    if same(record, value) {
                        return Ok(());
                    }
                    return Err(Error::Stale);
                }
                chosen = Some(index);
            } else if record.hda_location == value.hda_location
                && record.hda_device == value.hda_device
                && record.port_id == value.port_id
            {
                return Err(Error::Routing);
            }
        }
        self.records[chosen.or(vacant).ok_or(Error::Capacity)?] = *value;
        Ok(())
    }
    pub fn query(&self, location: u32, device: u32, index: u32) -> Option<a::GfxAudioRoute> {
        let mut ordinal = 0;
        for record in &self.records {
            if record.source.generation == 0
                || record.hda_location != location
                || record.hda_device != device
            {
                continue;
            }
            if ordinal == index {
                return Some(*record);
            }
            ordinal += 1;
        }
        None
    }
    pub fn invalidate(&mut self, generation: u64, sequence: u64) {
        for record in &mut self.records {
            if record.source.generation == generation {
                record.receiver_sequence = sequence;
                record.state = a::gfx_audio_route_pending;
                record.eld_bytes = 0;
                record.eld.fill(0);
            }
        }
    }
    pub fn erase(&mut self, generation: u64) -> bool {
        let mut changed = false;
        for record in &mut self.records {
            if record.source.generation == generation {
                *record = a::GfxAudioRoute::default();
                changed = true
            }
        }
        changed
    }
}
/// # Safety
/// Exact initialized route store is serialized by the receiver/display owner.
/// Inputs readable and operation-specific outputs exclusive; no callbacks/I/O.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn r4os_rust_display_audio_routes(
    op: u32,
    store: *mut Store,
    input: *const a::GfxAudioRoute,
    location: u32,
    device: u32,
    index: u32,
    generation: u64,
    sequence: u64,
    out: *mut a::GfxAudioRoute,
) -> i32 {
    match op {
        0 => match unsafe { (&mut *store).publish(&*input) } {
            Ok(()) => 0,
            Err(error) => error as i32,
        },
        1 => {
            let Some(value) = unsafe { &*store }.query(location, device, index) else {
                return 0;
            };
            unsafe { out.write(value) };
            1
        }
        2 => {
            unsafe { &mut *store }.invalidate(generation, sequence);
            0
        }
        3 => i32::from(unsafe { &mut *store }.erase(generation)),
        _ => panic!("receiver audio route operation"),
    }
}
const _: () = {
    assert!(size_of::<Store>() == 11264 && core::mem::align_of::<Store>() == 8);
};
