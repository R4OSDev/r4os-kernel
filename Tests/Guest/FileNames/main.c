/* Public SDK consumer against actual mounted private-image FAT32/NTFS volumes.
 * Never run on a volume with user-owned N27* files. No mock is a provider proof. */
#include <r4os/storage_helpers.h>

static uint32_t cases, failures;
static uint32_t length(const char *s) { uint32_t n=0; while(s[n])++n; return n; }
static void text(R4App *a,const char *s) {
    ((R4SysWriteFn)(uintptr_t)a->system.table->write)((const uint8_t *)s,length(s));
}
static void number(R4App *a,int64_t v) {
    uint8_t b[24];uint32_t n=0;uint64_t u=v<0?(uint64_t)(-(v+1))+1:(uint64_t)v;
    if(v<0)text(a,"-");do{b[n++]=(uint8_t)('0'+u%10);u/=10;}while(u);
    while(n)((R4SysWriteFn)(uintptr_t)a->system.table->write)(&b[--n],1);
}
static void field(R4App *a,const char *name,int64_t value) { text(a,name);number(a,value); }
static int equal(const uint8_t *a,const uint8_t *b,uint32_t n) {
    for(uint32_t i=0;i<n;++i)if(a[i]!=b[i])return 0;return 1;
}
static int filled(const void *value,uint32_t count,uint8_t byte) {
    const uint8_t *p=value;for(uint32_t i=0;i<count;++i)if(p[i]!=byte)return 0;return 1;
}
static void check(R4App *a,int ok,const char *label) {
    ++cases;if(!ok){++failures;text(a,"N27 FAIL ");text(a,label);text(a,"\n");}
}
static void join(char *out,const char *prefix,const char *name) {
    while(*prefix)*out++=*prefix++;while(*name)*out++=*name++;*out=0;
}
static int32_t remove_file(R4App *a,const char *path) {
    return ((R4SysFileDeleteFn)(uintptr_t)a->system.table->file_delete)((const uint8_t *)path);
}
static int32_t read_file(R4App *a,const char *path,uint8_t *out,uint32_t capacity) {
    return ((R4SysFileReadFn)(uintptr_t)a->system.table->file_read)((const uint8_t *)path,out,capacity);
}
static int32_t store(R4App *a,const char *path,const uint8_t *data,uint32_t count) {
    int32_t rc=((R4SysFileStreamBeginFn)(uintptr_t)a->system.table->file_stream_begin)(
        (const uint8_t *)path,R4OS_FILE_STREAM_OPEN_CREATE);
    if(rc!=0)return rc;
    rc=((R4SysFileStreamWriteFn)(uintptr_t)a->system.table->file_stream_write)(
        (const uint8_t *)path,0,data,count,0);
    if(rc==(int32_t)count) {
        rc=((R4SysFileStreamFinishFn)(uintptr_t)a->system.table->file_stream_finish)(
            (const uint8_t *)path,count,0);
        if(rc==0)return 0;
    }
    ((R4SysFileStreamAbortFn)(uintptr_t)a->system.table->file_stream_abort)((const uint8_t *)path);
    return rc<0?rc:R4OS_FILE_STREAM_ERROR_IO;
}
static void probe(R4App *a,const char *prefix,const char *label,const char *name,int32_t expected) {
    char path[1024];join(path,prefix,name);
    static const uint8_t payload[]="N27-PAYLOAD\n";uint8_t out[32]={0};
    R4FilePath parsed={0};int syntax=r4_file_path((const uint8_t *)path,length(path),&parsed);
    int32_t rc=store(a,path,payload,sizeof payload-1);
    text(a,"N27 vector=");text(a,label);text(a," path=");text(a,prefix);
    field(a," syntax=",syntax);field(a," bytes=",length(name));field(a," begin/finish=",rc);
    check(a,rc==expected,label);
    if(rc==0){
        int32_t got=read_file(a,path,out,sizeof out);field(a," read=",got);
        check(a,got==sizeof payload-1&&equal(out,payload,sizeof payload-1),"full payload readback");
        check(a,remove_file(a,path)==1,"successful fixture file cleanup");
    }else if(rc==R4OS_FILE_STREAM_ERROR_NAME_TOO_LONG) {
        R4FileNameLimits info;uint8_t *bytes=(uint8_t *)&info;
        for(uint32_t i=0;i<sizeof info;++i)bytes[i]=0xa5;
        int32_t query=r4sys_file_name_limits(&a->system,path,&info);
        check(a,query==expected,"component overflow query result");
        check(a,filled(&info,sizeof info,0xa5),"failed query output untouched");
    }
    text(a,"\n");
}
static void atomic_probe(R4App *a,const char *prefix,int long_stage,int long_target) {
    char target[1024],stage[1024],backup[1024];
    join(target,prefix,long_target?"N27-LONG-TARGET-DOCUMENT.DAT":"N27TARG.DAT");
    join(stage,prefix,long_stage?"N27-LONG-ATOMIC-STAGING-FILE.TMP":"N27STAGE.TMP");
    join(backup,prefix,"N27BACK.BAK");
    static const uint8_t old[]="OLD",data[]="N27-ATOMIC-PAYLOAD\n",guard[]="BACK";
    check(a,store(a,target,old,sizeof old-1)==0,"atomic target preparation");
    check(a,store(a,stage,data,sizeof data-1)==0,"atomic stage preparation");
    if(long_stage)check(a,store(a,backup,guard,sizeof guard-1)==0,"atomic backup sentinel");
    int32_t rc=((R4SysFileReplaceAtomicFn)(uintptr_t)a->system.table->file_replace_atomic)(
        (const uint8_t *)target,(const uint8_t *)stage,(const uint8_t *)backup,
        R4SYS_FILE_REPLACE_ATOMIC_CONSUME_STAGE);
    text(a,"N27 atomic path=");text(a,prefix);field(a," long-stage=",long_stage);
    field(a," long-target=",long_target);field(a," result=",rc);text(a,"\n");
    check(a,rc==(long_stage?R4OS_FILE_REPLACE_ATOMIC_ERROR_NAME_NOT_SUPPORTED:0),"atomic outcome");
    uint8_t out[64]={0};int32_t n=read_file(a,target,out,sizeof out);
    check(a,long_stage?(n==sizeof old-1&&equal(out,old,n)):(n==sizeof data-1&&equal(out,data,n)),
        "atomic target bytes");
    if(long_stage) {
        n=read_file(a,stage,out,sizeof out);
        check(a,n==sizeof data-1&&equal(out,data,n),"rejected stage unchanged");
        n=read_file(a,backup,out,sizeof out);
        check(a,n==sizeof guard-1&&equal(out,guard,n),"rejected backup unchanged");
    }
    remove_file(a,target);remove_file(a,stage);remove_file(a,backup);
}
static void suite(R4App *a,const char *prefix,uint32_t provider) {
    R4Files files={.system=&a->system};R4FilePath root={0};R4FileNameLimits info={0};
    check(a,r4_file_path((const uint8_t *)prefix,length(prefix),&root)==0,"query path syntax");
    int32_t rc=r4_files_name_limits(&files,&root,&info);
    text(a,"N27 limits path=");text(a,prefix);field(a," result=",rc);
    field(a," provider=",info.provider);field(a," flags=",info.flags);
    field(a," mount-slot=",info.mount_slot);field(a," mount-generation=",info.mount_generation);
    field(a," path-bytes=",info.max_path_bytes);field(a," name-bytes=",info.max_component_bytes);
    field(a," utf16-units=",info.max_component_utf16_units);field(a," atomic=",info.atomic_rules);text(a,"\n");
    check(a,rc==0&&info.size==56&&info.flags==3&&info.provider==provider,"actual provider snapshot");
    check(a,info.mount_generation!=0&&info.max_path_bytes==1023&&info.max_component_bytes==765
        &&info.max_component_utf16_units==255&&info.short_base_bytes==8&&info.short_extension_bytes==3,"limits units");
    check(a,info.name_rules==(provider==2?7u:8u)&&info.atomic_rules==(provider==2?7u:3u),"operation rules");
    char missing[1024];join(missing,prefix,"N27-NOT-CREATED-ANYWHERE.DAT");
    R4FileNameLimits other={0};
    check(a,r4sys_file_name_limits(&a->system,missing,&other)==0&&other.mount_slot==info.mount_slot
        &&other.mount_generation==info.mount_generation,"query requires no existing file");
    probe(a,prefix,"short","N27SHORT.TMP",0);
    probe(a,prefix,"long","N27-SOLITAIRE-UNIQUE-STAGING-0123456789.TMP",0);
    probe(a,prefix,"leading-dot",".N27-unique-staging.tmp",provider==2?R4OS_FILE_STREAM_ERROR_NAME_NOT_SUPPORTED:0);
    probe(a,prefix,"plus","N27+STAG.TMP",0);
    char name[769];for(uint32_t i=0;i<256;++i)name[i]=i<3?"N27"[i]:'X';
    name[255]=0;probe(a,prefix,"ascii255",name,0);
    name[255]='X';name[256]=0;probe(a,prefix,"ascii256",name,R4OS_FILE_STREAM_ERROR_NAME_TOO_LONG);
    for(uint32_t i=0;i<256;++i){name[i*3]=(char)0xe2;name[i*3+1]=(char)0x82;name[i*3+2]=(char)0xac;}
    name[765]=0;probe(a,prefix,"euro255/765bytes",name,0);
    name[765]=(char)0xe2;name[766]=(char)0x82;name[767]=(char)0xac;name[768]=0;
    probe(a,prefix,"euro256/768bytes",name,R4OS_FILE_STREAM_ERROR_NAME_TOO_LONG);
    for(uint32_t i=0;i<128;++i){name[i*4]=(char)0xf0;name[i*4+1]=(char)0x9f;name[i*4+2]=(char)0x99;name[i*4+3]=(char)0x82;}
    name[508]='X';name[509]=0;probe(a,prefix,"supplementary/255units",name,0);
    name[508]=(char)0xf0;name[509]=(char)0x9f;name[510]=(char)0x99;name[511]=(char)0x82;name[512]=0;
    probe(a,prefix,"supplementary/256units",name,R4OS_FILE_STREAM_ERROR_NAME_TOO_LONG);
    atomic_probe(a,prefix,0,0);atomic_probe(a,prefix,1,0);atomic_probe(a,prefix,0,1);
    char saved[1024];join(saved,prefix,"N27-LONG-DOCUMENT-FACADE.DAT");
    static const uint8_t data[]="N27-SDK-SAFE-STAGING\n";uint8_t out[64]={0};uint32_t sequence=0;
    check(a,store(a,saved,(const uint8_t *)"OLD",3)==0,"SDK save target preparation");
    rc=R4SdkDocumentSave(&a->system,&sequence,(const uint8_t *)saved,length(saved),data,sizeof data-1,0);
    check(a,rc==R4_SAVE_SAVED,"SDK safe unique short staging");
    int32_t n=read_file(a,saved,out,sizeof out);
    check(a,n==sizeof data-1&&equal(out,data,n),"SDK long target complete readback");
    remove_file(a,saved);
}
int32_t r4_app_main(R4App *a) {
    text(a,"N27-BEGIN SDK109\n");
    for(uint32_t index=0;index<26;++index){
        R4DriveInfo drive={0};
        int32_t rc=((R4SysDriveInfoGeometryFn)(uintptr_t)a->system.table->drive_info_geometry)(index,&drive);
        if(rc==1&&(drive.kind==2||drive.kind==3)){
            char prefix[]="C:\\";prefix[0]=(char)drive.letter;suite(a,prefix,drive.kind);
        }
    }
    suite(a,"C:\\BOOT\\",2);
    R4FileNameLimits untouched;uint8_t *bytes=(uint8_t *)&untouched;
    for(uint32_t i=0;i<sizeof untouched;++i)bytes[i]=0xa5;
    check(a,r4sys_file_name_limits(&a->system,0,&untouched)==R4OS_FILE_STREAM_ERROR_INVALID,"null query path");
    check(a,filled(&untouched,sizeof untouched,0xa5),"null path preserves output");
    /* No directory lookup is required for this boundary query. Every segment
     * fits 255 UTF16 units; the complete visible path is exactly 1023 bytes. */
    char boundary[1025];boundary[0]='C';boundary[1]=':';boundary[2]='\\';
    for(uint32_t i=3;i<1023;++i)boundary[i]=((i-3)%256==255)?'\\':'A';
    boundary[1023]=0;R4FileNameLimits limit={0};
    check(a,r4sys_file_name_limits(&a->system,boundary,&limit)==0,"exact 1023-byte path query");
    boundary[1023]='A';boundary[1024]=0;
    check(a,r4sys_file_name_limits(&a->system,boundary,&untouched)==R4OS_FILE_STREAM_ERROR_NAME_TOO_LONG,
        "1024-byte path query overflow");
    char overlong[1100];for(uint32_t i=0;i<1099;++i)overlong[i]='X';overlong[1099]=0;
    int32_t rc=((R4SysFileStreamBeginFn)(uintptr_t)a->system.table->file_stream_begin)(
        (const uint8_t *)overlong,R4OS_FILE_STREAM_OPEN_CREATE);
    check(a,rc==R4OS_FILE_STREAM_ERROR_NAME_TOO_LONG,"raw path byte overflow");
    check(a,r4sys_file_name_limits(&a->system,overlong,&untouched)==R4OS_FILE_STREAM_ERROR_NAME_TOO_LONG,"query raw path overflow");
    check(a,filled(&untouched,sizeof untouched,0xa5),"overflow preserves output");
    field(a,"N27-DONE checks=",cases);field(a," failures=",failures);text(a,"\n");
    return failures?1:0;
}
