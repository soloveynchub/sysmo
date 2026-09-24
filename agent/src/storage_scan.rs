use serde_json::{json,Value};
use std::{collections::{HashSet,VecDeque,BTreeMap},path::PathBuf,sync::{Arc,Mutex,atomic::{AtomicBool,Ordering}},time::{Instant,Duration},os::unix::fs::MetadataExt};
struct Category {name:String,path:PathBuf,dev:u64,queue:VecDeque<(PathBuf,usize)>,size:u64,items:BTreeMap<String,u64>,errors:usize}
struct Job {roots:Vec<Category>,links:HashSet<(u64,u64)>,files:Vec<Value>,visited:usize,started:f64}
impl Job {
 fn new()->Self {
  let home=PathBuf::from(std::env::var("HOME").unwrap_or_default());
  let paths=[("Документы",home.join("Documents")),("Загрузки",home.join("Downloads")),("Рабочий стол",home.join("Desktop")),("Приложения",PathBuf::from("/Applications")),("Видео",home.join("Movies")),("Изображения",home.join("Pictures")),("Библиотека",home.join("Library")),("Проекты",home.join("Projects")),("Общая библиотека",PathBuf::from("/Library")),("Служебные данные",PathBuf::from("/private")),("Инструменты /opt",PathBuf::from("/opt")),("Инструменты Unix",PathBuf::from("/System/Volumes/Data/usr")),("Системные данные",PathBuf::from("/System/Volumes/Data/System"))];
  let known:HashSet<PathBuf>=paths.iter().map(|(_,p)|p.clone()).collect();
  let mut roots:Vec<_>=paths.into_iter().filter_map(|(name,path)|{match std::fs::symlink_metadata(&path){Ok(m) if !m.is_symlink()=>Some(Category{name:name.into(),path:path.clone(),dev:m.dev(),queue:VecDeque::from([(path,0)]),size:0,items:BTreeMap::new(),errors:0}),Err(e) if e.kind()==std::io::ErrorKind::NotFound=>None,_=>Some(Category{name:name.into(),path,dev:0,queue:VecDeque::new(),size:0,items:BTreeMap::new(),errors:1})}}).collect();
  if let Ok(m)=std::fs::metadata(&home){let mut other=Category{name:"Другие файлы пользователя".into(),path:home.clone(),dev:m.dev(),queue:VecDeque::new(),size:0,items:BTreeMap::new(),errors:0};match std::fs::read_dir(&home){Ok(entries)=>for e in entries{match e{Ok(e) if !known.contains(&e.path())=>other.queue.push_back((e.path(),0)),Err(_)=>other.errors+=1,_=>{}}},Err(_)=>other.errors+=1}roots.push(other);}
  let data_root=PathBuf::from("/System/Volumes/Data");
  let exclusions=["Applications","Library","Users","private","opt","usr","System"].iter().map(|n|data_root.join(n)).collect::<Vec<_>>();
  for(name,path,excluded)in [("Другие пользователи",PathBuf::from("/Users"),vec![home.clone()]),("Другие данные APFS",data_root,exclusions)]{
   if let Ok(m)=std::fs::metadata(&path){let mut c=Category{name:name.into(),path:path.clone(),dev:m.dev(),queue:VecDeque::new(),size:0,items:BTreeMap::new(),errors:0};match std::fs::read_dir(&path){Ok(entries)=>for e in entries{match e{Ok(e)if !excluded.contains(&e.path())=>c.queue.push_back((e.path(),0)),Err(_)=>c.errors+=1,_=>{}}},Err(_)=>c.errors+=1}roots.push(c);}
  }
  Self{roots,links:HashSet::new(),files:vec![],visited:0,started:crate::collector::now()}
 }
 fn pending(&self)->bool{self.roots.iter().any(|r|!r.queue.is_empty())}
 fn snapshot(&self,status:&str)->Value {
  let mut categories:Vec<_>=self.roots.iter().map(|c|{let mut items:Vec<_>=c.items.iter().map(|(n,s)|json!({"name":n,"size":s})).collect();items.sort_by_key(|v|std::cmp::Reverse(v["size"].as_u64().unwrap_or(0)));items.truncate(100);json!({"name":c.name,"path":c.path,"size":c.size,"items":items,"partial":!c.queue.is_empty()||c.errors>0,"errors":c.errors,"complete":c.queue.is_empty()})}).collect();categories.sort_by_key(|v|std::cmp::Reverse(v["size"].as_u64().unwrap_or(0)));
  json!({"status":status,"categories":categories,"files":self.files,"visited":self.visited,"errors":self.roots.iter().map(|r|r.errors).sum::<usize>(),"started":self.started,"ts":crate::collector::now(),"interval_hours":6})
 }
}
// Metadata only, round-robin across categories. No whole-scan time or item cutoff.
pub fn run(state:Arc<Mutex<Value>>,cancel:Arc<AtomicBool>,cache:PathBuf){
 let mut j=Job::new();let mut cursor=0;let mut published=Instant::now();
 *state.lock().unwrap()=j.snapshot("scanning");
 while j.pending()&&!cancel.load(Ordering::SeqCst){
  let index=cursor%j.roots.len();cursor+=1;let c=&mut j.roots[index];let Some((path,depth))=c.queue.pop_back()else{continue};j.visited+=1;
  let m=match std::fs::symlink_metadata(&path){Ok(m)=>m,Err(_)=>{c.errors+=1;continue}};
  if m.is_symlink()||m.dev()!=c.dev{continue}
  if m.is_file()&&m.nlink()>1&&!j.links.insert((m.dev(),m.ino())){continue}
  let allocated=m.blocks().saturating_mul(512);c.size=c.size.saturating_add(allocated);
  let child=path.strip_prefix(&c.path).ok().and_then(|p|p.components().next()).map(|s|s.as_os_str().to_string_lossy().into_owned()).unwrap_or_else(||"Сама папка".into());*c.items.entry(child).or_default()+=allocated;
  if m.is_dir(){if depth>=128{c.errors+=1;continue}match std::fs::read_dir(&path){Ok(entries)=>for e in entries{if cancel.load(Ordering::SeqCst){break}match e{Ok(e)=>c.queue.push_back((e.path(),depth+1)),Err(_)=>c.errors+=1}},Err(_)=>c.errors+=1}}
  else if m.is_file()&&allocated>10*1024*1024{j.files.push(json!({"name":path.file_name().unwrap_or_default().to_string_lossy(),"path":path,"size":allocated,"logical":m.len()}));j.files.sort_by_key(|v|std::cmp::Reverse(v["size"].as_u64().unwrap_or(0)));j.files.truncate(50)}
  if j.visited%200==0{std::thread::sleep(Duration::from_millis(10));}
  if published.elapsed()>Duration::from_millis(750){*state.lock().unwrap()=j.snapshot("scanning");published=Instant::now();}
 }
 let status=if cancel.load(Ordering::SeqCst){"cancelled"}else if j.roots.iter().any(|r|r.errors>0){"partial"}else{"complete"};let result=j.snapshot(status);*state.lock().unwrap()=result.clone();
 let tmp=cache.with_extension("tmp");if let Ok(bytes)=serde_json::to_vec(&result){if std::fs::write(&tmp,bytes).is_ok(){let _=std::fs::rename(tmp,cache);}}
}
