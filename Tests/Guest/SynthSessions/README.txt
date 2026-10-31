Exclusive synth sessions: actual processes, callbacks and R4D lifetime
====================================================================
Explicit IMAGE_SCOPE=none native fixtures, never normal installed modules.
Build.ps1 uses the canonical SDK manifest project on either host; all generated
projects, caches and artifacts default to Temp/SynthSessionsFixtures.

The paired child/P53CH.R4X and controller/P53APP.R4X require the normal SID,
OPL3, MIDI and HDA providers and three audio R4Ps in a private KVM4 Test image.
Install both apps in /TEMP. Run controller SF/MF (foreign calls against another
live process), SE/ME (natural owner exit), SK/MK (targeted process kill), SX/MX
(actual existing stack-reserve guard recovery), one case per process. Full
generation-bound output, exact completion/reap and observation release matter.
The stack exhaustion body is the existing Spectra stack/recovery consume body;
require vector14, reserve-exhausted reason, and exact instance/task/thread
generations. Kernel447 also admits narrowly checked program #UD at the same
existing resident callback boundary; other origins remain fatal (below).

On original Kernel427 the eight frozen binaries fail these owner contracts.
Only that legacy controller explicitly repairs global state after recording a
failure. Kernel428 passes all eight with no such repair. These unchanged paired
source/binary/installed readbacks are retained in the release qualification.

For controlled boundaries install protocol/P53C.R4P, driver/P53D.R4D and
deep/P53DEEP.R4X. Use DRIVER=P53D and disable HDA/AC97/SID/OPL3/MIDI only in
this private CONFIG. The driver registers actual Core PCM and SID/OPL3 callbacks;
its counters describe software callback acknowledgements, never DMA/speakers.
The resident protocol observer binds after lazy activation, and releases the
borrowed driver record on real shutdown. There is no duplicated open count.

Deep modes on a normal kernel:
 P: failed private Stop retains exact pending PCM for a retry without rendering;
 S/M: actual exiting SID/MIDI owner stays unreaped through private Stop error,
      common Idle Busy/error, and (SID) private Release error, until actual ACK;
 R: same-config provider replacement is refused while acquired and allowed
    after exact release; stale calls without a new acquisition fail;
 T/U: two actual same-process program threads race SID/MIDI acquire, exactly
      one succeeds; main-thread use/close proves process-scoped ownership;
 C: MIDI Close with zero ordinary streams preserves a producing SID source,
    followed by SID continuation and one real last-source Idle acknowledgement.
 N: MIDI pending4096B survives private Stop error/Busy, resumes the identical
    PCM hash without rendering again, then one last-source Idle ACK; an actual
    ordinary PCM stream also survives MIDI Close and accepts the next write.
Each app prints detailed actual receipts, result=PASS and exits0. SID callbacks
also log through the real DriverApi owner. Existing callback admission forbids
heap allocation; a callback-scope check requires heap unavailable. This is not
a claim that CallbackToken alone retains driver lifetime.

bridge.rs is for an explicit additional private kernel only. Save normal
Rust/audio/core.rs and r4os.elf, include the module with relative path
../../Tests/Guest/SynthSessions/bridge.rs in core.rs and retain its function
address through black_box in init. Read the symbol address from the unstripped
Artifacts/Kernel/Rust/x86_64-unknown-none/release/r4os corresponding exactly to
the private stripped ELF. Pin that ELF's SHA and image boot readback before
calling deep G:HEX/H:HEX (same instance/wrong generation and explicit KERNEL
owner) or L:HEX/V:HEX (real SID/MIDI driver unload veto, closing admission,
successful unload/reload, actual runtime/resource/registration generations).
W:HEX switches from unloaded P53D to a separately installed P52D for the
maintained SidStop fixture. No arbitrary address or normal product may be used.
The bridge calls existing Core/R4D functions, not a second ownership predicate.
It adds no public platform API or table field. Restore core.rs and rebuild the
normal artifact byte-identically before product/benchmark use; do not commit
the include/call. Shut down owned guests normally and remove private source
images with the canonical Distribution test-retention helper.

CPU exceptions (0.88.87), still on the normal kernel
------------------------------------------------
Build only changed parts with -Part driver,fault,fault-controller; optional
-CacheRoot reuses an existing SDK cache without rebuilding unrelated fixtures.
Install fault/F87A.R4X, fault-controller/F87CTRL.R4X and the controlled P53D
with the same existing P53C protocol; provide C:\TEMP\IO87.BIN (8192 bytes).
F87CTRL M: real main UD2 after SID acquisition and a retained File I/O request.
F87CTRL T: real worker UD2 after MIDI acquisition; the exact worker generation
           differs from its main. Both must complete -4097 / Failed, full
           output/reap/observation release, one private Stop ACK, successful
           reacquire/close and a surviving foreign ordinary PCM write/close.
Private Stop error blocks completion; a configured Busy interval also stays
blocked. The counter check does not isolate one specific Busy callback; only
the final actual successful Stop acknowledgement authorizes release. Existing
controlled Stop/Busy cases retain their separate callback proof. An unclosed
I/O request is deliberately retained; no physically pending DMA is claimed.
The driver has test-only UD2 for write_result=-87. F87CTRL D must produce a
global vector6 fatal from the real loaded R4D inside an audio-progress task.
F87A I executes CLI then UD2 and must also stay globally fatal. These two
intentional fatal cases require isolated owned guests, full originals and one
guarded QMP quit after capture; they cannot shut down normally. Never run D/I
in a normal user session. Preserve the existing P53APP SX guard-recovery case.
These are explicit native fixtures, not normal boot tests or product probes.
