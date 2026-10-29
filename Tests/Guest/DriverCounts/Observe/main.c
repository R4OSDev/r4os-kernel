#include <r4os/r4os.h>
#define EXPECTED 129u
static int index_name(const uint8_t *s){if(s[0]!='D'||s[1]!='C'||s[2]!='2'||s[3]!='9')return -1;uint32_t n=0;for(uint32_t i=4;i<7;i++){if(s[i]<'0'||s[i]>'9')return -1;n=n*10+s[i]-'0';}return s[7]==0?(int)n:-1;}
static void number(R4App *app,uint32_t n){char text[16];uint32_t k=0;do{text[k++]=(char)('0'+n%10);n/=10;}while(n);char out[16];uint32_t i=0;while(k)out[i++]=text[--k];out[i]=0;r4sys_write_cstr(&app->system,out);}
int32_t r4_app_main(R4App *app){
 const R4XStartR4Dev *dev=app->devices.table;if(!dev||!dev->driver_module_info||!dev->device_inventory_summary||!dev->device_inventory_record)return -1;
 uint8_t seen[EXPECTED]={0};uint32_t count=0,max_owner=0;
 for(uint32_t owner=1;owner<EXPECTED+64;owner++){
  R4DriverModuleInfo info={.version=1,.size=sizeof(info)};int32_t rc=((R4DevDriverModuleInfoFn)(uintptr_t)dev->driver_module_info)(owner,&info);if(rc<0)return -2;if(!rc)continue;
  int n=index_name(info.driver_name);if(n<0)continue;if((uint32_t)n>=EXPECTED||seen[n]||info.owner!=owner||!info.generation||!info.module_generation)return -3;seen[n]=1;count++;max_owner=owner;
  R4ProgramDriverWorkPerformanceInfo work={.version=1,.size=sizeof(work)};if(r4dev_performance_driver_work(&app->devices,owner,&work)!=1||!work.owner_present||work.selected_owner!=owner||work.metrics.submitted<1||work.metrics.completed!=work.metrics.submitted||work.metrics.releases!=work.metrics.submitted||work.queue_capacity!=16)return -4;
 }
 if(count!=EXPECTED||max_owner<=64)return -5;
 R4DeviceInventorySummary summary={0};if(((R4DevDeviceInventorySummaryFn)(uintptr_t)dev->device_inventory_summary)(&summary)!=1||summary.truncated||summary.total<=128)return -6;
 for(uint32_t i=0;i<EXPECTED;i++)seen[i]=0;count=0;
 for(uint32_t i=0;i<summary.total;i++){
  R4DeviceInventoryRecord record={0};if(((R4DevDeviceInventoryRecordFn)(uintptr_t)dev->device_inventory_record)(i,&record)!=1)return -7;
  int n=index_name(record.name);if(n<0)continue;if((uint32_t)n>=EXPECTED||seen[n])return -8;seen[n]=1;count++;
 }
 if(count!=EXPECTED)return -9;
 R4DeviceInventoryRecord sentinel;for(uint32_t i=0;i<sizeof(sentinel);i++)((uint8_t *)&sentinel)[i]=0xa5;
 if(((R4DevDeviceInventoryRecordFn)(uintptr_t)dev->device_inventory_record)(summary.total,&sentinel)!=0)return -10;
 for(uint32_t i=0;i<sizeof(sentinel);i++)if(((uint8_t *)&sentinel)[i]!=0xa5)return -11;
 r4sys_write_cstr(&app->system,"DC29 consumer PASS active=");number(app,count);r4sys_write_cstr(&app->system," max_owner=");number(app,max_owner);r4sys_write_cstr(&app->system," inventory=");number(app,summary.total);r4sys_write_cstr(&app->system," work_each>=1 queue_capacity=16 outside_unchanged=1\n");return 0;
}
