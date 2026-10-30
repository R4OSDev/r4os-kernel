const std=@import("std");const r4os=@import("r4os");const a=r4os.abi;
var line:[512]u8=undefined;var output:[16384]u8=undefined;
const Child=struct{handle:a.ProgramProcessHandle,observation:a.ProgramObservationHandle};
const Run=struct{exit:i32=-99,reason:u8=0,flags:u32=0,ready:bool=false};
const Controller=struct{
 sys:r4os.r4sys.Context,desk:r4os.r4desk.Context,audio:r4os.r4audio.Context,performance:r4os.PerformanceView,
 fn report(self:*const Controller,comptime fmt:[]const u8,args:anytype)void{self.sys.println(std.fmt.bufPrint(&line,fmt,args) catch "P53 overflow");}
 fn start(self:*const Controller,kind:u8,mode:u8)?Child{
  var c:Child=.{.handle=.{},.observation=.{}};const args=[_]u8{kind,mode,0};
  const rc=self.sys.programSpawnWithConsoleHostHandle("C:\\TEMP\\P53CH.R4X",@ptrCast(&args),.console,.terminal_window,&c.handle);
  if(rc!=0)return null;
  if(self.sys.programObservationRetain(&c.handle,0,&c.observation)!=0){_=self.sys.programHandleKill(&c.handle);return null;}
  self.report("P53 SPAWN kind={c} mode={c} child={d}:{d}",.{kind,mode,c.handle.instance_id,c.handle.generation});return c;
 }
 fn ready(self:*const Controller,c:Child)bool{
  const end=self.sys.ticks()+|self.sys.ticksFromMilliseconds(3000);
  while(self.sys.ticks()<end){var s:a.ConsoleState=.{};if(self.desk.consoleState(c.handle.instance_id,&s)==0 and s.stdout_bytes!=0)return true;self.sys.sleepTicks(1);}return false;
 }
 fn finish(self:*const Controller,c:Child)Run{
  var done:a.ProgramProcessCompletion=.{};const waited=self.sys.programObservationWait(&c.observation,self.sys.ticksFromMilliseconds(5000),&done);
  if(waited!=0){_=self.sys.programHandleKill(&c.handle);_=self.sys.programHandleWait(&c.handle,self.sys.ticksFromMilliseconds(3000),&done);}
  var count:u32=0;const read=if(waited==0)self.sys.programObservationRead(&c.observation,0,&output,&count) else a.program_handle_error_would_block;
  self.report("P53 COMPLETION child={d}:{d} wait={d} exit={d} reason={d} flags={d} length={d} read={d} bytes={d}",.{c.handle.instance_id,c.handle.generation,waited,done.exit_code,done.exit_reason,done.flags,done.output_length,read,count});
  if(read==0)self.sys.write(output[0..@min(count,output.len)]);
  var reaped=self.sys.programHandleReap(&c.handle,&done);const end=self.sys.ticks()+|self.sys.ticksFromMilliseconds(3000);
  while(reaped==a.program_handle_error_would_block and self.sys.ticks()<end){self.sys.sleepTicks(1);reaped=self.sys.programHandleReap(&c.handle,&done);}
  const released=self.sys.programObservationRelease(&c.observation);
  self.report("P53 RETIRE child={d}:{d} reap={d} observation_release={d}",.{c.handle.instance_id,c.handle.generation,reaped,released});
  return .{.exit=done.exit_code,.reason=done.exit_reason,.flags=done.flags,.ready=waited==0 and read==0 and count==done.output_length and done.handle.instance_id==c.handle.instance_id and done.handle.generation==c.handle.generation and reaped==0 and released==0 and done.flags&a.program_completion_flag_output_incomplete==0 and done.flags&a.program_completion_flag_output_truncated==0};
 }
 fn faultExact(self:*const Controller,handle:a.ProgramProcessHandle)bool{
  var cpu:u32=0;
  while(cpu<64):(cpu+=1){
   var info:a.ProgramStackFaultInfo=.{};const status=self.performance.stackFault(cpu,&info);
   if(status==0)break;if(status!=1)return false;
   const flags=a.stack_fault_flag_task|a.stack_fault_flag_thread|a.stack_fault_flag_instance|a.stack_fault_flag_bounds;
   if(info.instance_id==handle.instance_id and info.instance_generation==handle.generation and info.flags&flags==flags and info.sequence!=0 and info.stack_kind==a.stack_fault_kind_program_main and info.reason==a.stack_fault_reason_reserve_exhausted and info.vm_error==0 and info.exception_vector==14 and info.task_generation!=0 and info.thread_generation!=0 and info.guard_base==info.reserve_base and info.guard_bytes==4096 and info.fault_address>=info.guard_base and info.fault_address-info.guard_base<info.guard_bytes){
    self.report("P53 EXACT_STACK_FAULT child={d}:{d} cpu={d} sequence={d} vector={d} task_generation={d} thread_generation={d} reason={d}",.{handle.instance_id,handle.generation,cpu,info.sequence,info.exception_vector,info.task_generation,info.thread_generation,info.reason});return true;
   }
  }return false;
 }
 fn repairLegacy(self:*const Controller,kind:u8)void{
  const stop=if(kind=='S')self.audio.sidStop(1) else self.audio.midiClose(1);
  const release=if(kind=='S')self.audio.sidRelease(1) else @as(i32,0);
  self.report("P53 BEFORE_ONLY_REPAIR kind={c} stop={d} release={d}; explicit foreign repair after failed owner contract, never a reaper receipt",.{kind,stop,release});
 }
};
pub fn r4_app_main(app:*r4os.App)i32{
 const dev=app.devices() orelse return 3;
 const c=Controller{.sys=app.system(),.desk=app.desktop() orelse return 2,.audio=app.audioLowLevel() orelse return 3,.performance=dev.performance()};
 const args=app.args();if(args.len<2)return 4;const kind=args[0];const mode=args[1];if(kind!='S' and kind!='M')return 5;
 const first=c.start(kind,if(mode=='F')'H' else if(mode=='K')'K' else if(mode=='X')'X' else 'E') orelse return 6;
 var ok=false;
 if(mode=='F'){
  if(!c.ready(first)){_=c.sys.programHandleKill(&first.handle);_=c.finish(first);return 7;}
  const peer=c.start(kind,'F') orelse {_=c.desk.consolePushKey(first.handle.instance_id,'q');_=c.finish(first);return 8;};
  const p=c.finish(peer);const pushed=c.desk.consolePushKey(first.handle.instance_id,'q');const f=c.finish(first);
  ok=p.ready and p.exit==0 and f.ready and f.exit==0 and pushed==0;
 }else{
  var killed:i32=0;
  if(mode=='K'){
   if(!c.ready(first)){_=c.sys.programHandleKill(&first.handle);_=c.finish(first);return 9;}
   killed=c.sys.programHandleKill(&first.handle);c.report("P53 KILL child={d}:{d} result={d}",.{first.handle.instance_id,first.handle.generation,killed});
  }
  const f=c.finish(first);const peer=c.start(kind,'C') orelse return 10;const p=c.finish(peer);
  const expected=if(mode=='K')f.reason==a.program_exit_reason_killed and f.exit!=0 and killed==0 else if(mode=='X')f.reason==a.program_exit_reason_failed and f.exit == -4096 and c.faultExact(first.handle) else f.exit==0;
  ok=f.ready and expected and p.ready and p.exit==0;
 }
 if(!ok)c.repairLegacy(kind);
 c.report("P53 RESULT kind={c} mode={c} result={s}; actual two process generations/native owner callbacks and final output; no modelled owner predicate",.{kind,mode,if(ok)@as([]const u8,"PASS")else@as([]const u8,"FAIL")});
 return if(ok)0 else 1;
}
