use super::*;
struct Source {
    bytes: Vec<u8>,
    calls: usize,
    fail_on: usize,
}
unsafe extern "C" fn source_read(
    context: *mut c_void,
    offset: usize,
    out: *mut u8,
    len: usize,
) -> bool {
    let source = unsafe { &mut *context.cast::<Source>() };
    source.calls += 1;
    assert!(len <= 408);
    if source.calls == source.fail_on {
        unsafe { ptr::write_bytes(out, 0xee, len / 2) };
        return false;
    }
    if offset > source.bytes.len() || len > source.bytes.len() - offset {
        return false;
    }
    unsafe { ptr::copy_nonoverlapping(source.bytes.as_ptr().add(offset), out, len) };
    true
}
unsafe extern "C" fn source_log(_: *mut c_void, _: *const u8, _: usize) {}
fn fixture(count: usize) -> (Source, Header) {
    let mut bytes = vec![0u8; 73 + count * 24];
    for i in 0..count {
        let values = [
            1 + i as u32 % 4,
            2,
            (i * 8) as u32,
            3,
            (i * 16) as u32,
            (-(i as i32) - 7) as u32,
        ];
        for (j, value) in values.into_iter().enumerate() {
            bytes[73 + i * 24 + j * 4..73 + i * 24 + j * 4 + 4]
                .copy_from_slice(&value.to_le_bytes());
        }
    }
    let mut header: Header = unsafe { core::mem::zeroed() };
    header.reloc_off = 73;
    header.reloc_count = count as u32;
    (
        Source {
            bytes,
            calls: 0,
            fail_on: 0,
        },
        header,
    )
}
fn io(source: &mut Source) -> Io {
    Io {
        context: ptr::from_mut(source).cast(),
        read: source_read,
        log: source_log,
        file_size: source.bytes.len(),
    }
}
fn values(r: RelocationRecord) -> [i64; 6] {
    [
        r.kind as i64,
        r.patch_section as i64,
        r.patch_offset as i64,
        r.target_section as i64,
        r.target_offset as i64,
        r.addend as i64,
    ]
}
#[test]
fn grouped_relocations_keep_original_values_and_exact_tail() {
    for count in [0, 1, 16, 17, 18, 169, 170, 171, 1031] {
        let (mut source, header) = fixture(count);
        let io = io(&mut source);
        let mut cursor = RelocationCursor::EMPTY;
        let mut record = MaybeUninit::uninit();
        for i in 0..count {
            assert!(unsafe {
                next_relocation(&io, &header, ptr::addr_of_mut!(cursor), record.as_mut_ptr())
            });
            assert_eq!(
                values(unsafe { record.assume_init() }),
                [
                    1 + i as i64 % 4,
                    2,
                    i as i64 * 8,
                    3,
                    i as i64 * 16,
                    -(i as i64) - 7
                ]
            );
        }
        assert!(!unsafe {
            next_relocation(&io, &header, ptr::addr_of_mut!(cursor), record.as_mut_ptr())
        });
        assert_eq!(source.calls, count.div_ceil(17));
    }
}
#[test]
fn failed_partial_batch_keeps_cursor_and_output_and_retry_reads_fresh_bytes() {
    let (mut source, header) = fixture(18);
    source.fail_on = 2;
    let mut io = io(&mut source);
    let mut cursor = RelocationCursor::EMPTY;
    let mut record = MaybeUninit::uninit();
    for _ in 0..17 {
        assert!(unsafe {
            next_relocation(&io, &header, ptr::addr_of_mut!(cursor), record.as_mut_ptr())
        });
    }
    let previous = values(unsafe { record.assume_init() });
    assert!(!unsafe {
        next_relocation(&io, &header, ptr::addr_of_mut!(cursor), record.as_mut_ptr())
    });
    assert_eq!(cursor.index, 17);
    assert_eq!(values(unsafe { record.assume_init() }), previous);
    assert_eq!(source.calls, 2);
    io.file_size -= 1;
    assert!(!unsafe {
        next_relocation(&io, &header, ptr::addr_of_mut!(cursor), record.as_mut_ptr())
    });
    assert_eq!(source.calls, 2);
    assert_eq!(cursor.index, 17);
    io.file_size += 1;
    assert!(unsafe {
        next_relocation(&io, &header, ptr::addr_of_mut!(cursor), record.as_mut_ptr())
    });
    assert_eq!(
        values(unsafe { record.assume_init() }),
        [2, 2, 136, 3, 272, -24]
    );
    assert_eq!(source.calls, 3);
    assert_eq!(cursor.index, 18);
}
