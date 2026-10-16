// General output routes have one native owner; private C calls carry no state.
pub const OutputHook = *const fn(u8) callconv(.c) bool;
extern fn r4os_rust_log_hook(?OutputHook,bool) void;
extern fn r4os_rust_log_write(u8,[*]const u8,usize) void;
extern fn r4os_rust_log_number(u64,u8,bool) void;
extern fn r4os_rust_log_control(u8,u32,u32,u32,u32) u32;
pub fn setOutputHook(hook:?OutputHook) void {r4os_rust_log_hook(hook,false);}
pub fn setOutputHookIntercept(hook:?OutputHook) void {r4os_rust_log_hook(hook,true);}
pub fn serialFlush() void {_ = r4os_rust_log_control(0,0,0,0,0);}
pub fn serialEnterSync() void {_ = r4os_rust_log_control(1,0,0,0,0);}
pub fn serialPutcRaw(byte:u8) void {_ = r4os_rust_log_control(2,byte,0,0,0);}
pub fn serialWriteRaw(text:[]const u8) void {r4os_rust_log_write(1,text.ptr,text.len);}
pub fn consolePutcRaw(byte:u8) void {_ = r4os_rust_log_control(3,byte,0,0,0);}
pub fn clearConsole() void {_ = r4os_rust_log_control(4,0,0,0,0);}
pub fn setConsoleMargins(a:u32,b:u32,c:u32,d:u32) void {_ = r4os_rust_log_control(5,a,b,c,d);}
pub fn setConsoleColors(fg:u32,bg:u32) void {_ = r4os_rust_log_control(6,fg,bg,0,0);}
pub fn setConsoleFontScale(scale:u32) void {_ = r4os_rust_log_control(7,scale,0,0,0);}
pub fn setConsoleCursor(x:u32,y:u32) void {_ = r4os_rust_log_control(8,x,y,0,0);}
pub fn clearConsoleFramed(border:u32,inner:u32) void {_ = r4os_rust_log_control(9,border,inner,0,0);}
pub fn consoleCols() u32 {return r4os_rust_log_control(10,0,0,0,0);}
pub fn consoleRows() u32 {return r4os_rust_log_control(11,0,0,0,0);}
pub fn putc(byte:u8) void {_ = r4os_rust_log_control(12,byte,0,0,0);}
pub fn puts(text:[]const u8) void {r4os_rust_log_write(0,text.ptr,text.len);}
pub fn putHex(value:u64,width:u8) void {r4os_rust_log_number(value,width,true);}
pub fn putDec(value:u64) void {r4os_rust_log_number(value,0,false);}
pub fn newline() void {puts("\r\n");}
