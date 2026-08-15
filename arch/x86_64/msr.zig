// x86_64 Model Specific Register helpers.

pub fn read(id: u32) u64 {
    var low: u32 = 0;
    var high: u32 = 0;
    asm volatile ("rdmsr"
        : [low] "={eax}" (low),
          [high] "={edx}" (high),
        : [id] "{ecx}" (id),
    );
    return (@as(u64, high) << 32) | low;
}

pub fn write(id: u32, value: u64) void {
    const low: u32 = @truncate(value);
    const high: u32 = @truncate(value >> 32);
    asm volatile ("wrmsr"
        :
        : [id] "{ecx}" (id),
          [low] "{eax}" (low),
          [high] "{edx}" (high),
    );
}
