//! Original R4R1 parsing, validation, path/value decisions and byte primitives.
//! Views retain caller-owned immutable storage; this codec owns no catalogue.
use super::registry_format_types::*;
use core::mem::MaybeUninit;
pub(super) fn read16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(
        bytes[offset..offset + 2]
            .try_into()
            .expect("Registry u16 window"),
    )
}
pub(super) fn read32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("Registry u32 window"),
    )
}
pub(super) fn read64(bytes: &[u8], offset: usize) -> u64 {
    u64::from(read32(bytes, offset)) | (u64::from(read32(bytes, offset + 4)) << 32)
}
pub(super) fn write16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}
pub(super) fn write32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
pub(super) fn write64(bytes: &mut [u8], offset: usize, value: u64) {
    write32(bytes, offset, value as u32);
    write32(bytes, offset + 4, (value >> 32) as u32);
}
pub(super) fn kind_valid(value: u16) -> bool {
    (1..=4).contains(&value)
}
pub(super) fn type_valid(value: u16) -> bool {
    (1..=6).contains(&value)
}
pub(super) fn root_name(kind: u16) -> &'static [u8] {
    match kind {
        1 => b"SYSTEM",
        2 => b"SOFTWARE",
        3 => b"DESKTOP",
        4 => b"USER",
        _ => panic!("Invalid registry hive kind"),
    }
}
fn separator(ch: u8) -> bool {
    matches!(ch, b'\\' | b'/')
}
fn space(ch: u8) -> bool {
    matches!(ch, b' ' | b'\t' | b'\r' | b'\n')
}
fn trim_path(mut path: &[u8]) -> &[u8] {
    while path.first().is_some_and(|&b| space(b)) {
        path = &path[1..];
    }
    while path.last().is_some_and(|&b| space(b)) {
        path = &path[..path.len() - 1];
    }
    path
}
fn trim_separators(mut path: &[u8]) -> &[u8] {
    while path.first().is_some_and(|&b| separator(b)) {
        path = &path[1..];
    }
    while path.last().is_some_and(|&b| separator(b)) {
        path = &path[..path.len() - 1];
    }
    path
}
fn root_end(path: &[u8]) -> usize {
    path.iter()
        .position(|&b| separator(b))
        .unwrap_or(path.len())
}
pub(super) fn parse_root(path: &[u8]) -> Option<ParsedRoot> {
    let path = trim_path(path);
    if path.is_empty() {
        return None;
    }
    let split = root_end(path);
    let name = &path[..split];
    let kind = (1..=4).find(|&kind| name.eq_ignore_ascii_case(root_name(kind)))?;
    let rest = if split < path.len() {
        &path[split + 1..]
    } else {
        &path[split..]
    };
    Some(ParsedRoot {
        kind,
        rest: Bytes::from_slice(trim_separators(rest)),
    })
}
pub(super) fn next_component<'a>(rest: &mut &'a [u8]) -> Option<&'a [u8]> {
    *rest = trim_separators(rest);
    if rest.is_empty() {
        return None;
    }
    let split = root_end(rest);
    let component = &rest[..split];
    *rest = if split < rest.len() {
        &rest[split + 1..]
    } else {
        &rest[split..]
    };
    Some(component)
}
pub(super) fn valid_name(name: &[u8], empty: bool) -> bool {
    if name.is_empty() {
        return empty;
    }
    name.len() <= 63
        && name
            .iter()
            .all(|&b| b >= 0x20 && b != 0x7f && !matches!(b, b'\\' | b'/' | b'='))
}
pub(super) fn validate_payload(kind: u16, data: &[u8]) -> Result<()> {
    match kind {
        1 | 5 => {}
        2 => {
            if data.len() != 4 {
                return Err(Error::BadData);
            }
        }
        3 => {
            if data.len() != 8 {
                return Err(Error::BadData);
            }
        }
        4 => {
            if data.len() != 1 || data[0] > 1 {
                return Err(Error::BadData);
            }
        }
        6 => {
            if data.len() < 4 {
                return Err(Error::BadData);
            }
            let count = read32(data, 0);
            let mut offset = 4;
            for _ in 0..count {
                if offset + 2 > data.len() {
                    return Err(Error::BadData);
                }
                let len = usize::from(read16(data, offset));
                offset += 2;
                if offset + len > data.len() {
                    return Err(Error::BadData);
                }
                offset += len;
            }
            if offset != data.len() {
                return Err(Error::BadData);
            }
        }
        _ => panic!("Invalid registry value type"),
    }
    Ok(())
}
fn byte_range(bytes: &[u8], offset: u32, size: u32) -> Result<&[u8]> {
    let start = offset as usize;
    let len = size as usize;
    if len == 0 {
        if start == 0 {
            return Ok(&bytes[..0]);
        }
        if start > bytes.len() {
            return Err(Error::BadRange);
        }
        return Ok(&bytes[start..start]);
    }
    if start > bytes.len() || len > bytes.len() - start {
        return Err(Error::BadRange);
    }
    Ok(&bytes[start..start + len])
}
fn table_range(bytes: &[u8], offset: u32, count: u32, size: usize) -> Result<&[u8]> {
    if count == 0 {
        if offset != 0 {
            return Err(Error::BadRange);
        }
        return Ok(&bytes[..0]);
    }
    let size = u64::from(count) * (size as u64);
    let size = u32::try_from(size).map_err(|_| Error::BadRange)?;
    byte_range(bytes, offset, size)
}
pub(super) fn heap_slice(bytes: &[u8], offset: u32, len: u32) -> &[u8] {
    if len == 0 {
        &bytes[..0]
    } else {
        &bytes[offset as usize..offset as usize + len as usize]
    }
}
pub(super) unsafe fn key_at(view: View, index: u32) -> KeyRecord {
    unsafe {
        let table = view.key_table.slice();
        let o = index as usize * KEY_SIZE;
        KeyRecord {
            parent_index: read32(table, o),
            name_offset: read32(table, o + 4),
            name_len: read16(table, o + 8),
            first_value_index: read32(table, o + 12),
            value_count: read32(table, o + 16),
            first_child_index: read32(table, o + 20),
            child_count: read32(table, o + 24),
        }
    }
}
pub(super) unsafe fn value_at(view: View, index: u32) -> ValueRecord {
    unsafe {
        let table = view.value_table.slice();
        let o = index as usize * VALUE_SIZE;
        let kind = read16(table, o + 10);
        ValueRecord {
            owner_key_index: read32(table, o),
            name_offset: read32(table, o + 4),
            name_len: read16(table, o + 8),
            value_type: if type_valid(kind) { kind } else { 5 },
            data_offset: read32(table, o + 16),
            data_len: read32(table, o + 20),
        }
    }
}
pub(super) unsafe fn key_name<'a>(view: View, key: KeyRecord) -> &'a [u8] {
    unsafe {
        heap_slice(
            view.string_heap.slice(),
            key.name_offset,
            u32::from(key.name_len),
        )
    }
}
pub(super) unsafe fn value_name<'a>(view: View, value: ValueRecord) -> &'a [u8] {
    unsafe {
        heap_slice(
            view.string_heap.slice(),
            value.name_offset,
            u32::from(value.name_len),
        )
    }
}
pub(super) unsafe fn value_data<'a>(view: View, value: ValueRecord) -> &'a [u8] {
    unsafe { heap_slice(view.data_heap.slice(), value.data_offset, value.data_len) }
}
pub(super) unsafe fn find_child(view: View, parent_index: u32, name: &[u8]) -> Option<u32> {
    unsafe {
        if parent_index >= view.header.key_count {
            return None;
        }
        let parent = key_at(view, parent_index);
        if parent.child_count == 0 || parent.first_child_index == INVALID {
            return None;
        }
        for i in parent.first_child_index..parent.first_child_index + parent.child_count {
            let key = key_at(view, i);
            if key.parent_index == parent_index && key_name(view, key).eq_ignore_ascii_case(name) {
                return Some(i);
            }
        }
        None
    }
}
pub(super) unsafe fn find_value(view: View, owner: u32, name: &[u8]) -> Option<u32> {
    unsafe {
        if owner >= view.header.key_count {
            return None;
        }
        let key = key_at(view, owner);
        if key.value_count == 0 || key.first_value_index == INVALID {
            return None;
        }
        for i in key.first_value_index..key.first_value_index + key.value_count {
            let value = value_at(view, i);
            if value.owner_key_index == owner && value_name(view, value).eq_ignore_ascii_case(name)
            {
                return Some(i);
            }
        }
        None
    }
}
pub(super) unsafe fn find_key(view: View, path: &[u8]) -> Option<u32> {
    unsafe {
        let parsed = parse_root(path)?;
        if parsed.kind != view.header.hive_kind {
            return None;
        }
        let mut current = 0;
        let mut rest = parsed.rest.slice();
        while let Some(part) = next_component(&mut rest) {
            current = find_child(view, current, part)?;
        }
        Some(current)
    }
}
pub(super) unsafe fn get_value(view: View, path: &[u8], name: &[u8]) -> Option<Value> {
    unsafe {
        let key = find_key(view, path)?;
        let value = value_at(view, find_value(view, key, name)?);
        Some(Value {
            value_type: value.value_type,
            data: Bytes::from_slice(value_data(view, value)),
        })
    }
}
fn index_in_range(first: u32, count: u32, index: u32) -> bool {
    count != 0 && first != INVALID && index >= first && index - first < count
}
fn validate_index(first: u32, count: u32, total: u32) -> Result<()> {
    if count == 0 {
        if first != INVALID {
            return Err(Error::BadRange);
        }
        return Ok(());
    }
    if first == INVALID || first >= total || count > total - first {
        return Err(Error::BadRange);
    }
    Ok(())
}
fn validate_name(heap: &[u8], offset: u32, len: u16, empty: bool) -> Result<()> {
    if len == 0 {
        if !empty && offset != 0 {
            return Err(Error::BadName);
        }
        return Ok(());
    }
    let name = byte_range(heap, offset, u32::from(len))?;
    if !valid_name(name, empty) {
        return Err(Error::BadName);
    }
    Ok(())
}
pub(super) fn name_hash(owner: u32, name: &[u8]) -> u32 {
    let mut hash = 2166136261u32;
    for shift in [0, 8, 16, 24] {
        hash = (hash ^ ((owner >> shift) & 255)).wrapping_mul(16777619);
    }
    for &b in name {
        hash = (hash ^ u32::from(b.to_ascii_uppercase())).wrapping_mul(16777619);
    }
    hash
}
pub(super) fn bucket_count(count: usize) -> usize {
    let mut capacity = 2;
    while capacity < count * 2 {
        capacity *= 2;
    }
    capacity
}
pub(super) fn init_buckets(storage: &mut [MaybeUninit<u32>], len: usize) -> &mut [u32] {
    for cell in &mut storage[..len] {
        cell.write(INVALID);
    }
    // Only the same initialized bounded prefix becomes a typed slice.
    unsafe { core::slice::from_raw_parts_mut(storage.as_mut_ptr().cast(), len) }
}
unsafe fn record_name<'a>(view: View, index: u32, keys: bool) -> &'a [u8] {
    unsafe {
        if keys {
            key_name(view, key_at(view, index))
        } else {
            value_name(view, value_at(view, index))
        }
    }
}
unsafe fn validate_name_range(view: View, first: u32, count: u32, keys: bool) -> Result<()> {
    unsafe {
        if count < 2 {
            return Ok(());
        }
        let error = if keys {
            Error::DuplicateKey
        } else {
            Error::DuplicateValue
        };
        let mut table = [const { MaybeUninit::<u32>::uninit() }; 1024];
        let buckets = init_buckets(
            &mut table,
            if count <= 512 {
                bucket_count(count as usize)
            } else {
                0
            },
        );
        for i in 0..count {
            let name = record_name(view, first + i, keys);
            if buckets.is_empty() {
                for prior in 0..i {
                    if record_name(view, first + prior, keys).eq_ignore_ascii_case(name) {
                        return Err(error);
                    }
                }
                continue;
            }
            let mut at = name_hash(0, name) as usize & (buckets.len() - 1);
            while buckets[at] != INVALID {
                if record_name(view, first + buckets[at], keys).eq_ignore_ascii_case(name) {
                    return Err(error);
                }
                at = (at + 1) & (buckets.len() - 1);
            }
            buckets[at] = i;
        }
        Ok(())
    }
}
unsafe fn validate(view: View) -> Result<()> {
    unsafe {
        let root = key_at(view, 0);
        if root.parent_index != INVALID || root.name_len != 0 || root.name_offset != 0 {
            return Err(Error::BadRootKey);
        }
        let keys = view.key_table.slice();
        let values = view.value_table.slice();
        let names = view.string_heap.slice();
        for i in 0..view.header.key_count {
            let o = i as usize * KEY_SIZE;
            if read16(keys, o + 10) != 0 || read32(keys, o + 28) != 0 {
                return Err(Error::BadReserved);
            }
            let key = key_at(view, i);
            if key.parent_index != INVALID && key.parent_index >= view.header.key_count {
                return Err(Error::BadKey);
            }
            if i != 0 {
                if key.parent_index == INVALID {
                    return Err(Error::BadKey);
                }
                let parent = key_at(view, key.parent_index);
                if !index_in_range(parent.first_child_index, parent.child_count, i) {
                    return Err(Error::BadKey);
                }
                validate_name(names, key.name_offset, key.name_len, false)?;
            }
            validate_index(
                key.first_child_index,
                key.child_count,
                view.header.key_count,
            )?;
            validate_index(
                key.first_value_index,
                key.value_count,
                view.header.value_count,
            )?;
            for offset in 0..key.child_count {
                if key_at(view, key.first_child_index + offset).parent_index != i {
                    return Err(Error::BadKey);
                }
            }
            for offset in 0..key.value_count {
                if value_at(view, key.first_value_index + offset).owner_key_index != i {
                    return Err(Error::BadValue);
                }
            }
        }
        for i in 0..view.header.value_count {
            let o = i as usize * VALUE_SIZE;
            if !type_valid(read16(values, o + 10)) {
                return Err(Error::BadValue);
            }
            if read32(values, o + 12) != 0 || read64(values, o + 24) != 0 {
                return Err(Error::BadReserved);
            }
            let value = value_at(view, i);
            if value.owner_key_index >= view.header.key_count {
                return Err(Error::BadValue);
            }
            let owner = key_at(view, value.owner_key_index);
            if !index_in_range(owner.first_value_index, owner.value_count, i) {
                return Err(Error::BadValue);
            }
            validate_name(names, value.name_offset, value.name_len, true)?;
            validate_payload(
                value.value_type,
                byte_range(view.data_heap.slice(), value.data_offset, value.data_len)?,
            )?;
        }
        for i in 0..view.header.key_count {
            let key = key_at(view, i);
            validate_name_range(view, key.first_child_index, key.child_count, true)?;
        }
        for i in 0..view.header.key_count {
            let key = key_at(view, i);
            validate_name_range(view, key.first_value_index, key.value_count, false)?;
        }
        Ok(())
    }
}
pub(super) unsafe fn parse(input: Bytes) -> Result<View> {
    unsafe {
        let bytes = input.slice();
        if bytes.len() < HEADER_SIZE {
            return Err(Error::FileTooSmall);
        }
        if &bytes[..4] != b"R4R1" {
            return Err(Error::BadMagic);
        }
        if read16(bytes, 4) != 1 {
            return Err(Error::BadVersion);
        }
        if usize::from(read16(bytes, 6)) != HEADER_SIZE {
            return Err(Error::BadHeaderSize);
        }
        if read16(bytes, 8) != 1 {
            return Err(Error::BadEndian);
        }
        let kind = read16(bytes, 10);
        if !kind_valid(kind) {
            return Err(Error::BadHiveKind);
        }
        if read32(bytes, 12) != 0 {
            return Err(Error::BadFlags);
        }
        if read64(bytes, 16) != bytes.len() as u64 {
            return Err(Error::BadFileSize);
        }
        if read32(bytes, 64) != 0 || read32(bytes, 68) != 0 {
            return Err(Error::BadRange);
        }
        let checksum_type = read32(bytes, 72);
        if checksum_type > 1 {
            return Err(Error::BadChecksumType);
        }
        if bytes[80..96].iter().any(|&b| b != 0) {
            return Err(Error::BadReserved);
        }
        let checksum = read32(bytes, 76);
        if checksum_type == 1 {
            let mut sum = 0u32;
            for (i, &b) in bytes.iter().enumerate() {
                if !(76..80).contains(&i) {
                    sum = sum.wrapping_add(u32::from(b));
                }
            }
            if sum != checksum {
                return Err(Error::BadChecksum);
            }
        }
        let header = Header {
            hive_kind: kind,
            generation: read64(bytes, 24),
            key_table_offset: read32(bytes, 32),
            key_count: read32(bytes, 36),
            value_table_offset: read32(bytes, 40),
            value_count: read32(bytes, 44),
            string_heap_offset: read32(bytes, 48),
            string_heap_size: read32(bytes, 52),
            data_heap_offset: read32(bytes, 56),
            data_heap_size: read32(bytes, 60),
            checksum_type,
            checksum,
        };
        if header.key_count == 0 {
            return Err(Error::BadRootKey);
        }
        let view = View {
            bytes: input,
            header,
            key_table: Bytes::from_slice(table_range(
                bytes,
                header.key_table_offset,
                header.key_count,
                KEY_SIZE,
            )?),
            value_table: Bytes::from_slice(table_range(
                bytes,
                header.value_table_offset,
                header.value_count,
                VALUE_SIZE,
            )?),
            string_heap: Bytes::from_slice(byte_range(
                bytes,
                header.string_heap_offset,
                header.string_heap_size,
            )?),
            data_heap: Bytes::from_slice(byte_range(
                bytes,
                header.data_heap_offset,
                header.data_heap_size,
            )?),
        };
        validate(view)?;
        Ok(view)
    }
}
