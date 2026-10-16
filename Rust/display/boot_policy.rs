//! Existing exact CONFIG selection and one-boot Limine override.
pub fn config_value(value: &[u8]) -> Option<u32> {
    if value.eq_ignore_ascii_case(b"AUTO") {
        Some(0)
    } else if value.eq_ignore_ascii_case(b"SOFTWARE") {
        Some(1)
    } else {
        None
    }
}
pub fn software_once(command: &[u8]) -> bool {
    command
        .split(|b| b" \t\r\n".contains(b))
        .any(|token| token.eq_ignore_ascii_case(b"r4os.graphics=software"))
}
pub fn effective(config: u32, once: bool) -> u32 {
    assert!(config <= 2, "graphics policy");
    if once { 2 } else { config }
}
/// # Safety
/// The byte span is readable for its supplied extent; operation is private.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_graphics_policy(
    op: u32,
    bytes: *const u8,
    len: usize,
    config: u32,
    once: bool,
) -> i32 {
    match op {
        0 | 1 => {
            let bytes = if len == 0 {
                &[]
            } else {
                unsafe { core::slice::from_raw_parts(bytes, len) }
            };
            if op == 0 {
                config_value(bytes).map_or(-1, |value| value as i32)
            } else {
                i32::from(software_once(bytes))
            }
        }
        2 => effective(config, once) as i32,
        _ => panic!("graphics boot policy operation"),
    }
}
