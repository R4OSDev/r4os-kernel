use std::{env, fs, path::PathBuf};
fn main() {
    println!("cargo::rustc-check-cfg=cfg(r4os_owner_fixture)");
    println!("cargo::rustc-cfg=r4os_owner_fixture");
    println!("cargo::rerun-if-env-changed=R4OS_KERNEL_BUILD_CONFIG");
    let path = env::var("R4OS_KERNEL_BUILD_CONFIG")
        .expect("Use the shared Kernel builder for owner fixtures");
    println!("cargo::rerun-if-changed={path}");
    let source = fs::read_to_string(path).expect("Kernel fixture configuration");
    let mut constants = String::new();
    for key in [
        "ENABLE_METRICS",
        "FORCE_X2APIC",
        "ENABLE_COM1_DEBUG",
        "ENABLE_NET_LOSS_TEST",
    ] {
        let prefix = format!("{key}=");
        let value = source
            .lines()
            .find_map(|line| line.strip_prefix(&prefix))
            .expect("Canonical kernel fixture flag");
        assert!(value == "true" || value == "false");
        constants.push_str(&format!("pub const {key}: bool = {value};\n"));
    }
    println!("cargo::rerun-if-changed=../../VERSION.R4S");
    let version = fs::read_to_string("../../VERSION.R4S").expect("Kernel version");
    let version = version
        .trim_start_matches('\u{feff}')
        .trim()
        .strip_prefix("KERNEL_VERSION=")
        .expect("Canonical Kernel version");
    let parts: Vec<u32> = version
        .split('.')
        .map(|v| v.parse().expect("Version integer"))
        .collect();
    assert_eq!(parts.len(), 3);
    for (name, value) in ["MAJOR", "MINOR", "PATCH"].into_iter().zip(parts) {
        constants.push_str(&format!("pub const KERNEL_{name}: u32 = {value};\n"));
    }
    fs::write(
        PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("owner_config.rs"),
        constants,
    )
    .unwrap();
}
