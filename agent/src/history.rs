use rusqlite::{Connection,params};
use serde_json::{json,Value};
use std::{collections::BTreeMap,path::Path};
#[derive(Clone,serde::Serialize,serde::Deserialize,Debug)]
pub struct Stat {pub count:u64,pub sum:f64,pub min:f64,pub max:f64}
type Metrics=BTreeMap<String,Stat>;
pub struct History {conn:Connection,last_cleanup:i64}
const TIERS:[(i64,i64);6]=[(1,900),(5,3600),(15,21600),(60,86400),(300,604800),(900,2592000)];
fn merge(to:&mut Metrics,from:Metrics){for(k,s)in from{to.entry(k).and_modify(|v|{v.count+=s.count;v.sum+=s.sum;v.min=v.min.min(s.min);v.max=v.max.max(s.max);}).or_insert(s);}}
fn flatten(v:&Value,prefix:&str,out:&mut Metrics){if let Some(n)=v.as_f64(){if n.is_finite(){out.insert(prefix.into(),Stat{count:1,sum:n,min:n,max:n});}}else if let Some(map)=v.as_object(){for(k,v)in map{if ["machine","interfaces","disks","cores","ecpu_cores","pcpu_cores","fans"].contains(&k.as_str()){continue}flatten(v,&if prefix.is_empty(){k.clone()}else{format!("{prefix}.{k}")},out)}}}
impl History {
 pub fn open(path:&Path)->rusqlite::Result<Self>{let conn=Connection::open(path)?;conn.pragma_update(None,"journal_mode","WAL")?;conn.pragma_update(None,"synchronous","NORMAL")?;conn.busy_timeout(std::time::Duration::from_secs(2))?;
 conn.execute_batch("CREATE TABLE IF NOT EXISTS samples(res INTEGER NOT NULL, ts INTEGER NOT NULL, data TEXT NOT NULL, PRIMARY KEY(res,ts)); CREATE TABLE IF NOT EXISTS processes(id TEXT NOT NULL, ts INTEGER NOT NULL, data TEXT NOT NULL, PRIMARY KEY(id,ts)); CREATE INDEX IF NOT EXISTS processes_time ON processes(ts); CREATE TABLE IF NOT EXISTS events(ts REAL NOT NULL, code TEXT NOT NULL, data TEXT NOT NULL); CREATE INDEX IF NOT EXISTS events_time ON events(ts);")?;Ok(Self{conn,last_cleanup:0})}
 pub fn record(&mut self,s:&Value,p:&[Value],docker:&Value,watch:&std::collections::HashSet<String>)->rusqlite::Result<()> {
 let ts=s["ts"].as_f64().unwrap_or_default() as i64;let mut metrics=Metrics::new();flatten(s,"",&mut metrics);
 for state in ["normal","warning","critical"]{let n=if s["memory"]["pressure"]==state{1.0}else{0.0};metrics.insert(format!("pressure.{state}"),Stat{count:1,sum:n,min:n,max:n});}
 let tx=self.conn.transaction()?;tx.execute("INSERT OR REPLACE INTO samples VALUES (1,?1,?2)",params![ts,serde_json::to_string(&metrics).unwrap()])?;
 // Record selected consumers at a two-second cadence, without command-line secrets.
 if ts%2==0 {let mut keep=std::collections::HashSet::new();for key in ["cpu","memory","write"]{let mut sorted:Vec<_>=p.iter().collect();sorted.sort_by(|a,b|b[key].as_f64().unwrap_or(0.0).total_cmp(&a[key].as_f64().unwrap_or(0.0)));for v in sorted.into_iter().take(8){keep.insert(v["id"].as_str().unwrap_or_default().to_owned());}}
 for v in p.iter().filter(|v|v["development"]==true||keep.contains(v["id"].as_str().unwrap_or_default())||watch.contains(v["id"].as_str().unwrap_or_default())){let value=json!({"label":v["label"],"cpu":v["cpu"],"memory":v["memory"],"read":v["read"],"write":v["write"],"read_total":v["read_total"],"write_total":v["write_total"]});tx.execute("INSERT OR REPLACE INTO processes VALUES(?1,?2,?3)",params![v["id"].as_str(),ts,value.to_string()])?;}
 for v in docker["containers"].as_array().into_iter().flatten(){tx.execute("INSERT OR REPLACE INTO processes VALUES(?1,?2,?3)",params![format!("docker:{}",v["id"].as_str().unwrap_or_default()),ts,v.to_string()])?;}}
 tx.commit()?;if ts-self.last_cleanup>=60{self.compact(ts)?;self.last_cleanup=ts;}Ok(())
 }
 fn compact(&mut self,now:i64)->rusqlite::Result<()> {
 for i in 0..TIERS.len()-1{let(res,age)=TIERS[i];let next=TIERS[i+1].0;let cutoff=(now-age)/next*next;
 let rows:Vec<(i64,String)>={let mut stmt=self.conn.prepare("SELECT ts,data FROM samples WHERE res=?1 AND ts<?2 ORDER BY ts")?;stmt.query_map(params![res,cutoff],|r|Ok((r.get(0)?,r.get(1)?)))?.collect::<Result<_,_>>()?};
 let mut buckets:BTreeMap<i64,Metrics>=BTreeMap::new();for(t,d)in rows{if let Ok(m)=serde_json::from_str(&d){merge(buckets.entry(t/next*next).or_default(),m);}}
 let tx=self.conn.transaction()?;for(t,mut m)in buckets{let old:Option<String>=tx.query_row("SELECT data FROM samples WHERE res=?1 AND ts=?2",params![next,t],|r|r.get(0)).ok();if let Some(old)=old{if let Ok(v)=serde_json::from_str(&old){merge(&mut m,v)}}tx.execute("INSERT OR REPLACE INTO samples VALUES(?1,?2,?3)",params![next,t,serde_json::to_string(&m).unwrap()])?;}
 tx.execute("DELETE FROM samples WHERE res=?1 AND ts<?2",params![res,cutoff])?;tx.commit()?;}
 self.conn.execute("DELETE FROM samples WHERE ts<?1",[now-2592000])?;
 self.conn.execute("DELETE FROM processes WHERE ts<?1",[now-86400])?;self.conn.execute("DELETE FROM events WHERE ts<?1",[now-2592000])?;Ok(())
 }
 pub fn query(&self,since:f64,until:f64)->rusqlite::Result<Value>{let mut stmt=self.conn.prepare("SELECT res,ts,data FROM samples WHERE ts>=?1 AND ts<=?2 ORDER BY ts")?;let rows=stmt.query_map(params![since as i64,until as i64],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?,r.get::<_,String>(2)?)))?;
 let step=((until-since)/1200.0).ceil().max(1.0) as i64;let mut buckets:BTreeMap<i64,(i64,Metrics)>=BTreeMap::new();for row in rows{let(res,t,d)=row?;let e=buckets.entry(t/step*step).or_insert((res,Metrics::new()));e.0=e.0.max(res).max(step);merge(&mut e.1,serde_json::from_str(&d).unwrap_or_default());}
 Ok(json!(buckets.into_iter().map(|(ts,(resolution,m))|{let data:serde_json::Map<String,Value>=m.into_iter().map(|(k,s)|(k,json!({"avg":s.sum/s.count as f64,"min":s.min,"max":s.max,"count":s.count}))).collect();json!({"ts":ts,"resolution":resolution,"metrics":data})}).collect::<Vec<_>>()))}
 pub fn process(&self,id:&str,since:f64)->rusqlite::Result<Value>{let mut stmt=self.conn.prepare("SELECT ts,data FROM processes WHERE id=?1 AND ts>=?2 ORDER BY ts LIMIT 43200")?;let rows=stmt.query_map(params![id,since as i64],|r|{let d:String=r.get(1)?;Ok(json!({"ts":r.get::<_,i64>(0)?,"metrics":serde_json::from_str::<Value>(&d).unwrap_or(Value::Null)}))})?;Ok(json!(rows.collect::<Result<Vec<_>,_>>()?))}
 pub fn event(&self,code:&str,data:&Value)->rusqlite::Result<()>{self.conn.execute("INSERT INTO events VALUES(?1,?2,?3)",params![crate::collector::now(),code,data.to_string()])?;Ok(())}
 pub fn events(&self)->rusqlite::Result<Value>{let mut stmt=self.conn.prepare("SELECT ts,code,data FROM events ORDER BY ts DESC LIMIT 100")?;let rows=stmt.query_map([],|r|{let d:String=r.get(2)?;Ok(json!({"ts":r.get::<_,f64>(0)?,"code":r.get::<_,String>(1)?,"data":serde_json::from_str::<Value>(&d).unwrap_or(Value::Null)}))})?;Ok(json!(rows.collect::<Result<Vec<_>,_>>()?))}
}
#[cfg(test)]mod tests{use super::*;#[test]fn weighted_merge_preserves_spike(){let mut a=Metrics::from([("cpu".into(),Stat{count:4,sum:40.0,min:0.0,max:30.0})]);merge(&mut a,Metrics::from([("cpu".into(),Stat{count:1,sum:100.0,min:100.0,max:100.0})]));assert_eq!(a["cpu"].sum/a["cpu"].count as f64,28.0);assert_eq!(a["cpu"].max,100.0);}
#[test]fn compaction_retains_counts_and_restart_history(){let mut h=History::open(Path::new(":memory:")).unwrap();for t in 1000..1010 {let m=Metrics::from([("cpu".into(),Stat{count:1,sum:(t-1000) as f64,min:(t-1000) as f64,max:(t-1000) as f64})]);h.conn.execute("INSERT INTO samples VALUES(1,?1,?2)",params![t,serde_json::to_string(&m).unwrap()]).unwrap();}h.compact(3000).unwrap();let q=h.query(999.0,1011.0).unwrap();assert_eq!(q.as_array().unwrap().len(),2);assert_eq!(q[0]["metrics"]["cpu"]["count"],5);assert_eq!(q[1]["metrics"]["cpu"]["max"],9.0);h.compact(3000).unwrap();assert_eq!(q,h.query(999.0,1011.0).unwrap());}}
