// Bounded LOG1 format belongs to the native log owner.
pub const Severity=enum(u8){info=0,warn=1,err=2};
extern fn r4os_rust_log_event(bool,u8,u32,[*:0]const u8) void;
pub fn driver(level:Severity,owner:u32,text:[*:0]const u8) void {r4os_rust_log_event(false,@intFromEnum(level),owner,text);}
pub fn protocol(level:Severity,slot:u32,text:[*:0]const u8) void {r4os_rust_log_event(true,@intFromEnum(level),slot,text);}
