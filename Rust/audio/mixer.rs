//! Original S16 ring and saturating fixed-point mixer, unchanged budgets.
use core::{ptr, slice};
/// Canonical productive stream capacity; wider private facade inputs retain
/// the original generic path rather than introducing an artificial limit.
pub(crate) const MAX_SOURCES: usize = 8;

#[derive(Clone, Copy)]
struct Cursor(Source);
impl Cursor {
    const EMPTY: Self = Self(Source::EMPTY);
    fn new(mut source: Source) -> Self {
        if source.len >= 2 && source.read_pos >= source.len {
            source.read_pos %= source.len;
        }
        Self(source)
    }
    unsafe fn sample(&mut self) -> i16 {
        let source = &mut self.0;
        if source.len < 2 {
            return 0;
        }
        let low = source.read_pos;
        let high = if low + 1 == source.len { 0 } else { low + 1 };
        source.read_pos = if high + 1 == source.len { 0 } else { high + 1 };
        unsafe { i16::from_le_bytes([source.ring.add(low).read(), source.ring.add(high).read()]) }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Work {
    pub accumulations: u64,
    pub sample_writes: u64,
    pub direct_bytes: u64,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Owner {
    pub instance_id: u32,
    pub generation: u64,
}
impl Owner {
    pub const KERNEL: Self = Self {
        instance_id: 0,
        generation: 0,
    };
    pub fn valid(self) -> bool {
        (self.instance_id == 0 && self.generation == 0)
            || (self.instance_id != 0 && self.generation != 0)
    }
}
pub fn same_owner(a: Owner, b: Owner) -> bool {
    a.instance_id == b.instance_id && a.generation == b.generation
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Source {
    pub ring: *const u8,
    pub len: usize,
    pub read_pos: usize,
    pub volume: u32,
}
impl Source {
    pub const EMPTY: Self = Self {
        ring: ptr::null(),
        len: 0,
        read_pos: 0,
        volume: 0,
    };
}
pub(crate) unsafe fn mix(sources: &[Source], output: &mut [u8], work: *mut Work) {
    unsafe { mix_indexed(sources.len(), |i| sources[i], output, work) }
}
unsafe fn mix_indexed(
    count: usize,
    mut source_at: impl FnMut(usize) -> Source,
    output: &mut [u8],
    work: *mut Work,
) {
    unsafe {
        if count == 1 && source_at(0).volume == 65_536 {
            let source = source_at(0);
            let ring = slice::from_raw_parts(source.ring, source.len);
            let first = output.len().min(ring.len() - source.read_pos);
            output[..first].copy_from_slice(&ring[source.read_pos..source.read_pos + first]);
            let len = output.len();
            output[first..].copy_from_slice(&ring[..len - first]);
            if !work.is_null() {
                (*work).direct_bytes += len as u64;
            }
            return;
        }
        if count <= MAX_SOURCES {
            // Read reservations keep each source stable for this complete
            // call. Only local copied positions advance; actual consumption
            // remains with the original backend acknowledgement owner.
            let mut cursors = [Cursor::EMPTY; MAX_SOURCES];
            for (i, cursor) in cursors[..count].iter_mut().enumerate() {
                *cursor = Cursor::new(source_at(i));
            }
            let mut byte_offset = 0;
            while byte_offset < output.len() {
                let mut total = 0;
                for cursor in &mut cursors[..count] {
                    total = accumulate_sample(total, cursor.sample(), cursor.0.volume, work);
                }
                write_s16(output, byte_offset, clamp_sample(total), work);
                byte_offset += 2;
            }
            return;
        }
        let mut byte_offset = 0;
        while byte_offset < output.len() {
            let mut total = 0;
            for i in 0..count {
                let source = source_at(i);
                total = accumulate_sample(
                    total,
                    ring_read_s16(
                        slice::from_raw_parts(source.ring, source.len),
                        source.read_pos,
                        byte_offset,
                    ),
                    source.volume,
                    work,
                );
            }
            write_s16(output, byte_offset, clamp_sample(total), work);
            byte_offset += 2;
        }
    }
}
pub fn ring_write(ring: &mut [u8], write_pos: &mut usize, used: &mut usize, data: &[u8]) -> usize {
    if ring.is_empty() || *used >= ring.len() || data.is_empty() {
        return 0;
    }
    let count = data.len().min(ring.len() - *used);
    let first = count.min(ring.len() - *write_pos);
    ring[*write_pos..*write_pos + first].copy_from_slice(&data[..first]);
    if first < count {
        ring[..count - first].copy_from_slice(&data[first..count]);
    }
    *write_pos = (*write_pos + count) % ring.len();
    *used += count;
    count
}
pub fn ring_read_s16(ring: &[u8], read_pos: usize, byte_offset: usize) -> i16 {
    if ring.len() < 2 {
        return 0;
    }
    let low = (read_pos + byte_offset) % ring.len();
    let high = (low + 1) % ring.len();
    i16::from_le_bytes([ring[low], ring[high]])
}
pub fn ring_consume(ring_len: usize, read_pos: &mut usize, used: &mut usize, byte_count: usize) {
    if ring_len == 0 || *used == 0 {
        return;
    }
    let count = byte_count.min(*used);
    *read_pos = (*read_pos + count) % ring_len;
    *used -= count;
}
pub(crate) unsafe fn accumulate_sample(
    total: i64,
    sample: i16,
    volume: u32,
    work: *mut Work,
) -> i64 {
    unsafe {
        if !work.is_null() {
            (*work).accumulations += 1;
        }
        total + (sample as i64 * volume as i64) / 65_536
    }
}
pub fn clamp_sample(total: i64) -> i16 {
    if total > 32_767 {
        32_767
    } else if total < -32_768 {
        -32_768
    } else {
        total.try_into().unwrap()
    }
}
pub(crate) unsafe fn write_s16(output: &mut [u8], offset: usize, sample: i16, work: *mut Work) {
    unsafe {
        if !work.is_null() {
            (*work).sample_writes += 1;
        }
        let b = sample.to_le_bytes();
        output[offset] = b[0];
        output[offset + 1] = b[1];
    }
}
#[cfg(test)]
#[unsafe(no_mangle)]
pub static mut r4os_rust_audio_mixer_work: Work = Work {
    accumulations: 0,
    sample_writes: 0,
    direct_bytes: 0,
};
#[cfg(test)]
#[repr(C)]
pub struct Args {
    pub ring: *mut u8,
    pub ring_len: usize,
    pub data: *const u8,
    pub data_len: usize,
    pub position: *mut usize,
    pub used: *mut usize,
    pub offset: usize,
    pub volume: u32,
    pub total: i64,
    pub sample: i16,
    pub sources: *const Source,
    pub count: usize,
    pub work: *mut Work,
    pub reader: Option<unsafe extern "C" fn(*const core::ffi::c_void, usize, *mut Source)>,
}
/// # Safety
/// Private facade records carry the original bounded caller-owned ring spans.
#[cfg(test)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_audio_mixer(op: u32, args: *const Args) -> i64 {
    unsafe {
        let a = &*args;
        match op {
            0 => {
                let out = if a.ring_len == 0 {
                    &mut [][..]
                } else {
                    slice::from_raw_parts_mut(a.ring, a.ring_len)
                };
                mix_indexed(
                    a.count,
                    |i| {
                        if let Some(reader) = a.reader {
                            let mut value = Source::EMPTY;
                            reader(a.sources.cast(), i, &mut value);
                            value
                        } else {
                            *a.sources.add(i)
                        }
                    },
                    out,
                    a.work,
                );
                0
            }
            1 => {
                let ring = if a.ring_len == 0 {
                    &mut [][..]
                } else {
                    slice::from_raw_parts_mut(a.ring, a.ring_len)
                };
                let data = if a.data_len == 0 {
                    &[][..]
                } else {
                    slice::from_raw_parts(a.data, a.data_len)
                };
                ring_write(ring, &mut *a.position, &mut *a.used, data) as i64
            }
            2 => ring_read_s16(
                if a.ring_len == 0 {
                    &[]
                } else {
                    slice::from_raw_parts(a.ring, a.ring_len)
                },
                a.offset,
                a.data_len,
            ) as i64,
            3 => {
                ring_consume(a.ring_len, &mut *a.position, &mut *a.used, a.offset);
                0
            }
            4 => accumulate_sample(a.total, a.sample, a.volume, a.work),
            5 => clamp_sample(a.total) as i64,
            7 => i64::from(
                Owner {
                    instance_id: a.volume,
                    generation: a.total as u64,
                }
                .valid(),
            ),
            8 => i64::from(same_owner(
                (a.ring as *const Owner).read(),
                (a.data as *const Owner).read(),
            )),
            6 => {
                write_s16(
                    slice::from_raw_parts_mut(a.ring, a.ring_len),
                    a.offset,
                    a.sample,
                    a.work,
                );
                0
            }
            _ => 0,
        }
    }
}

const _: () = {
    assert!(size_of::<Owner>() == 16);
    assert!(core::mem::offset_of!(Owner, generation) == 8);
    assert!(size_of::<Source>() == 32);
};

#[cfg(test)]
const _: () = {
    assert!(size_of::<Args>() == 112);
    assert!(core::mem::offset_of!(Args, reader) == 104);
};
