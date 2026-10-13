const std = @import("std");

const KernelOption = struct {
    name: []const u8,
    config: []const u8,
    type: enum { bool, u32 },
    default: u64,
    bit: ?u6 = null,
    description: []const u8,
};

pub fn build(b: *std.Build) void {
    // The R4OS kernel is pinned to ReleaseSafe. Debug overflows the Zig 0.16
    // ELF linker, while the kernel must never inherit a caller optimization.
    const optimize: std.builtin.OptimizeMode = .ReleaseSafe;
    const parsed = std.json.parseFromSlice([]KernelOption, b.allocator, @embedFile("BuildOptions.json"), .{}) catch @panic("Invalid canonical Kernel build options");
    const config = b.addOptions();
    var rust_build_flags: u64 = 0;
    var strip_kernel = true;
    for (parsed.value) |option| switch (option.type) {
        .bool => {
            const value = b.option(bool, option.name, option.description) orelse (option.default != 0);
            config.addOption(bool, option.config, value);
            if (value) rust_build_flags |= @as(u64, 1) << option.bit.?;
            if (std.mem.eql(u8, option.name, "strip-kernel")) strip_kernel = value;
        },
        .u32 => config.addOption(u32, option.config, b.option(u32, option.name, option.description) orelse @intCast(option.default)),
    };
    config.addOption(u64, "rust_build_flags", rust_build_flags);
    const rust_archive = b.option([]const u8, "rust-archive", "Private native archive produced by Kernel/Build.ps1") orelse @panic("Use Kernel/Build.sh or Build.bat to build the checked Cargo archive first; there is no Zig-only kernel fallback");

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
        .single_threaded = false,
        .stack_check = false,
        .stack_protector = false,
        .strip = strip_kernel,
    });
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
    kernel_mod.addAssemblyFile(b.path("arch/x86_64/ap_trampoline.S"));
    kernel_mod.addObjectFile(.{ .cwd_relative = rust_archive });

    const kernel = b.addExecutable(.{
        .name = "r4os.elf",
        .root_module = kernel_mod,
    });
    // Select referenced sections from native Cargo/core archive members.
    // Unused compiler runtime (including hard-float helpers) is not kernel code.
    kernel.link_gc_sections = true;
    kernel.setLinkerScript(b.path("linker.ld"));
    kernel.entry = .{ .symbol_name = "kmain" };
    kernel.image_base = 0xffffffff80000000;
    b.installArtifact(kernel);

    const test_step = b.step("test", "Build the kernel and run kernel-owned host tests");
    const storage_test_step = b.step("storage-test", "Bounded partition identity and installation boot tests");
    addUnitTest(b, storage_test_step, config, "storage_boot_tests.zig");
    addUnitTest(b, storage_test_step, config, "storage/access_state.zig");
    addUnitTest(b, storage_test_step, config, "fs/fat/directory_growth.zig");
    addUnitTest(b, storage_test_step, config, "fs/copy_transfer.zig");
    test_step.dependOn(storage_test_step);
    const input_test_step = b.step("input-test", "Run the existing bounded input and USB owner tests");
    for ([_][]const u8{
        "input_controller_tests.zig",
        "driver/input/codepoint_queue.zig",
        "driver/input/key_layout.zig",
        "driver/input/hid_set1.zig",
        "usb_host_controller_tests.zig",
        "driver/usb/xhci_event_router.zig",
    }) |path| addUnitTest(b, input_test_step, config, path);
    test_step.dependOn(input_test_step);
    test_step.dependOn(&kernel.step);
    const display_test_step = b.step("display-test", "Bounded display ownership, fallback and firmware-writer tests");
    addDisplayUnitTest(b, display_test_step, contract, config);
    addUnitTest(b, display_test_step, config, "program/remote_frame_state.zig");
    test_step.dependOn(display_test_step);
    const unit_tests = [_][]const u8{
        "audio/backend_contract.zig",
        "audio/mixer.zig",
        "audio/pcm.zig",
        "driver/usb/usb_boot_timing.zig",
        "driver/usb/usb_msc_retry.zig",
        "driver/usb/xhci_bulk_completion.zig",
        "driver/usb/xhci_endpoint_context.zig",
        "driver/usb/xhci_endpoint_recovery.zig",
        "driver/usb/xhci_ring_cycle.zig",
        "driver/usb/xhci_trb_chain.zig",
        "driver/usb/xhci_transfer_pool.zig",
        "fs/request_scope.zig",
        "fs/page_cache_batch.zig",
        "fs/page_cache_policy.zig",
        "memory/heap_policy.zig",
        "memory/owner_lock_policy.zig",
        "memory/page_batch.zig",
        "kernel/bootscreen_r4b_format.zig",
        "kernel/dma_segments.zig",
        "kernel/dma_sync_range.zig",
        "kernel/pci_interrupt_policy.zig",
        "kernel/driver_work_deadline.zig",
        "kernel/driver_work_queue.zig",
        "driver/com_tx_policy.zig",
        "kernel/service_ipc_queue.zig",
        "kernel/smp_policy.zig",
        "arch/x86_64/tlb_policy.zig",
        "net/backend_contract.zig",
        "net/config_writer.zig",
        "net/rx_handoff.zig",
        "net/serial_link_runtime.zig",
        "net/tcp_runtime.zig",
        "platform/monotonic_math.zig",
        "platform/pci_scan.zig",
        "program/gui_alpha8.zig",
        "program/lifecycle_retire_policy.zig",
        "program/r4x_start.zig",
        "sched/initial_stack.zig",
        "sched/task_index.zig",
        "sched/wait_node.zig",
        "storage/block_dispatch.zig",
        "storage/block_split.zig",
        "storage/gpt.zig",
    };
    for (unit_tests) |path| addUnitTest(b, test_step, config, path);
    addFontUnitTest(b, test_step);
    addBootscreenUnitTest(b, test_step);
    addContractUnitTest(b, test_step, contract, config, "services_tests.zig");
    const loader_test_step = b.step("loader-test", "Run the existing module and driver ownership tests");
    addLoaderTests(b, loader_test_step, contract, config);
    test_step.dependOn(loader_test_step);

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
        "Tests/Fixture/ProviderUnknownFieldNegative.zig",
        "no field named",
    );
}

fn addBootscreenUnitTest(b: *std.Build, test_step: *std.Build.Step) void {
    const root = b.createModule(.{
        .root_source_file = b.path("bootscreen_tests.zig"),
        .target = b.graph.host,
        .optimize = .ReleaseSafe,
    });
    root.addImport("r4f_format", b.createModule(.{
        .root_source_file = b.path("kernel/font_format.zig"),
        .target = b.graph.host,
        .optimize = .ReleaseSafe,
    }));
    const tests = b.addTest(.{ .root_module = root });
    const run = b.addRunArtifact(tests);
    test_step.dependOn(&run.step);
}

fn addDisplayUnitTest(b: *std.Build, test_step: *std.Build.Step, contract: *std.Build.Dependency, config: *std.Build.Step.Options) void {
    const root = b.createModule(.{ .root_source_file = b.path("display_tests.zig"), .target = b.graph.host, .optimize = .ReleaseSafe });
    root.addOptions("config", config);
    root.addImport("r4os_kernel_contract", b.createModule(.{ .root_source_file = contract.path("Generated/Kernel/Zig/r4x_api_generated.zig"), .target = b.graph.host, .optimize = .ReleaseSafe }));
    root.addImport("r4f_format", b.createModule(.{ .root_source_file = b.path("kernel/font_format.zig"), .target = b.graph.host, .optimize = .ReleaseSafe }));
    const filter = b.option([]const u8, "display-test-filter", "Run only matching existing display ownership tests");
    const tests = b.addTest(.{ .root_module = root, .filters = if (filter) |value| &.{value} else &.{} });
    const run = b.addRunArtifact(tests);
    test_step.dependOn(&run.step);
}

fn addUnitTest(b: *std.Build, test_step: *std.Build.Step, config: *std.Build.Step.Options, path: []const u8) void {
    const tests = b.addTest(.{
        .root_module = b.createModule(.{
            .root_source_file = b.path(path),
            .target = b.graph.host,
            .optimize = .ReleaseSafe,
        }),
    });
    tests.root_module.addOptions("config", config);
    const run = b.addRunArtifact(tests);
    test_step.dependOn(&run.step);
}

fn addFontUnitTest(b: *std.Build, test_step: *std.Build.Step) void {
    const root = b.createModule(.{
        .root_source_file = b.path("font_test.zig"),
        .target = b.graph.host,
        .optimize = .ReleaseSafe,
    });
    root.addImport("r4f_format", b.createModule(.{
        .root_source_file = b.path("kernel/font_format.zig"),
        .target = b.graph.host,
        .optimize = .ReleaseSafe,
    }));
    const tests = b.addTest(.{ .root_module = root });
    const run = b.addRunArtifact(tests);
    test_step.dependOn(&run.step);
}

fn addContractUnitTest(
    b: *std.Build,
    test_step: *std.Build.Step,
    contract: *std.Build.Dependency,
    config: *std.Build.Step.Options,
    path: []const u8,
) void {
    const host_contract = b.createModule(.{
        .root_source_file = contract.path("Generated/Kernel/Zig/r4x_api_generated.zig"),
        .target = b.graph.host,
        .optimize = .ReleaseSafe,
    });
    const root = b.createModule(.{
        .root_source_file = b.path(path),
        .target = b.graph.host,
        .optimize = .ReleaseSafe,
    });
    root.addOptions("config", config);
    root.addImport("r4os_kernel_contract", host_contract);
    const tests = b.addTest(.{ .root_module = root });
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

    const filter = b.option([]const u8, "loader-test-filter", "Run only matching existing loader ownership tests");
    const tests = b.addTest(.{ .root_module = root, .filters = if (filter) |value| &.{value} else &.{} });
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
