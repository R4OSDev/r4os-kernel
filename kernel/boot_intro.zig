// Boot intro block after display, CPU, and input foundations.

const boot_display = @import("../display/boot_display.zig");
const boot_info = @import("../bootloader/boot_info.zig");
const boot_status = @import("boot_status.zig");
const config = @import("config");
const display = @import("../display/display.zig");
const fatal = @import("fatal.zig");
const font = @import("font.zig");
const interrupts = @import("../arch/x86_64/interrupts.zig");
const log = @import("log.zig");

pub fn init() bool {
    log.puts("\r\n=== R4OS x86_64 (Limine boot) ===\r\n");

    if (!boot_info.get().initialized) {
        return fatal.fail(.entry, "BootInfo init failed");
    }

    const boot_display_state = boot_display.get() orelse {
        return fatal.fail(.entry, "No framebuffer in BootInfo");
    };
    const display_mode = display.activeMode() orelse {
        return fatal.fail(.entry, "No active DisplayBackend");
    };
    const boot_console = boot_display_state.console_metrics;

    boot_status.printFoundationSummary();
    if (config.enable_exception_test) triggerExceptionTest();
    if (config.enable_general_protection_test) triggerGeneralProtectionTest();

    log.puts("  Long mode active\r\n");
    log.puts("  Framebuffer: ");
    log.putDec(display_mode.width);
    log.puts(" x ");
    log.putDec(display_mode.height);
    log.puts(" @ ");
    log.putDec(display_mode.bpp);
    log.puts(" bpp\r\n");
    log.puts("  Console: ");
    log.putDec(boot_console.cols);
    log.puts(" cols x ");
    log.putDec(boot_console.rows);
    log.puts(" rows (");
    log.putDec(font.glyphWidth());
    log.putc('x');
    log.putDec(font.glyphHeight());
    log.puts(" font, ");
    log.putDec(boot_console.scale);
    log.puts("x scale)\r\n");
    log.puts("  Kernel base: 0x");
    log.putHex(0xffffffff80000000, 16);
    log.puts("\r\n\r\n");
    return true;
}

fn triggerExceptionTest() noreturn {
    log.puts("  Triggering test exception...\r\n");
    asm volatile ("ud2");
    interrupts.haltForever();
}

fn triggerGeneralProtectionTest() noreturn {
    log.puts("  Triggering test general protection fault...\r\n");
    asm volatile (
        \\movw $0xffff, %%ax
        \\movw %%ax, %%ds
    );
    interrupts.haltForever();
}
