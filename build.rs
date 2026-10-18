use std::{collections::BTreeMap, env, fs, path::PathBuf};

fn main() {
    let manifest =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("Kernel manifest directory"));
    let script = manifest.join("linker.ld");
    println!("cargo::rerun-if-changed=linker.ld");
    println!("cargo::rustc-link-arg-bin=r4os=-T{}", script.display());
    println!("cargo::rustc-link-arg-bin=r4os=--build-id=none");
    println!("cargo::rustc-link-arg-bin=r4os=-zmax-page-size=4096");

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
    let cpu_name = values
        .remove("MODULE_CPU_NAME")
        .expect("Module CPU profile");
    assert!(!cpu_name.is_empty() && cpu_name.len() <= 48);
    assert!(
        cpu_name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
    );
    let cpu_hash = values
        .remove("MODULE_CPU_SHA256")
        .expect("Module CPU profile hash");
    assert!(cpu_hash.len() == 64 && cpu_hash.bytes().all(|b| b.is_ascii_hexdigit()));
    let xcr0: u64 = values
        .remove("MODULE_CPU_XCR0_MASK")
        .expect("Module XSTATE mask")
        .parse()
        .expect("XSTATE bitmap");
    assert_eq!(
        xcr0, 7,
        "Review the FPU layout before enabling another module XSTATE profile"
    );
    let cpu_count: usize = values
        .remove("MODULE_CPU_REQUIREMENT_COUNT")
        .expect("Module CPU requirements")
        .parse()
        .expect("CPU requirement count");
    assert!((1..=32).contains(&cpu_count));
    generated.push_str(&format!("pub const MODULE_CPU_NAME:&[u8]=b\"{cpu_name}\";\npub const MODULE_CPU_SHA256:&[u8]=b\"{cpu_hash}\";\npub const MODULE_CPU_XCR0_MASK:u64={xcr0};\npub const MODULE_CPU_REQUIREMENTS:[(&[u8],u32,u32,u8,u8);{cpu_count}]=[\n"));
    for index in 0..cpu_count {
        let value = values
            .remove(&format!("MODULE_CPU_REQUIREMENT_{index}"))
            .expect("Ordered CPU requirement");
        let row: Vec<&str> = value.split(',').collect();
        assert_eq!(row.len(), 5);
        let name = row[0];
        assert!(
            !name.is_empty()
                && name.len() <= 32
                && name
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        );
        let leaf: u32 = row[1].parse().expect("CPUID leaf");
        let subleaf: u32 = row[2].parse().expect("CPUID subleaf");
        let register: u8 = row[3].parse().expect("CPUID register");
        let bit: u8 = row[4].parse().expect("CPUID bit");
        assert!(register < 4 && bit < 32);
        generated.push_str(&format!(
            "(b\"{name}\",{leaf},{subleaf},{register},{bit}),\n"
        ));
    }
    generated.push_str("];\n");
    let platform_count: usize = values
        .remove("PLATFORM_API_COUNT")
        .expect("Contract platform metadata")
        .parse()
        .expect("Platform API count");
    generated.push_str(&format!("pub const PLATFORM_API_COUNT:usize={platform_count};\npub const PLATFORM_APIS:[(&[u8],u32);PLATFORM_API_COUNT]=[\n"));
    for index in 0..platform_count {
        let value = values
            .remove(&format!("PLATFORM_API_{index}"))
            .expect("Ordered Contract platform entry");
        let (name, group) = value.split_once(',').expect("Contract platform name,group");
        assert!(
            !name.is_empty()
                && name
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_'),
            "Contract platform name"
        );
        let group: u32 = group.parse().expect("Contract platform group");
        generated.push_str(&format!("(b\"{name}\",{group}),\n"));
    }
    generated.push_str("];\n");
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
        .map(|v| {
            assert!(
                !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()),
                "Decimal kernel version component"
            );
            assert!(
                v.len() == 1 || !v.starts_with('0'),
                "Canonical kernel version component"
            );
            v.parse().expect("Version integer")
        })
        .collect();
    assert_eq!(parts.len(), 3);
    assert!(version.len() < 16, "Kernel metadata version capacity");
    generated.push_str(&format!(
        "pub const KERNEL_VERSION_TEXT: &[u8] = b\"{version}\";\n"
    ));
    for (name, value) in ["MAJOR", "MINOR", "PATCH"].into_iter().zip(parts) {
        generated.push_str(&format!("pub const KERNEL_{name}: u32 = {value};\n"));
    }
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo output directory"));
    fs::write(output.join("kernel_config.rs"), generated)
        .expect("Write private Rust build constants");
}
