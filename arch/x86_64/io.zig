// CPU-I/O-Port-Zugriffe (x86_64).

pub fn outb(port: u16, val: u8) void {
    asm volatile ("outb %[v], %[p]"
        :
        : [v] "{al}" (val),
          [p] "{dx}" (port),
    );
}

pub fn inb(port: u16) u8 {
    return asm volatile ("inb %[p], %[v]"
        : [v] "={al}" (-> u8),
        : [p] "{dx}" (port),
    );
}

pub fn outw(port: u16, val: u16) void {
    asm volatile ("outw %[v], %[p]"
        :
        : [v] "{ax}" (val),
          [p] "{dx}" (port),
    );
}

pub fn inw(port: u16) u16 {
    return asm volatile ("inw %[p], %[v]"
        : [v] "={ax}" (-> u16),
        : [p] "{dx}" (port),
    );
}

pub fn outl(port: u16, val: u32) void {
    asm volatile ("outl %[v], %[p]"
        :
        : [v] "{eax}" (val),
          [p] "{dx}" (port),
    );
}

pub fn inl(port: u16) u32 {
    return asm volatile ("inl %[p], %[v]"
        : [v] "={eax}" (-> u32),
        : [p] "{dx}" (port),
    );
}

pub inline fn cli() void {
    asm volatile ("cli");
}

pub inline fn sti() void {
    asm volatile ("sti");
}

pub inline fn readRflags() u64 {
    return asm volatile ("pushfq; pop %[out]"
        : [out] "=r" (-> u64),
    );
}

pub inline fn wait() void {
    outb(0x80, 0);
}

pub inline fn hlt() void {
    asm volatile ("hlt");
}
