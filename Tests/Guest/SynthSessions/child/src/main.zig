const std=@import("std");const r4os=@import("r4os");const a=r4os.abi;
var line:[256]u8=undefined;
var checksum:u64=0;
pub fn r4_app_main(app:*r4os.App) i32 {
 const sys=app.system();const audio=app.audioLowLevel() orelse return 2;const desk=app.desktop() orelse return 2;
 const args=app.args();if(args.len<2)return 3;const sid=args[0]=='S';const mode=args[1];
 if(mode=='F'){
  const written=if(sid)audio.sidWriteRegister(1,4,0x11) else audio.midiSend(1,0,0x90,60,90);
  const stopped=if(sid)audio.sidStop(1) else @as(i32,0);
  const released=if(sid)audio.sidRelease(1) else audio.midiClose(1);
  sys.println(std.fmt.bufPrint(&line,"P53 FOREIGN kind={c} write={d} stop={d} release={d}",.{args[0],written,stopped,released}) catch "P53 overflow");
  return if(written<0 and released<0 and (!sid or stopped<0))0 else 1;
 }
 const handle=if(sid)audio.sidAcquire() else audio.midiOpenSynth("OPL3");
 sys.println(std.fmt.bufPrint(&line,"P53 ACQUIRE kind={c} mode={c} handle={d}",.{args[0],mode,handle}) catch "P53 overflow");
 if(handle<=0)return 1;
 const h:u32=@intCast(handle);
 if(mode=='X'){
  var info:a.ProgramThreadInfo=.{};
  if(sys.threadStatus(0,&info)!=a.thread_ok or info.stack_reserved_bytes==0 or info.stack_reserved_bytes>32*1024*1024)return 11;
  sys.println(std.fmt.bufPrint(&line,"P53 STACK_EXHAUST kind={c} reserved={d}; existing actual stackguard recovery, distinct from fatalUD2",.{args[0],info.stack_reserved_bytes}) catch "P53 overflow");
  consume(@intCast(info.stack_reserved_bytes/4096+2));return 12;
 }
 if(mode=='H' or mode=='K'){
  sys.println("P53 READY actual acquired source; parent may act");
  const end=sys.ticks()+|sys.ticksFromMilliseconds(10000);
  while(desk.readKey()!='q' and sys.ticks()<end)sys.sleepTicks(1);
  if(mode=='K')return 10;
  const own=if(sid)audio.sidWriteRegister(h,4,0x11) else audio.midiSend(h,0,0x90,62,90);
  sys.println(std.fmt.bufPrint(&line,"P53 OWN_AFTER_PEER kind={c} result={d}",.{args[0],own}) catch "P53 overflow");
  return if(own==0)0 else 1;
 }
 if(mode=='C'){
  const end=sys.ticks()+|sys.ticksFromMilliseconds(3000);
  var stopped=if(sid)audio.sidStop(h) else audio.midiClose(h);
  while(stopped==a.service_api_result_busy and sys.ticks()<end){sys.sleepTicks(1);stopped=if(sid)audio.sidStop(h) else audio.midiClose(h);}
  const released=if(sid)audio.sidRelease(h) else @as(i32,0);
  sys.println(std.fmt.bufPrint(&line,"P53 OWN_CLOSE kind={c} stop={d} release={d}",.{args[0],stopped,released}) catch "P53 overflow");
  return if(stopped==0 and released==0)0 else 1;
 }
 sys.println("P53 EXIT_WITHOUT_RELEASE natural source owner must be reaped");
 return 0;
}

noinline fn consume(depth: u32) void {
    var storage: [4096]u8 = undefined;
    asm volatile (""
        :
        : [storage] "r" (&storage),
        : .{ .memory = true });
    const bytes: [*]volatile u8 = @ptrCast(&storage);
    bytes[0] = @truncate(depth);
    bytes[storage.len - 1] = 0x5b;
    if (depth != 0) consume(depth - 1);
    checksum +%= @as(u64, bytes[0]) + bytes[storage.len - 1];
}
