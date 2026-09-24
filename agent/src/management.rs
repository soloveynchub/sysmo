use axum::{extract::State, http::StatusCode, Json};
use serde_json::{json, Value};
use std::{collections::HashMap, io::Read, sync::{Arc,Mutex}, time::{Duration,Instant}, path::PathBuf};

pub type Error=(StatusCode,Json<Value>);
fn error(message:impl ToString)->Error {(StatusCode::CONFLICT,Json(json!({"error":message.to_string()})))}
fn nonce()->String {let mut b=[0u8;24];std::fs::File::open("/dev/urandom").expect("random source").read_exact(&mut b).expect("random bytes");b.iter().map(|x|format!("{x:02x}")).collect()}
#[derive(Clone)]
struct Plan {created:Instant,action:String,id:String,target:Value,socket:Option<PathBuf>,daemon:String}
pub struct Management {pub token:String,inventory:tokio::sync::Mutex<(Instant,Option<Value>)>,cache:PathBuf,plans:Mutex<HashMap<String,Plan>>,serial:tokio::sync::Mutex<()>,journal:Mutex<Vec<Value>>,pub scan:Arc<Mutex<Value>>,pub scanning:std::sync::atomic::AtomicBool,pub cancel:Arc<std::sync::atomic::AtomicBool>}
impl Management {pub fn new(cache:PathBuf)->Self {let cached=std::fs::read(&cache).ok().and_then(|v|serde_json::from_slice::<Value>(&v).ok()).unwrap_or_else(||json!({"status":"idle","categories":[],"files":[]}));Self{token:nonce(),inventory:tokio::sync::Mutex::new((Instant::now(),None)),cache,plans:Mutex::new(HashMap::new()),serial:tokio::sync::Mutex::new(()),journal:Mutex::new(vec![]),scan:Arc::new(Mutex::new(cached)),scanning:false.into(),cancel:Arc::new(false.into())}}}
#[derive(serde::Deserialize)]#[serde(deny_unknown_fields)]pub struct Request {action:String,id:String}
#[derive(serde::Deserialize)]#[serde(deny_unknown_fields)]pub struct Execute {plan:String}
pub async fn session(State(a):State<crate::App>)->Json<Value>{Json(json!({"token":a.management.token,"journal":a.management.journal.lock().unwrap().clone()}))}
pub async fn inventory(State(a):State<crate::App>)->Result<Json<Value>,Error>{
 let mut cache=a.management.inventory.lock().await;if cache.0.elapsed()<Duration::from_secs(10){if let Some(v)=&cache.1{return Ok(Json(v.clone()))}}
 let value=crate::docker::inventory().await.map_err(error)?;*cache=(Instant::now(),Some(value.clone()));Ok(Json(value))
}
pub async fn plan(State(a):State<crate::App>,Json(r):Json<Request>)->Result<Json<Value>,Error>{
 let (target,socket,daemon)=if r.action=="process.terminate" {
   (crate::collector::action_process(&r.id).map_err(error)?,None,String::new())
 }else if matches!(r.action.as_str(),"container.remove"|"container.stop") {
   if !valid_container_id(&r.id){return Err(error("Некорректный ID контейнера"))}
   let socket=crate::docker::endpoint().map_err(error)?;
   let info=crate::docker::get(&socket,"/info").await.map_err(error)?;
   let target=crate::docker::get(&socket,&format!("/containers/{}/json",r.id)).await.map_err(error)?;
   validate_container(&r.action,&target).map_err(error)?;
   let daemon=info["ID"].as_str().filter(|s|!s.is_empty()).ok_or_else(||error("Docker не сообщил ID движка"))?.to_owned();
   (target,Some(socket),daemon)
 }else{return Err(error("Операция не поддерживается. Тома, образы и кэш доступны только для просмотра."))};
 let key=nonce();let p=Plan{created:Instant::now(),action:r.action,id:r.id,target:target.clone(),socket,daemon};
 let mut plans=a.management.plans.lock().unwrap();plans.retain(|_,p|p.created.elapsed()<Duration::from_secs(90));if plans.len()>=32{return Err(error("Слишком много планов. Повторите через минуту."))}
 let shown=if p.action=="process.terminate"{target}else{json!({"Name":target["Name"],"Mounts":target["Mounts"]})};let response=json!({"plan":key,"action":p.action,"id":p.id,"target":shown,"expires_in":90});plans.insert(key,p);Ok(Json(response))
}
fn valid_container_id(id:&str)->bool{id.len()==64&&id.bytes().all(|c|c.is_ascii_hexdigit())}
fn validate_container(action:&str,v:&Value)->Result<(),String>{
 let state=v["State"]["Status"].as_str().ok_or("Не удалось проверить состояние контейнера")?;
 if action=="container.remove"&&!matches!(state,"exited"|"created"){return Err("Удаление разрешено только для остановленного контейнера (exited / created).".into())}
 if action=="container.stop"&&state!="running"{return Err("Контейнер уже остановлен или меняет состояние.".into())}
 if v["Config"]["Labels"].get("com.docker.swarm.service.id").is_some(){return Err("Контейнер управляется Swarm: используйте инструменты оркестратора.".into())}Ok(())
}
fn fingerprint(v:&Value)->Value {json!([v["Id"],v["Name"],v["State"]["Status"],v["State"]["StartedAt"],v["State"]["FinishedAt"],v["Config"],v["HostConfig"],v["Mounts"]])}
pub async fn execute(State(a):State<crate::App>,Json(r):Json<Execute>)->Result<Json<Value>,Error>{
 let _lock=a.management.serial.try_lock().map_err(|_|error("Другая операция ещё выполняется"))?;
 let p=a.management.plans.lock().unwrap().remove(&r.plan).ok_or_else(||error("План уже использован или недействителен. Проверьте объект заново."))?;
 if p.created.elapsed()>Duration::from_secs(90){return Err(error("План устарел. Откройте подтверждение заново."))}
 let result:Result<String,String>=async {
  if p.action=="process.terminate" {crate::collector::terminate_process(&p.id)?;return Ok("Запрос SIGTERM отправлен. Процесс может завершиться не сразу; список обновится по факту.".into())}
  let socket=crate::docker::endpoint()?;
  if Some(&socket)!=p.socket.as_ref(){return Err("Контекст Docker изменился. Операция отменена.".into())}
  let info=crate::docker::get(&socket,"/info").await?;
  if info["ID"].as_str()!=Some(&p.daemon){return Err("Движок Docker изменился. Операция отменена.".into())}
  let current=crate::docker::get(&socket,&format!("/containers/{}/json",p.id)).await?;
  validate_container(&p.action,&current)?;
  if fingerprint(&current)!=fingerprint(&p.target){return Err("Состояние или настройки контейнера изменились. Откройте подтверждение заново.".into())}
  if p.action=="container.remove" {crate::docker::request(&socket,"DELETE",&format!("/containers/{}?force=false&v=false",p.id),30).await?;Ok("Контейнер удалён через Docker. Тома и bind-mount папки сохранены. Место на SSD перепроверяется отдельно.".into())}
  else {crate::docker::request(&socket,"POST",&format!("/containers/{}/stop?t=10",p.id),30).await?;Ok("Контейнер остановлен через Docker. Образы и данные сохранены.".into())}
 }.await;
 *a.management.inventory.lock().await=(Instant::now(),None);
 let detail=result.clone().unwrap_or_else(|e|e);let entry=json!({"time":crate::collector::now(),"action":p.action,"id":p.id,"ok":result.is_ok(),"detail":detail});
 {let mut j=a.management.journal.lock().unwrap();j.insert(0,entry.clone());j.truncate(100);}
 if let Err(e)=a.db.lock().unwrap().event("management_action",&entry){eprintln!("management audit: {e}")}
 result.map(|message|Json(json!({"message":message}))).map_err(error)
}
pub async fn scan(State(a):State<crate::App>)->Json<Value>{Json(a.management.scan.lock().unwrap().clone())}
pub fn start_scan(m:Arc<Management>)->bool {
 use std::sync::atomic::Ordering;
 if m.scanning.swap(true,Ordering::SeqCst){return false}m.cancel.store(false,Ordering::SeqCst);
 {let mut state=m.scan.lock().unwrap();state["status"]=json!("scanning");}
 tokio::task::spawn_blocking(move||{crate::storage_scan::run(m.scan.clone(),m.cancel.clone(),m.cache.clone());m.scanning.store(false,Ordering::SeqCst);});true
}
pub async fn scan_start(State(a):State<crate::App>)->Json<Value>{let started=start_scan(a.management);Json(json!({"status":"scanning","started":started}))}
pub async fn scan_cancel(State(a):State<crate::App>)->Json<Value>{a.management.cancel.store(true,std::sync::atomic::Ordering::SeqCst);Json(json!({"status":"cancelling"}))}
#[cfg(test)]mod tests{use super::*;#[test]fn removal_rejects_active_and_ambiguous_states(){for state in ["running","paused","restarting","removing","dead",""]{assert!(validate_container("container.remove",&json!({"State":{"Status":state}})).is_err())}for state in ["exited","created"]{assert!(validate_container("container.remove",&json!({"State":{"Status":state}})).is_ok())}}#[test]fn identifiers_cannot_inject_paths(){for id in ["x","../volumes","a?force=true",""]{assert!(!valid_container_id(id))}assert!(valid_container_id(&"a".repeat(64)))}#[test]fn changed_mount_invalidates_plan(){let a=json!({"Id":"a","Mounts":[]});let b=json!({"Id":"a","Mounts":[{"Source":"/data"}]});assert_ne!(fingerprint(&a),fingerprint(&b));}}
