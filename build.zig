const std = @import("std");

pub fn build(b: *std.Build) void {
    // The R4OS kernel is pinned to ReleaseSafe. Debug overflows the Zig 0.16
    // ELF linker, while the kernel must never inherit a caller optimization.
    const optimize: std.builtin.OptimizeMode = .ReleaseSafe;
    const enable_exception_test = b.option(
        bool,
        "exception-test",
        "Trigger an invalid-opcode exception after loading the IDT",
    ) orelse false;
    const enable_page_fault_test = b.option(
        bool,
        "page-fault-test",
        "Trigger a page fault after paging helpers are initialized",
    ) orelse false;
    const enable_general_protection_test = b.option(
        bool,
        "general-protection-test",
        "Trigger a general-protection fault after loading the IDT",
    ) orelse false;
    const enable_crash_screen_test = b.option(
        bool,
        "crash-screen-test",
        "Render the crash screen after the boot framebuffer is initialized",
    ) orelse false;
    const enable_kernel_fatal_test = b.option(
        bool,
        "kernel-fatal-test",
        "Trigger a central KernelFatal crash after the boot framebuffer is initialized",
    ) orelse false;
    const enable_zig_panic_test = b.option(
        bool,
        "zig-panic-test",
        "Trigger a Zig panic through the central crash path after the boot framebuffer is initialized",
    ) orelse false;
    const enable_com1_debug = b.option(
        bool,
        "com1-debug",
        "Enable COM1 serial output for early kernel debug logs",
    ) orelse true;
    const enable_metrics = b.option(
        bool,
        "metrics",
        "Enable lock/starvation metrics scans",
    ) orelse true;
    const force_x2apic = b.option(
        bool,
        "force-x2apic",
        "Force-enable x2APIC MSR mode when supported",
    ) orelse false;
    const stack_guard_test = b.option(
        bool,
        "stack-guard-test",
        "Spawn a kernel thread that intentionally overflows its stack",
    ) orelse false;
    const boot_selftests = b.option(
        bool,
        "boot-selftests",
        "Run invasive heap, page-table, scheduler, and sync selftests during boot",
    ) orelse false;
    const net_loss_test = b.option(
        bool,
        "net-loss-test",
        "Drop every Nth received network frame in test builds",
    ) orelse false;
    const strip_kernel = b.option(
        bool,
        "strip-kernel",
        "Strip host-only debug and symbol sections from the installed kernel ELF",
    ) orelse true;

    const target = b.resolveTargetQuery(.{
        .cpu_arch = .x86_64,
        .os_tag = .freestanding,
        .abi = .none,
        .cpu_features_sub = std.Target.x86.featureSet(&.{
            .mmx, .sse, .sse2, .sse3, .ssse3, .sse4_1, .sse4_2, .avx, .avx2,
        }),
        .cpu_features_add = std.Target.x86.featureSet(&.{.soft_float}),
    });

    const contract = b.dependency("r4os_contract", .{});
    const contract_kernel = b.createModule(.{
        .root_source_file = contract.path("Generated/Kernel/Zig/r4x_api_generated.zig"),
        .target = target,
        .optimize = optimize,
    });

    const kernel_mod = b.createModule(.{
        .root_source_file = b.path("main.zig"),
        .target = target,
        .optimize = optimize,
        .code_model = .kernel,
        .red_zone = false,
        .pic = false,
        .sanitize_c = .off,
        .single_threaded = true,
        .stack_check = false,
        .stack_protector = false,
        .strip = strip_kernel,
    });
    const config = b.addOptions();
    config.addOption(bool, "enable_exception_test", enable_exception_test);
    config.addOption(bool, "enable_page_fault_test", enable_page_fault_test);
    config.addOption(bool, "enable_general_protection_test", enable_general_protection_test);
    config.addOption(bool, "enable_crash_screen_test", enable_crash_screen_test);
    config.addOption(bool, "enable_kernel_fatal_test", enable_kernel_fatal_test);
    config.addOption(bool, "enable_zig_panic_test", enable_zig_panic_test);
    config.addOption(bool, "enable_com1_debug", enable_com1_debug);
    config.addOption(bool, "enable_metrics", enable_metrics);
    config.addOption(bool, "force_x2apic", force_x2apic);
    config.addOption(bool, "enable_stack_guard_test", stack_guard_test);
    config.addOption(bool, "enable_boot_selftests", boot_selftests);
    config.addOption(bool, "enable_net_loss_test", net_loss_test);
    kernel_mod.addOptions("config", config);
    kernel_mod.addImport("r4os_kernel_contract", contract_kernel);
    kernel_mod.addImport("r4f_format", b.createModule(.{
        .root_source_file = b.path("kernel/font_format.zig"),
        .target = target,
        .optimize = optimize,
    }));
    const ntfs_format_mod = b.createModule(.{
        .root_source_file = b.path("Support/ntfs_format.zig"),
        .target = target,
        .optimize = optimize,
    });
    kernel_mod.addImport("ntfs_format", ntfs_format_mod);
    const ntfs_volume_mod = b.createModule(.{
        .root_source_file = b.path("Support/ntfs_volume.zig"),
        .target = target,
        .optimize = optimize,
    });
    ntfs_volume_mod.addImport("ntfs_format", ntfs_format_mod);
    kernel_mod.addImport("ntfs_volume", ntfs_volume_mod);
    kernel_mod.addImport("system_update_recovery", b.createModule(.{
        .root_source_file = b.path("Support/system_update_recovery.zig"),
        .target = target,
        .optimize = optimize,
    }));
    kernel_mod.addImport("upload_publish_claim", b.createModule(.{
        .root_source_file = b.path("Support/upload_publish_claim.zig"),
        .target = target,
        .optimize = optimize,
    }));
    kernel_mod.addImport("r4_registry_core", b.createModule(.{
        .root_source_file = b.path("Support/registry_core.zig"),
        .target = target,
        .optimize = optimize,
    }));
    kernel_mod.addAssemblyFile(b.path("arch/x86_64/cpu.S"));
    kernel_mod.addAssemblyFile(b.path("arch/x86_64/isr.S"));

    const kernel = b.addExecutable(.{
        .name = "r4os.elf",
        .root_module = kernel_mod,
    });
    kernel.setLinkerScript(b.path("linker.ld"));
    kernel.entry = .{ .symbol_name = "kmain" };
    kernel.image_base = 0xffffffff80000000;
    b.installArtifact(kernel);

    const test_step = b.step("test", "Build the kernel and run kernel-owned host tests");
    test_step.dependOn(&kernel.step);
    const unit_tests = [_][]const u8{
        "bootscreen_tests.zig",
        "display/console_scroll_buffer.zig",
        "display/framebuffer.zig",
        "driver/input/key_layout.zig",
        "driver/usb/usb_boot_timing.zig",
        "driver/usb/usb_msc_retry.zig",
        "driver/usb/xhci_bulk_completion.zig",
        "driver/usb/xhci_endpoint_context.zig",
        "driver/usb/xhci_endpoint_recovery.zig",
        "driver/usb/xhci_event_router.zig",
        "driver/usb/xhci_ring_cycle.zig",
        "kernel/bootscreen_r4b_format.zig",
        "net/config_writer.zig",
        "platform/monotonic_math.zig",
        "program/gui_alpha8.zig",
        "program/r4x_start.zig",
        "sched/wait_node.zig",
    };
    for (unit_tests) |path| addUnitTest(b, test_step, path);
    addLoaderTests(b, test_step, contract, config);

    addProviderNegative(
        b,
        test_step,
        contract,
        "Tests/Fixture/ProviderWrongSignatureNegative.zig",
        "expected type",
    );
    addProviderNegative(
        b,
        test_step,
        contract,
        "Tests/Fixture/ProviderMissingFieldNegative.zig",
        "missing struct field",
    );
}

fn addUnitTest(b: *std.Build, test_step: *std.Build.Step, path: []const u8) void {
    const tests = b.addTest(.{
        .root_module = b.createModule(.{
            .root_source_file = b.path(path),
            .target = b.graph.host,
            .optimize = .ReleaseSafe,
        }),
    });
    const run = b.addRunArtifact(tests);
    test_step.dependOn(&run.step);
}

fn addLoaderTests(
    b: *std.Build,
    test_step: *std.Build.Step,
    contract: *std.Build.Dependency,
    config: *std.Build.Step.Options,
) void {
    const target = b.graph.host;
    const optimize: std.builtin.OptimizeMode = .ReleaseSafe;
    const root = b.createModule(.{
        .root_source_file = b.path("loader_tests.zig"),
        .target = target,
        .optimize = optimize,
    });
    root.addOptions("config", config);
    root.addImport("r4os_kernel_contract", b.createModule(.{
        .root_source_file = contract.path("Generated/Kernel/Zig/r4x_api_generated.zig"),
        .target = target,
        .optimize = optimize,
    }));
    root.addImport("r4f_format", b.createModule(.{
        .root_source_file = b.path("kernel/font_format.zig"),
        .target = target,
        .optimize = optimize,
    }));
    const ntfs_format = b.createModule(.{
        .root_source_file = b.path("Support/ntfs_format.zig"),
        .target = target,
        .optimize = optimize,
    });
    root.addImport("ntfs_format", ntfs_format);
    const ntfs_volume = b.createModule(.{
        .root_source_file = b.path("Support/ntfs_volume.zig"),
        .target = target,
        .optimize = optimize,
    });
    ntfs_volume.addImport("ntfs_format", ntfs_format);
    root.addImport("ntfs_volume", ntfs_volume);
    root.addImport("system_update_recovery", b.createModule(.{
        .root_source_file = b.path("Support/system_update_recovery.zig"),
        .target = target,
        .optimize = optimize,
    }));
    root.addImport("upload_publish_claim", b.createModule(.{
        .root_source_file = b.path("Support/upload_publish_claim.zig"),
        .target = target,
        .optimize = optimize,
    }));
    root.addImport("r4_registry_core", b.createModule(.{
        .root_source_file = b.path("Support/registry_core.zig"),
        .target = target,
        .optimize = optimize,
    }));

    const tests = b.addTest(.{ .root_module = root });
    const run = b.addRunArtifact(tests);
    test_step.dependOn(&run.step);
}

fn addProviderNegative(
    b: *std.Build,
    test_step: *std.Build.Step,
    contract: *std.Build.Dependency,
    fixture: []const u8,
    expected_diagnostic: []const u8,
) void {
    const compile = b.addSystemCommand(&.{
        b.graph.zig_exe,
        "build-obj",
        "--dep",
        "kernel_api",
    });
    compile.addPrefixedFileArg("-Mroot=", b.path(fixture));
    compile.addArgs(&.{ "--dep", "r4os_kernel_contract" });
    compile.addPrefixedFileArg("-Mkernel_api=", b.path("program/r4x_api.zig"));
    compile.addPrefixedFileArg(
        "-Mr4os_kernel_contract=",
        contract.path("Generated/Kernel/Zig/r4x_api_generated.zig"),
    );
    compile.addArg("-fno-emit-bin");
    compile.expectExitCode(1);
    compile.expectStdErrMatch(expected_diagnostic);
    test_step.dependOn(&compile.step);
}
