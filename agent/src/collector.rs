use serde_json::{json, Value};
use std::{collections::{HashMap, HashSet, VecDeque}, ffi::CStr, time::{Instant, SystemTime, UNIX_EPOCH}};
use sysinfo::{System, Networks, Disks, DiskRefreshKind};

#[repr(C)]
struct NativeProc { footprint:u64, read:u64, write:u64, start_us:u64, rss:u64,cpu_ns:u64, threads:i32, state:i32, has_usage:i32,ppid:i32,uid:i32,name:[libc::c_char;64],exe:[libc::c_char;4096] }
#[repr(C)]
struct NativeMem { compressed:u64,wired:u64,active:u64,inactive:u64,free:u64,purgeable:u64,pressure:i32,available:i32 }
#[repr(C)]
struct NativeBattery { charge:f64,charging:f64,external:f64,cycles:f64,health:f64,remaining:f64 }
unsafe extern "C" { fn sm_process(pid:i32,out:*mut NativeProc)->i32; fn sm_memory(out:*mut NativeMem); fn sm_battery(out:*mut NativeBattery)->i32; fn sm_thermal()->i32;fn sm_list(pids:*mut i32,count:i32)->i32;fn sm_metadata(pid:i32,cwd:*mut libc::c_char,command:*mut libc::c_char,cap:i32); }
pub fn now()->f64 { SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs_f64() }
pub fn action_process(id:&str)->Result<Value,String>{
 let (pid,start)=id.split_once(':').ok_or("Некорректный ID процесса")?;let pid:i32=pid.parse().map_err(|_|"Некорректный PID")?;let start:u64=start.parse().map_err(|_|"Некорректное время запуска")?;
 if pid<=1||pid==std::process::id() as i32||start==0{return Err("Служебный процесс защищён".into())}
 let mut n:NativeProc=unsafe{std::mem::zeroed()};if unsafe{sm_process(pid,&mut n)}==0||n.start_us!=start{return Err("Процесс завершился или PID уже принадлежит другому процессу".into())}
 let exe=unsafe{CStr::from_ptr(n.exe.as_ptr())}.to_string_lossy().into_owned();let name=unsafe{CStr::from_ptr(n.name.as_ptr())}.to_string_lossy().into_owned();
 if n.uid<501||n.uid as u32!=unsafe{libc::geteuid()}||exe.is_empty()||exe.starts_with("/System/")||exe.starts_with("/usr/libexec/")||exe.starts_with("/usr/sbin/")||["docker","com.docker","virtualization","system-monitor","windowserver","loginwindow","launchd"].iter().any(|s|format!("{exe} {name}").to_lowercase().contains(s)){return Err("Системный процесс, Docker VM или процесс другого пользователя защищён".into())}
 // Also protect the agent's ancestors, including its launcher.
 let mut ancestor=unsafe{libc::getppid()};for _ in 0..32{if ancestor<=1{break}if pid==ancestor{return Err("Процесс запуска монитора защищён".into())}let mut parent:NativeProc=unsafe{std::mem::zeroed()};if unsafe{sm_process(ancestor,&mut parent)}==0{break}ancestor=parent.ppid;}
 Ok(json!({"id":id,"pid":pid,"name":name,"executable":exe,"memory":n.footprint,"signal":"SIGTERM"}))
}
pub fn terminate_process(id:&str)->Result<(),String>{let p=action_process(id)?;let pid=p["pid"].as_i64().ok_or("PID unavailable")? as i32;if unsafe{libc::kill(pid,libc::SIGTERM)}!=0{return Err(std::io::Error::last_os_error().to_string())}Ok(())}
fn opt(n:f64)->Option<f64>{ if n>=0.0 && n.is_finite(){Some(n)}else{None} }
pub fn classify(name:&str,cmd:&str)->(String,bool,bool){
 if name=="system-monitor-agent"{return ("System Monitor Agent".into(),false,false)}
 let s=format!("{} {}",name,cmd).to_lowercase();
 let node=["node","npm","npx","yarn","pnpm","bun","vite","next","webpack","rollup","esbuild","tsserver","eslint","prettier","jest","vitest","playwright","electron"].iter().any(|x|name.to_lowercase().contains(x)||s.contains(&format!("/{x}"))||s.contains(&format!(" {x}")));
 let labels=[("tsserver","TypeScript Server"),("vite","Vite dev server"),("next","Next.js"),("webpack","Webpack"),("rollup","Rollup"),("esbuild","esbuild"),("eslint","ESLint"),("prettier","Prettier"),("vitest","Vitest"),("jest","Jest"),("playwright","Playwright"),("codex","Codex"),("cursor","Cursor"),("visual studio code","VS Code"),("code helper","VS Code"),("docker","Docker"),("postgres","PostgreSQL"),("redis","Redis"),("python","Python"),("java","Java"),("terminal","Terminal"),("git","Git")];
 let label=labels.iter().find(|(key,_)|s.contains(key)).map(|(_,v)|v.to_string()).unwrap_or_else(||if node{"Node.js".into()}else{name.into()});
 let dev=node||labels.iter().any(|(key,_)|s.contains(key));(label,dev,node)
}
pub struct Collector { system:System,networks:Networks,disks:Disks,metadata:HashMap<String,(Instant,String,String)>,cpu_previous:HashMap<String,u64>,last_disks:Instant,user_cache:HashMap<i32,String>,previous:HashMap<String,(u64,u64)>,last:Instant,first:bool,write_history:HashMap<String,VecDeque<(f64,u64)>> }
impl Collector {
 pub fn new()->Self {Self{system:System::new(),networks:Networks::new_with_refreshed_list(),disks:Disks::new_with_refreshed_list(),metadata:HashMap::new(),cpu_previous:HashMap::new(),last_disks:Instant::now(),user_cache:HashMap::new(),previous:HashMap::new(),last:Instant::now(),first:true,write_history:HashMap::new()}}
 pub fn sample(&mut self)->(Value,Vec<Value>){
 let dt=self.last.elapsed().as_secs_f64(); self.last=Instant::now();
 self.system.refresh_cpu_usage();self.system.refresh_memory();
 self.networks.refresh(true);if self.last_disks.elapsed().as_secs()>=30{self.disks.refresh(true);self.last_disks=Instant::now();}else{for d in self.disks.list_mut(){d.refresh_specifics(DiskRefreshKind::nothing().with_io_usage());}}
 let ts=now();let mut rows=vec![];let mut identities=HashSet::new();
 let mut pids=vec![0i32;8192];let count=unsafe{sm_list(pids.as_mut_ptr(),pids.len() as i32)}.max(0) as usize;
 for pid in pids.into_iter().take(count){
  let mut n:NativeProc=unsafe{std::mem::zeroed()};let ok=unsafe{sm_process(pid,&mut n)}!=0;
  if !ok {continue}
  let start=n.start_us;let id=format!("{pid}:{start}");identities.insert(id.clone());
  let old=self.previous.get(&id).copied();let usage=ok&&n.has_usage!=0;
  let rate=|current:u64,previous:u64|->Option<f64>{if current>=previous && dt>0.0{Some((current-previous) as f64/dt)}else{None}};
  let read=if usage{old.and_then(|v|rate(n.read,v.0))}else{None};let write=if usage{old.and_then(|v|rate(n.write,v.1))}else{None};
  let mut windows=serde_json::Map::new();
  if usage {let history=self.write_history.entry(id.clone()).or_default();
   if history.back().is_none_or(|v|ts-v.0>=5.0){history.push_back((ts,n.write));}
   while history.front().is_some_and(|v|ts-v.0>905.0){history.pop_front();}
   for seconds in [60,300,900]{let baseline=history.iter().find(|v|v.0>=ts-seconds as f64);if let Some((t,total))=baseline{windows.insert(seconds.to_string(),json!({"bytes":n.write.saturating_sub(*total),"observed_seconds":ts-t}));}}
  }
  if usage {self.previous.insert(id.clone(),(n.read,n.write));}
  let name=unsafe{CStr::from_ptr(n.name.as_ptr())}.to_string_lossy().into_owned();
  let exe=Some(unsafe{CStr::from_ptr(n.exe.as_ptr())}.to_string_lossy().into_owned());
  if self.metadata.get(&id).is_none_or(|v|v.0.elapsed().as_secs()>30){let mut cwd=[0;1024];let mut cmd=[0;8192];unsafe{sm_metadata(pid,cwd.as_mut_ptr(),cmd.as_mut_ptr(),8192)};self.metadata.insert(id.clone(),(Instant::now(),unsafe{CStr::from_ptr(cwd.as_ptr())}.to_string_lossy().into_owned(),unsafe{CStr::from_ptr(cmd.as_ptr())}.to_string_lossy().into_owned()));}
  let (_,cwd,cmd)=self.metadata.get(&id).unwrap();let cwd=cwd.clone();let cmd=cmd.clone();
  let (label,dev,node)=classify(&name,&cmd);
  let app=exe.as_ref().and_then(|s|s.split(".app/").next().filter(|_|s.contains(".app/")).and_then(|v|v.rsplit('/').next())).map(str::to_owned);
  let cpu=if n.threads==0{None}else{self.cpu_previous.insert(id.clone(),n.cpu_ns).and_then(|prev|if n.cpu_ns>=prev && dt>0.0{Some((n.cpu_ns-prev) as f64/1e9/dt*100.0)}else{None})};
  let user=self.user_cache.entry(n.uid).or_insert_with(||unsafe{let u=libc::getpwuid(n.uid as u32);if u.is_null(){n.uid.to_string()}else{CStr::from_ptr((*u).pw_name).to_string_lossy().into_owned()}}).clone();
  let manage_allowed=action_process(&id).is_ok();
  rows.push(json!({"manage_allowed":manage_allowed,"id":id,"pid":pid,"ppid":n.ppid,"name":name,"label":label,"command":cmd,"executable":exe,"cwd":if cwd.is_empty(){None}else{Some(&cwd)},"project":if dev && !cwd.is_empty() && cwd!="/"{Some(cwd.clone())}else{None},"user":user,"cpu":cpu,"rss":if n.threads>0{Some(n.rss)}else{None},"footprint":if usage{Some(n.footprint)}else{None},"memory":if usage{Some(n.footprint)}else if n.threads>0{Some(n.rss)}else{None},"memory_kind":if usage{"footprint"}else{"rss"},"threads":if n.threads>0{Some(n.threads)}else{None},"start":start as f64/1e6,"uptime":(ts-start as f64/1e6).max(0.0),"state":match n.state{1=>"Idle",2=>"Runnable",3=>"Sleeping",4=>"Stopped",5=>"Zombie",_=>"Unknown"},"read":read,"write":write,"write_windows":windows,"read_total":if usage{Some(n.read)}else{None},"write_total":if usage{Some(n.write)}else{None},"network":Value::Null,"development":dev,"node":node,"application":app}));
 }
 self.previous.retain(|k,_|identities.contains(k));self.metadata.retain(|k,_|identities.contains(k));self.cpu_previous.retain(|k,_|identities.contains(k));self.write_history.retain(|k,_|identities.contains(k));
 // Attribute each PID exactly once to its outermost observed application ancestor.
 let lookup:HashMap<u64,(Option<u64>,Option<String>)>=rows.iter().map(|p|(p["pid"].as_u64().unwrap(),(p["ppid"].as_u64(),p["application"].as_str().map(str::to_owned)))).collect();
 for p in &mut rows {let mut ancestor=p["pid"].as_u64().unwrap();let mut group=None;let mut seen=HashSet::new();while seen.insert(ancestor){if let Some((parent,app))=lookup.get(&ancestor){if let Some(app)=app{if group.is_none(){group=Some(app.clone());}else if group.as_ref()!=Some(app){break}}if let Some(parent)=parent{ancestor=*parent;}else{break}}else{break}}
 p["group"]=json!(group.unwrap_or_else(||p["label"].as_str().unwrap_or("System").to_owned())); }
 let mut mem:NativeMem=unsafe{std::mem::zeroed()};unsafe{sm_memory(&mut mem)};
 let mut b:NativeBattery=unsafe{std::mem::zeroed()};let battery=if unsafe{sm_battery(&mut b)}!=0{json!({"charge":opt(b.charge),"charging":opt(b.charging).map(|x|x>0.0),"external":opt(b.external).map(|x|x>0.0),"cycles":opt(b.cycles),"health":opt(b.health),"remaining":if b.remaining<65535.0{opt(b.remaining)}else{None}})}else{Value::Null};
 let interfaces:Vec<_>=self.networks.iter().filter(|(name,_)|!name.starts_with("lo")).map(|(name,n)|json!({"name":name,"kind":if name.starts_with("utun"){"VPN"}else if name.starts_with("en"){"Network"}else{"Interface"},"download":if self.first{None}else{Some(n.received() as f64/dt)},"upload":if self.first{None}else{Some(n.transmitted() as f64/dt)},"received":n.total_received(),"transmitted":n.total_transmitted()})).collect();
 let disks:Vec<_>=self.disks.iter().map(|d|{let u=d.usage();json!({"name":d.name().to_string_lossy(),"mount":d.mount_point().to_string_lossy(),"total":d.total_space(),"free":d.available_space(),"read":if self.first{None}else{Some(u.read_bytes as f64/dt)},"write":if self.first{None}else{Some(u.written_bytes as f64/dt)}})}).collect();
 let sum=|key:&str|->Option<f64>{if self.first{None}else{Some(interfaces.iter().filter_map(|n|n[key].as_f64()).sum())}};
 let disk=disks.iter().find(|d|d["mount"]=="/System/Volumes/Data").or_else(||disks.iter().find(|d|d["mount"]=="/")).cloned().unwrap_or(Value::Null);
 let result=json!({"ts":ts,"interval":dt,"cpu":if self.first{None}else{Some(self.system.global_cpu_usage())},"cores":self.system.cpus().iter().map(|c|if self.first{None}else{Some(c.cpu_usage())}).collect::<Vec<_>>(),"memory":{"total":self.system.total_memory(),"used":self.system.used_memory(),"available":if mem.available!=0{Some(mem.free+mem.inactive)}else{None},"available_note":"Оценка: free + inactive pages","swap":self.system.used_swap(),"swap_total":self.system.total_swap(),"compressed":if mem.available!=0{Some(mem.compressed)}else{None},"wired":if mem.available!=0{Some(mem.wired)}else{None},"pressure":match mem.pressure{1=>"normal",2=>"warning",4=>"critical",_=>"unavailable"}},"network":{"download":sum("download"),"upload":sum("upload"),"interfaces":interfaces,"note":"Сумма интерфейсов; VPN может повторно учитывать трафик"},"disk":disk,"disks":disks,"battery":battery,"machine":{"model":System::name(),"os":System::long_os_version(),"chip":self.system.cpus().first().map(|c|c.brand()),"cores":self.system.cpus().len()},"thermal_state":match unsafe{sm_thermal()}{0=>"nominal",1=>"fair",2=>"serious",3=>"critical",_=>"unavailable"},"process_count":rows.len()});
 self.first=false;(result,rows)
 }
}
#[cfg(test)] mod tests {use super::*;#[test]fn classifies_real_command_patterns(){assert_eq!(classify("node","node /project/node_modules/vite/bin/vite.js").0,"Vite dev server");assert!(classify("node","/usr/bin/node tsserver.js").2);assert!(!classify("WindowServer","").1);}
 #[test]fn only_exact_owned_disposable_process_can_be_terminated(){
  let mut child=std::process::Command::new("/bin/sleep").arg("30").spawn().unwrap();let pid=child.id() as i32;let mut native:NativeProc=unsafe{std::mem::zeroed()};assert_ne!(unsafe{sm_process(pid,&mut native)},0);let id=format!("{pid}:{}",native.start_us);
  assert!(action_process(&format!("{pid}:{}",native.start_us+1)).is_err());assert!(action_process("1:1").is_err());assert!(action_process(&format!("{}:1",std::process::id())).is_err());
  let result=terminate_process(&id);if result.is_err(){let _=child.kill();}assert!(result.is_ok(),"{result:?}");let status=child.wait().unwrap();assert!(!status.success());assert!(action_process(&id).is_err());
 }
}
