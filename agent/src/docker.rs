use serde_json::{json,Value};
use std::{collections::HashMap,env,fs,path::PathBuf,time::Instant};
use tokio::{net::UnixStream,io::{AsyncReadExt,AsyncWriteExt},time::{timeout,Duration}};

pub fn endpoint()->Result<PathBuf,String>{
 let home=PathBuf::from(env::var("HOME").unwrap_or_default());
 let root=env::var("DOCKER_CONFIG").map(PathBuf::from).unwrap_or_else(|_|home.join(".docker"));
 let config:Value=fs::read(root.join("config.json")).ok().and_then(|b|serde_json::from_slice(&b).ok()).unwrap_or(Value::Null);
 let context=env::var("DOCKER_CONTEXT").ok();
 if context.is_none(){if let Ok(host)=env::var("DOCKER_HOST"){return unix(&host)}}
 let name=context.or_else(||config["currentContext"].as_str().map(str::to_owned)).unwrap_or_else(||"default".into());
 if name!="default"{if let Ok(entries)=fs::read_dir(root.join("contexts/meta")){for e in entries.flatten(){let v:Value=fs::read(e.path().join("meta.json")).ok().and_then(|b|serde_json::from_slice(&b).ok()).unwrap_or(Value::Null);if v["Name"]==name{return v["Endpoints"]["docker"]["Host"].as_str().ok_or("invalid_context".into()).and_then(unix)}}}return Err("context_unavailable".into())}
 [home.join(".docker/run/docker.sock"),PathBuf::from("/var/run/docker.sock")].into_iter().find(|p|p.exists()).ok_or_else(||if PathBuf::from("/Applications/Docker.app").exists(){"daemon_unavailable".into()}else{"not_installed".into()})
}
fn unix(s:&str)->Result<PathBuf,String>{s.strip_prefix("unix://").filter(|p|p.starts_with('/')).map(PathBuf::from).ok_or("unsupported_remote".into())}
async fn get(socket:&PathBuf,path:&str)->Result<Value,String>{
 timeout(Duration::from_secs(3),async{
 let mut s=UnixStream::connect(socket).await.map_err(|e|e.to_string())?;
 s.write_all(format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").as_bytes()).await.map_err(|e|e.to_string())?;
 let mut buf=vec![];s.take(8*1024*1024).read_to_end(&mut buf).await.map_err(|e|e.to_string())?;
 let i=buf.windows(4).position(|w|w==b"\r\n\r\n").ok_or("bad_http")?;let head=String::from_utf8_lossy(&buf[..i]);if !head.starts_with("HTTP/1.1 200"){return Err(head.lines().next().unwrap_or("http_error").into())}
 let body=&buf[i+4..];let bytes=if head.to_lowercase().contains("transfer-encoding: chunked"){let mut out=vec![];let mut rest=body;loop{let i=rest.windows(2).position(|w|w==b"\r\n").ok_or("bad_chunk")?;let size=usize::from_str_radix(std::str::from_utf8(&rest[..i]).map_err(|_|"bad_chunk")?.split(';').next().unwrap(),16).map_err(|_|"bad_chunk")?;if size==0{break}if rest.len()<i+2+size+2{return Err("short_chunk".into())}out.extend_from_slice(&rest[i+2..i+2+size]);rest=&rest[i+2+size+2..];}out}else{body.to_vec()};
 serde_json::from_slice(&bytes).map_err(|e|e.to_string())
 }).await.map_err(|_|"timeout".to_string())?
}
pub struct Docker {previous:HashMap<String,(Instant,f64,f64,f64,f64,f64,f64)>}
impl Docker {
 pub fn new()->Self{Self{previous:HashMap::new()}}
 pub async fn sample(&mut self)->Value{
 let socket=match endpoint(){Ok(p)=>p,Err(status)=>return json!({"status":status,"containers":[]})};
 let list=match get(&socket,"/containers/json?all=1").await{Ok(v)=>v,Err(e)=>return json!({"status":"daemon_unavailable","reason":e,"containers":[]})};
 let mut rows=vec![];let mut live=std::collections::HashSet::new();
 for c in list.as_array().into_iter().flatten(){let id=c["Id"].as_str().unwrap_or_default();if id.is_empty(){continue}live.insert(id.to_owned());let running=c["State"]=="running";
 let stats=if running{get(&socket,&format!("/containers/{id}/stats?stream=false&one-shot=true")).await.ok()}else{None};
 let mut row=json!({"id":id,"name":c["Names"][0].as_str().unwrap_or(id).trim_start_matches('/'),"image":c["Image"],"state":c["State"],"status_text":c["Status"],"created":c["Created"],"cpu":null,"memory":null,"limit":null,"download":null,"upload":null,"read":null,"write":null,"pids":null,"stats_available":stats.is_some()});
 if let Some(s)=stats{
 let n=|v:&Value|v.as_f64().unwrap_or(0.0);
 let cpu=n(&s["cpu_stats"]["cpu_usage"]["total_usage"]);let sys=n(&s["cpu_stats"]["system_cpu_usage"]);let count=n(&s["cpu_stats"]["online_cpus"]);
 let net=s["networks"].as_object();let rx=net.map(|n|n.values().map(|v|nval(&v["rx_bytes"])).sum()).unwrap_or(0.0);let tx=net.map(|n|n.values().map(|v|nval(&v["tx_bytes"])).sum()).unwrap_or(0.0);
 let block=|op:&str|s["blkio_stats"]["io_service_bytes_recursive"].as_array().map(|a|a.iter().filter(|v|v["op"].as_str().unwrap_or("").eq_ignore_ascii_case(op)).map(|v|n(&v["value"])).sum()).unwrap_or(0.0);let read=block("read");let write=block("write");
 if let Some((time,pc,ps,prx,ptx,pr,pw))=self.previous.get(id){let dt=time.elapsed().as_secs_f64();let rate=|a:f64,b:f64|if a>=b&&dt>0.0{Some((a-b)/dt)}else{None};row["cpu"]=json!(if cpu>=*pc&&sys>*ps&&count>0.0{Some((cpu-pc)/(sys-ps)*count*100.0)}else{None});row["download"]=json!(rate(rx,*prx));row["upload"]=json!(rate(tx,*ptx));row["read"]=json!(rate(read,*pr));row["write"]=json!(rate(write,*pw));}
 self.previous.insert(id.into(),(Instant::now(),cpu,sys,rx,tx,read,write));
 row["memory"]=s["memory_stats"]["usage"].clone();row["limit"]=s["memory_stats"]["limit"].clone();row["pids"]=s["pids_stats"]["current"].clone();row["read_total"]=json!(read);row["write_total"]=json!(write);row["received"]=json!(rx);row["transmitted"]=json!(tx);
 }rows.push(row);
 }
 self.previous.retain(|id,_|live.contains(id));json!({"status":"available","socket":socket,"ts":crate::collector::now(),"containers":rows})
 }
}
fn nval(v:&Value)->f64{v.as_f64().unwrap_or(0.0)}
#[cfg(test)]mod tests{use super::*;#[test]fn excludes_remote_endpoints(){assert!(unix("tcp://remote:2375").is_err());assert!(unix("ssh://remote").is_err());assert_eq!(unix("unix:///tmp/docker.sock").unwrap(),PathBuf::from("/tmp/docker.sock"));}}
