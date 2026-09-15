#include <libproc.h>
#include <sys/sysctl.h>
#include <sys/proc_info.h>
#include <sys/resource.h>
#include <mach/mach.h>
#include <mach/mach_time.h>
#include <IOKit/IOKitLib.h>
#include <CoreFoundation/CoreFoundation.h>
#include <string.h>
#include <stdlib.h>
#include <stdio.h>

struct sm_proc { uint64_t footprint, read, write, start_us, rss, cpu_ns; int threads, state, has_usage, ppid, uid; char name[64]; char exe[4096]; };
int sm_process(int pid, struct sm_proc *out) {
 memset(out, 0, sizeof(*out));
 struct proc_bsdinfo b;
 if (proc_pidinfo(pid, PROC_PIDTBSDINFO, 0, &b, sizeof(b)) != sizeof(b)) {
  struct kinfo_proc k;size_t ks=sizeof(k);int mib[4]={CTL_KERN,KERN_PROC,KERN_PROC_PID,pid};
  if(sysctl(mib,4,&k,&ks,NULL,0)||!ks)return 0;
  memset(&b,0,sizeof(b));b.pbi_start_tvsec=k.kp_proc.p_starttime.tv_sec;b.pbi_start_tvusec=k.kp_proc.p_starttime.tv_usec;b.pbi_status=k.kp_proc.p_stat;b.pbi_ppid=k.kp_eproc.e_ppid;b.pbi_uid=k.kp_eproc.e_ucred.cr_uid;strncpy(b.pbi_name,k.kp_proc.p_comm,sizeof(b.pbi_name)-1);
 }
 out->start_us = b.pbi_start_tvsec * 1000000 + b.pbi_start_tvusec;
 out->state = b.pbi_status; out->ppid=b.pbi_ppid;out->uid=b.pbi_uid;strncpy(out->name,b.pbi_name[0]?b.pbi_name:b.pbi_comm,63);proc_pidpath(pid,out->exe,4096);
 struct proc_taskinfo t;
 if (proc_pidinfo(pid, PROC_PIDTASKINFO, 0, &t, sizeof(t)) == sizeof(t)) {out->threads = t.pti_threadnum;out->rss=t.pti_resident_size;static mach_timebase_info_data_t tb={0,0};if(!tb.denom)mach_timebase_info(&tb);out->cpu_ns=(uint64_t)((long double)(t.pti_total_user+t.pti_total_system)*tb.numer/tb.denom);}
 struct rusage_info_v2 r;
 if (proc_pid_rusage(pid, RUSAGE_INFO_V2, (rusage_info_t *)&r) == 0) {
  out->has_usage = 1; out->footprint=r.ri_phys_footprint; out->read=r.ri_diskio_bytesread; out->write=r.ri_diskio_byteswritten;
 }
 return 1;
}
struct sm_memory { uint64_t compressed, wired, active, inactive, free, purgeable; int pressure, available; };
void sm_memory(struct sm_memory *out) {
 memset(out,0,sizeof(*out)); out->pressure=-1;
 size_t sz=sizeof(out->pressure);
 if(sysctlbyname("kern.memorystatus_vm_pressure_level", &out->pressure,&sz,NULL,0)) out->pressure=-1;
 vm_statistics64_data_t v; mach_msg_type_number_t n=HOST_VM_INFO64_COUNT;
 mach_port_t host=mach_host_self();
 if(host_statistics64(host,HOST_VM_INFO64,(host_info64_t)&v,&n)==KERN_SUCCESS) {
 vm_size_t page; host_page_size(host,&page); out->available=1;
 out->compressed=(uint64_t)v.compressor_page_count*page;
 out->wired=(uint64_t)v.wire_count*page;out->active=(uint64_t)v.active_count*page;
 out->inactive=(uint64_t)v.inactive_count*page;out->free=(uint64_t)v.free_count*page;out->purgeable=(uint64_t)v.purgeable_count*page;
 }
 mach_port_deallocate(mach_task_self(),host);
}
static double number(CFDictionaryRef d, CFStringRef key) {
 CFTypeRef v=CFDictionaryGetValue(d,key); double n=-1;
 if(v && CFGetTypeID(v)==CFNumberGetTypeID()) CFNumberGetValue(v,kCFNumberDoubleType,&n);
 if(v && CFGetTypeID(v)==CFBooleanGetTypeID()) n=CFBooleanGetValue(v);
 return n;
}
struct sm_battery { double charge, charging, external, cycles, health, remaining; };
int sm_battery(struct sm_battery *out) {
 *out=(struct sm_battery){-1,-1,-1,-1,-1,-1};
 io_service_t s=IOServiceGetMatchingService(kIOMainPortDefault,IOServiceMatching("AppleSmartBattery"));
 if(!s) return 0;
 CFMutableDictionaryRef d=NULL; kern_return_t ret=IORegistryEntryCreateCFProperties(s,&d,kCFAllocatorDefault,0); IOObjectRelease(s);
 if(ret!=KERN_SUCCESS || !d) return 0;
 out->charge=number(d,CFSTR("CurrentCapacity"));out->charging=number(d,CFSTR("IsCharging"));out->external=number(d,CFSTR("ExternalConnected"));
 out->cycles=number(d,CFSTR("CycleCount"));out->remaining=number(d,CFSTR("TimeRemaining"));
 double max=number(d,CFSTR("AppleRawMaxCapacity")),design=number(d,CFSTR("DesignCapacity"));
 if(max>=0 && design>0) out->health=max/design*100;
 CFRelease(d);return 1;
}

int sm_list(int *pids,int count){return proc_listallpids(pids,count*sizeof(int));}
void sm_metadata(int pid,char *cwd,char *command,int cap){
 cwd[0]=0;command[0]=0;
 struct proc_vnodepathinfo v;
 if(proc_pidinfo(pid,PROC_PIDVNODEPATHINFO,0,&v,sizeof(v))==sizeof(v))strncpy(cwd,v.pvi_cdir.vip_path,1023);
 int mib[3]={CTL_KERN,KERN_PROCARGS2,pid};size_t size=1024*1024;char *args=malloc(size);
 if(!args)return;
 if(sysctl(mib,3,args,&size,NULL,0)==0 && size>sizeof(int)){
  int argc=0;memcpy(&argc,args,sizeof(int));char *p=args+sizeof(int),*end=args+size;
  while(p<end && *p)p++;while(p<end && !*p)p++;int used=0;
  for(int i=0;i<argc && p<end;i++){while(p<end && *p){if(used<cap-2)command[used++]=*p;p++;}if(used<cap-2)command[used++]=' ';p++;}command[used]=0;
 }free(args);
}
