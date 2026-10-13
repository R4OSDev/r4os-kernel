use std::{collections::BTreeMap, env, fs, path::PathBuf};

fn main() {
    println!("cargo::rustc-check-cfg=cfg(r4os_owner_fixture)");
    println!("cargo::rerun-if-env-changed=R4OS_KERNEL_BUILD_CONFIG");
    println!("cargo::rerun-if-changed=VERSION.R4S");
    let path = env::var("R4OS_KERNEL_BUILD_CONFIG").expect(
        "Use Kernel/Build.ps1 (Build.sh/Build.bat) to select the checked kernel configuration",
    );
    println!("cargo::rerun-if-changed={path}");
    let source = fs::read_to_string(&path).expect("Read selected kernel build configuration");
    let mut values = BTreeMap::new();
    for line in source.lines() {
        let (key, value) = line
            .split_once('=')
            .expect("Kernel configuration key=value");
        assert!(
            key.bytes()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == b'_')
        );
        assert!(
            values.insert(key.to_owned(), value.to_owned()).is_none(),
            "Duplicate kernel configuration"
        );
    }
    assert_eq!(values.remove("FORMAT").as_deref(), Some("1"));
    let flags: u64 = values
        .remove("FLAGS")
        .expect("Build flags")
        .parse()
        .expect("u64 build flags");
    let mut generated = format!("pub const BUILD_FLAGS: u64 = {flags};\n");
    for (key, value) in values {
        if value == "true" || value == "false" {
            generated.push_str(&format!("pub const {key}: bool = {value};\n"));
        } else {
            let value: u32 = value.parse().expect("u32 kernel option");
            generated.push_str(&format!("pub const {key}: u32 = {value};\n"));
        }
    }
    let version = fs::read_to_string("VERSION.R4S").expect("Kernel version");
    let version = version
        .trim_start_matches('\u{feff}')
        .trim()
        .strip_prefix("KERNEL_VERSION=")
        .expect("Canonical kernel version");
    let parts: Vec<u32> = version
        .split('.')
        .map(|v| v.parse().expect("Version integer"))
        .collect();
    assert_eq!(parts.len(), 3);
    for (name, value) in ["MAJOR", "MINOR", "PATCH"].into_iter().zip(parts) {
        generated.push_str(&format!("pub const KERNEL_{name}: u32 = {value};\n"));
    }
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo output directory"));
    fs::write(output.join("kernel_config.rs"), generated)
        .expect("Write private Rust build constants");
}
