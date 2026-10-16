//! Caller-owned R4R1 scratch builder, preserving the original bounded hash
//! paths, insertion order, breadth-first flattening and allocation-free fallback.
use super::{registry_format as format, registry_format_types::*};
use core::mem::MaybeUninit;

impl BuildInput {
    unsafe fn key(self, index: u32) -> BuildKey {
        assert!((index as usize) < self.key_capacity);
        unsafe {
            if let Some(read) = self.read_key {
                let mut result = MaybeUninit::uninit();
                read(self.keys, index as usize, result.as_mut_ptr());
                result.assume_init()
            } else {
                self.keys.cast::<BuildKey>().add(index as usize).read()
            }
        }
    }
    unsafe fn put_key(self, index: u32, value: BuildKey) {
        assert!((index as usize) < self.key_capacity);
        unsafe {
            if let Some(write) = self.write_key {
                write(self.keys, index as usize, &value);
            } else {
                self.keys
                    .cast::<BuildKey>()
                    .add(index as usize)
                    .write(value);
            }
        }
    }
    unsafe fn value(self, index: usize) -> BuildValue {
        assert!(index < self.values_len);
        unsafe {
            if let Some(read) = self.read_value {
                let mut result = MaybeUninit::uninit();
                read(self.values, index, result.as_mut_ptr());
                result.assume_init()
            } else {
                self.values.cast::<BuildValue>().add(index).read()
            }
        }
    }
    unsafe fn owner(self, index: usize) -> u32 {
        assert!(index < self.owners_capacity);
        unsafe { self.owners.add(index).read() }
    }
    unsafe fn put_owner(self, index: usize, value: u32) {
        assert!(index < self.owners_capacity);
        unsafe {
            self.owners.add(index).write(value);
        }
    }
    unsafe fn flat(self, index: u32) -> u32 {
        assert!((index as usize) < self.flat_capacity);
        unsafe { self.flat.add(index as usize).read() }
    }
    unsafe fn put_flat(self, index: u32, value: u32) {
        assert!((index as usize) < self.flat_capacity);
        unsafe {
            self.flat.add(index as usize).write(value);
        }
    }
    unsafe fn grouped(self, index: usize) -> u32 {
        assert!(index < self.grouped_capacity);
        unsafe { self.grouped.add(index).read() }
    }
    unsafe fn put_grouped(self, index: usize, value: u32) {
        assert!(index < self.grouped_capacity);
        unsafe {
            self.grouped.add(index).write(value);
        }
    }
}
unsafe fn find_child(
    input: BuildInput,
    buckets: &[u32],
    parent: u32,
    name: &[u8],
    count: u32,
) -> Option<u32> {
    unsafe {
        if !buckets.is_empty() {
            let mut slot = format::name_hash(parent, name) as usize & (buckets.len() - 1);
            while buckets[slot] != INVALID {
                let index = buckets[slot];
                let key = input.key(index);
                if key.parent == parent && key.name.slice().eq_ignore_ascii_case(name) {
                    return Some(index);
                }
                slot = (slot + 1) & (buckets.len() - 1);
            }
            return None;
        }
        for index in 1..count {
            let key = input.key(index);
            if key.parent == parent && key.name.slice().eq_ignore_ascii_case(name) {
                return Some(index);
            }
        }
        None
    }
}
unsafe fn ensure_key(
    input: BuildInput,
    buckets: &mut [u32],
    path: &[u8],
    count: &mut u32,
) -> Result<u32> {
    unsafe {
        let parsed = format::parse_root(path).ok_or(Error::InvalidPath)?;
        if parsed.kind != input.kind {
            return Err(Error::RootMismatch);
        }
        let mut current = 0;
        let mut rest = parsed.rest.slice();
        while let Some(part) = format::next_component(&mut rest) {
            if !format::valid_name(part, false) {
                return Err(Error::BadName);
            }
            if let Some(child) = find_child(input, buckets, current, part, *count) {
                current = child;
                continue;
            }
            if *count as usize >= input.key_capacity {
                return Err(Error::TooManyEntries);
            }
            let next = *count;
            input.put_key(
                next,
                BuildKey {
                    parent: current,
                    name: Bytes::from_slice(part),
                    ..BuildKey::EMPTY
                },
            );
            let mut parent = input.key(current);
            if parent.last_child != INVALID {
                let mut last = input.key(parent.last_child);
                last.next_sibling = next;
                input.put_key(parent.last_child, last);
            } else {
                parent.first_child = next;
            }
            parent.last_child = next;
            parent.child_count += 1;
            input.put_key(current, parent);
            if !buckets.is_empty() {
                let mut slot = format::name_hash(current, part) as usize & (buckets.len() - 1);
                while buckets[slot] != INVALID {
                    slot = (slot + 1) & (buckets.len() - 1);
                }
                buckets[slot] = next;
            }
            *count += 1;
            current = next;
        }
        Ok(current)
    }
}
unsafe fn validate_duplicates(input: BuildInput) -> Result<()> {
    unsafe {
        let mut storage = [const { MaybeUninit::<u32>::uninit() }; 1024];
        let buckets = format::init_buckets(
            &mut storage,
            if input.values_len <= 512 {
                format::bucket_count(input.values_len)
            } else {
                0
            },
        );
        for index in 0..input.values_len {
            let value = input.value(index);
            let name = value.name.slice();
            let owner = input.owner(index);
            if buckets.is_empty() {
                for prior in 0..index {
                    if input.owner(prior) == owner
                        && input.value(prior).name.slice().eq_ignore_ascii_case(name)
                    {
                        return Err(Error::DuplicateValue);
                    }
                }
                continue;
            }
            let mut slot = format::name_hash(owner, name) as usize & (buckets.len() - 1);
            while buckets[slot] != INVALID {
                let prior = buckets[slot] as usize;
                if input.owner(prior) == owner
                    && input.value(prior).name.slice().eq_ignore_ascii_case(name)
                {
                    return Err(Error::DuplicateValue);
                }
                slot = (slot + 1) & (buckets.len() - 1);
            }
            buckets[slot] = u32::try_from(index).expect("Registry value index");
        }
        Ok(())
    }
}
fn append(
    out: &mut [u8],
    bytes: &[u8],
    offset: usize,
    cursor: &mut usize,
    string: bool,
) -> Result<u32> {
    if string && bytes.len() > u16::MAX as usize {
        return Err(Error::TooManyEntries);
    }
    let start = *cursor;
    if offset + start + bytes.len() > out.len() {
        return Err(Error::OutOfMemory);
    }
    if !bytes.is_empty() {
        out[offset + start..offset + start + bytes.len()].copy_from_slice(bytes);
    }
    *cursor += bytes.len();
    Ok(u32::try_from(start).expect("Registry heap cursor"))
}
fn write_key(out: &mut [u8], offset: usize, key: KeyRecord) {
    format::write32(out, offset, key.parent_index);
    format::write32(out, offset + 4, key.name_offset);
    format::write16(out, offset + 8, key.name_len);
    format::write32(out, offset + 12, key.first_value_index);
    format::write32(out, offset + 16, key.value_count);
    format::write32(out, offset + 20, key.first_child_index);
    format::write32(out, offset + 24, key.child_count);
}
fn write_value(out: &mut [u8], offset: usize, value: ValueRecord) {
    format::write32(out, offset, value.owner_key_index);
    format::write32(out, offset + 4, value.name_offset);
    format::write16(out, offset + 8, value.name_len);
    format::write16(out, offset + 10, value.value_type);
    format::write32(out, offset + 16, value.data_offset);
    format::write32(out, offset + 20, value.data_len);
}
pub(super) unsafe fn build(input: BuildInput) -> Result<View> {
    unsafe {
        if input.key_capacity == 0
            || input.flat_capacity == 0
            || input.values_len > input.owners_capacity
        {
            return Err(Error::TooManyEntries);
        }
        // The field adapters only marshal caller-owned records; they never own
        // a second scratch table or perform any builder operation.
        assert_eq!(input.read_key.is_some(), input.write_key.is_some());
        let mut key_storage = [const { MaybeUninit::<u32>::uninit() }; 512];
        let buckets = format::init_buckets(
            &mut key_storage,
            if input.key_capacity <= 256 {
                format::bucket_count(input.key_capacity)
            } else {
                0
            },
        );
        let mut key_count = 1;
        input.put_key(0, BuildKey::EMPTY);
        for index in 0..input.values_len {
            let value = input.value(index);
            format::validate_payload(value.value_type, value.data.slice())?;
            if !format::valid_name(value.name.slice(), true) {
                return Err(Error::BadName);
            }
            let owner = ensure_key(input, buckets, value.key_path.slice(), &mut key_count)?;
            input.put_owner(index, owner);
            let mut key = input.key(owner);
            key.value_count += 1;
            input.put_key(owner, key);
        }
        validate_duplicates(input)?;
        input.put_flat(0, 0);
        let mut root = input.key(0);
        root.flat_index = 0;
        input.put_key(0, root);
        let mut flat_count = 1;
        let mut cursor = 0;
        while cursor < flat_count {
            let mut child = input.key(input.flat(cursor)).first_child;
            while child != INVALID {
                if flat_count as usize >= input.flat_capacity {
                    return Err(Error::TooManyEntries);
                }
                let mut key = input.key(child);
                key.flat_index = flat_count;
                input.put_key(child, key);
                input.put_flat(flat_count, child);
                flat_count += 1;
                child = key.next_sibling;
            }
            cursor += 1;
        }
        let mut string_size = 0;
        let mut data_size = 0;
        let mut first = 0;
        for i in 0..flat_count {
            let index = input.flat(i);
            let mut key = input.key(index);
            string_size += key.name.len;
            key.value_first = first;
            first += key.value_count;
            input.put_key(index, key);
        }
        for i in 0..input.values_len {
            let value = input.value(i);
            string_size += value.name.len;
            data_size += value.data.len;
        }
        let grouped = input.grouped_capacity >= input.values_len;
        if grouped {
            for i in 0..input.values_len {
                let index = input.owner(i);
                let mut key = input.key(index);
                input.put_grouped(
                    (key.value_first + key.value_cursor) as usize,
                    u32::try_from(i).expect("Registry grouped index"),
                );
                key.value_cursor += 1;
                input.put_key(index, key);
            }
        }
        let key_offset = HEADER_SIZE;
        let value_offset = key_offset + key_count as usize * KEY_SIZE;
        let string_offset = value_offset + input.values_len * VALUE_SIZE;
        let data_offset = string_offset + string_size;
        let file_size = data_offset + data_size;
        if file_size > u32::MAX as usize {
            return Err(Error::TooManyEntries);
        }
        if file_size > input.output_capacity {
            return Err(Error::OutOfMemory);
        }
        // Caller grants this output range exclusively, disjoint from values
        // and scratch. Only the original file prefix is cleared.
        let out = core::slice::from_raw_parts_mut(input.output, file_size);
        out.fill(0);
        let mut string_cursor = 0;
        let mut data_cursor = 0;
        let mut flat_value = 0;
        for flat in 0..flat_count {
            let index = input.flat(flat);
            let key = input.key(index);
            let name_offset = append(
                out,
                key.name.slice(),
                string_offset,
                &mut string_cursor,
                true,
            )?;
            write_key(
                out,
                key_offset + flat as usize * KEY_SIZE,
                KeyRecord {
                    parent_index: if key.parent == INVALID {
                        INVALID
                    } else {
                        input.key(key.parent).flat_index
                    },
                    name_offset,
                    name_len: u16::try_from(key.name.len).expect("Registry key name"),
                    first_value_index: if key.value_count == 0 {
                        INVALID
                    } else {
                        flat_value
                    },
                    value_count: key.value_count,
                    first_child_index: if key.first_child == INVALID {
                        INVALID
                    } else {
                        input.key(key.first_child).flat_index
                    },
                    child_count: key.child_count,
                },
            );
            let scan_count = if grouped {
                key.value_count as usize
            } else {
                input.values_len
            };
            for i in 0..scan_count {
                if !grouped && input.owner(i) != index {
                    continue;
                }
                let source = if grouped {
                    input.grouped(key.value_first as usize + i) as usize
                } else {
                    i
                };
                let value = input.value(source);
                let name_offset = append(
                    out,
                    value.name.slice(),
                    string_offset,
                    &mut string_cursor,
                    true,
                )?;
                let data_offset = append(
                    out,
                    value.data.slice(),
                    data_offset,
                    &mut data_cursor,
                    false,
                )?;
                write_value(
                    out,
                    value_offset + flat_value as usize * VALUE_SIZE,
                    ValueRecord {
                        owner_key_index: flat,
                        name_offset,
                        name_len: u16::try_from(value.name.len).expect("Registry value name"),
                        value_type: value.value_type,
                        data_offset,
                        data_len: u32::try_from(value.data.len).expect("Registry value length"),
                    },
                );
                flat_value += 1;
            }
        }
        out[..4].copy_from_slice(b"R4R1");
        format::write16(out, 4, 1);
        format::write16(out, 6, HEADER_SIZE as u16);
        format::write16(out, 8, 1);
        format::write16(out, 10, input.kind);
        format::write64(out, 16, file_size as u64);
        format::write64(out, 24, input.generation);
        format::write32(out, 32, key_offset as u32);
        format::write32(out, 36, key_count);
        format::write32(
            out,
            40,
            if input.values_len == 0 {
                0
            } else {
                value_offset as u32
            },
        );
        format::write32(out, 44, input.values_len as u32);
        format::write32(
            out,
            48,
            if string_size == 0 {
                0
            } else {
                string_offset as u32
            },
        );
        format::write32(out, 52, string_size as u32);
        format::write32(
            out,
            56,
            if data_size == 0 {
                0
            } else {
                data_offset as u32
            },
        );
        format::write32(out, 60, data_size as u32);
        format::parse(Bytes::from_slice(out))
    }
}
