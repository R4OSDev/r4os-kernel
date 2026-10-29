/* Explicit private-image R4D fixture; no hardware backend or installed module. */
#include <r4os/driver_lifecycle.h>
#ifndef COUNT_FAIL_INIT
#define COUNT_FAIL_INIT 0
#endif
static R4SdkDriverContext ctx;
static R4SdkDriverHeapContext heap;
static R4SdkDriverSemaphoresContext sem;
static const R4DriverApi *saved;
static R4DriverHeapAllocation allocation={.version=1,.size=sizeof(R4DriverHeapAllocation)};
static uint64_t semaphore;
static R4DriverDmaLease dma;
static _Alignas(4096) uint8_t pinned[4096];
static int32_t work(uintptr_t x){return x==29?129:-1;}
static int32_t thread(uintptr_t x){return x==29?229:-1;}
static void log_epoch(const char *stage,uint64_t epoch){
 uint8_t line[112];uint32_t n=0;const char *p="DC29 ";while(*p)line[n++]=(uint8_t)*p++;
 for(p=stage;*p;)line[n++]=(uint8_t)*p++;p=" epoch=";while(*p)line[n++]=(uint8_t)*p++;
 uint8_t digits[24];uint32_t k=0;do{digits[k++]=(uint8_t)('0'+epoch%10);epoch/=10;}while(epoch);while(k)line[n++]=digits[--k];line[n]=0;saved->log_info(line);
}
#define CHECK(x) do{if(r4driver_result((x))!=0){saved->log_error((const uint8_t *)("DC29 failed " #x));return -29;}}while(0)
int32_t count_init(const R4DriverApi *api){
 saved=api;if(r4driver_init(api,&ctx))return -1;
 R4SdkDriverSlot h=R4SdkDriverContext_heap(0,&ctx,&heap),s=R4SdkDriverContext_semaphores(0,&ctx,&sem);
 if(h.status||!(h.flags&1)||s.status||!(s.flags&1))return -2;
 CHECK(R4SdkDriverHeapContext_allocate(0,&heap,192,64,&allocation));
 uint8_t *data=(uint8_t *)(uintptr_t)allocation.cpu_address;for(uint32_t i=0;i<192;i++)data[i]=(uint8_t)(i^29);
 CHECK(R4SdkDriverSemaphoresContext_create(0,&sem,1,1,&semaphore));
 CHECK(R4SdkDriverSemaphoresContext_acquire(0,&sem,semaphore,0));CHECK(R4SdkDriverSemaphoresContext_release(0,&sem,semaphore));
 R4DriverHeapStats hs={.version=1,.size=sizeof(hs)};CHECK(R4SdkDriverHeapContext_stats(0,&heap,&hs));if(hs.allocations!=1||!hs.owner_epoch)return -3;
 if(COUNT_FAIL_INIT){log_epoch("intentional-init-refusal heap+sem-owned",hs.owner_epoch);return -129;}
 dma.owner=&ctx;dma.pin=(R4DmaPinnedBuffer){.version=1,.size=sizeof(dma.pin)};dma.mapping=(R4DmaMapping){.version=1,.size=sizeof(dma.mapping)};
 CHECK(R4SdkDriverContext_pinDmaBuffer(0,&ctx,pinned,sizeof(pinned),&dma.pin));
 R4DmaConstraints limits={.version=1,.size=sizeof(limits),.dma_mask=0xffffffff,.boundary=4096,.max_segment_bytes=4096,.alignment=16,.max_segments=4,.flags=R4OS_DMA_FLAG_COHERENT|R4OS_DMA_FLAG_ALLOW_BOUNCE};
 CHECK(R4SdkDriverContext_mapDmaPinned(0,&ctx,&dma.pin,&limits,R4OS_DMA_DIRECTION_BIDIRECTIONAL,&dma.mapping));
 CHECK(R4SdkDriverContext_syncDmaForDevice(0,&ctx,&dma.mapping));CHECK(R4SdkDriverContext_syncDmaForCpu(0,&ctx,&dma.mapping));
 if(r4driver_dma_close_quiesced(&dma))return -12;
 R4DriverWorkLease w={.owner=&ctx};CHECK(R4SdkDriverContext_workSubmit(0,&ctx,(uintptr_t)work,29,0,&w.ticket));
 int32_t result=0;if(r4driver_result(R4SdkDriverContext_completionWait(0,&ctx,w.ticket,1000,&result))<0||result!=129)return -4;
 if(r4driver_work_release_terminal(&w))return -5;
 R4SdkDriverThreadsContext threads;R4SdkDriverSlot t=R4SdkDriverContext_threads(0,&ctx,&threads);if(t.status||!(t.flags&1))return -6;
 uint64_t handle=0;CHECK(R4SdkDriverThreadsContext_start(0,&threads,(uintptr_t)thread,29,R4OS_DRIVER_THREAD_FLAG_PARALLEL,&handle));
 CHECK(R4SdkDriverThreadsContext_join(0,&threads,handle,1000,&result));if(result!=229)return -7;
 R4DriverThreadStatus status={.version=1,.size=sizeof(status)};CHECK(R4SdkDriverThreadsContext_status(0,&threads,handle,&status));if(!status.owner_epoch||!status.task_generation)return -8;
 CHECK(R4SdkDriverThreadsContext_release(0,&threads,handle));
 log_epoch("init PASS heap+sem-live DMA-ended work+task-released",hs.owner_epoch);return 0;
}
int32_t count_shutdown(void){
 if(allocation.handle){uint8_t *data=(uint8_t *)(uintptr_t)allocation.cpu_address;for(uint32_t i=0;i<192;i++)if(data[i]!=(uint8_t)(i^29))return -9;}
 if(dma.owner && r4driver_dma_close_quiesced(&dma))return -10;
 if(semaphore){CHECK(R4SdkDriverSemaphoresContext_destroy(0,&sem,semaphore));semaphore=0;}
 if(allocation.handle){CHECK(R4SdkDriverHeapContext_release(0,&heap,allocation.handle));allocation.handle=0;}
 R4DriverHeapStats hs={.version=1,.size=sizeof(hs)};CHECK(R4SdkDriverHeapContext_stats(0,&heap,&hs));if(hs.allocations||hs.pending_creates||hs.pending_releases)return -11;
 log_epoch("shutdown PASS heap+sem+DMA-ended no-work-or-task",hs.owner_epoch);return 0;
}
R4D_ENTRIES(count_init,count_shutdown);
