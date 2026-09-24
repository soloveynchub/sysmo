//! Workspace inventory runs independently from the 1 Hz telemetry collector.
use axum::{extract::{Query, State}, http::StatusCode, Json};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{path::PathBuf, sync::{Arc, atomic::{AtomicBool, Ordering}}, time::Duration};
use std::os::fd::AsRawFd;
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, process::Command};
use crate::App;

pub struct Workspaces {
    root: PathBuf,
    data: PathBuf,
    running: AtomicBool,
    ai_running: AtomicBool,
    ai: tokio::sync::OnceCell<Value>,
}

impl Workspaces {
    pub fn new(root: PathBuf, data: PathBuf) -> Self {
        Self { root, data, running: AtomicBool::new(false), ai_running: AtomicBool::new(false), ai: tokio::sync::OnceCell::new() }
    }
    fn busy(&self) -> bool {
        if self.running.load(Ordering::SeqCst) { return true; }
        // A worker may still be finishing after the agent was restarted.
        if let Ok(lock) = std::fs::File::open(self.data.join("workspace.lock")) {
            let acquired = unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0;
            if acquired { unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_UN); } }
            return !acquired;
        }
        false
    }
    fn ai_busy(&self) -> bool {
        if self.ai_running.load(Ordering::SeqCst) { return true; }
        if let Ok(lock) = std::fs::File::open(self.data.join("workspace-ai.lock")) {
            let acquired = unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0;
            if acquired { unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_UN); } }
            return !acquired;
        }
        false
    }
    fn command(&self, mode: &str) -> Command {
        let mut command = Command::new("/usr/bin/python3");
        command.arg(self.root.join("scripts/workspace_monitor.py")).arg(mode).arg("--data").arg(&self.data)
            .kill_on_drop(true).stderr(std::process::Stdio::null());
        command
    }
    async fn invoke(&self, mode: &str, query: &Selection, input: Option<Value>) -> Result<Value, &'static str> {
        let mut command = self.command(mode);
        if let Some(id) = query.before { command.arg("--before").arg(id.to_string()); }
        if let Some(id) = query.after { command.arg("--after").arg(id.to_string()); }
        command.stdout(std::process::Stdio::piped()).stdin(std::process::Stdio::piped());
        let mut child = command.spawn().map_err(|_| "Не удалось запустить Python 3.")?;
        if let Some(mut stdin) = child.stdin.take() {
            if let Some(value) = input { stdin.write_all(value.to_string().as_bytes()).await.map_err(|_| "Не удалось передать настройки.")?; }
        }
        let mut output = child.stdout.take().ok_or("Нет ответа сборщика.")?.take(32 * 1024 * 1024);
        let mut bytes = Vec::new();
        tokio::time::timeout(Duration::from_secs(if mode == "ai-analyze" { 120 } else { 30 }), output.read_to_end(&mut bytes)).await
            .map_err(|_| "История не ответила вовремя.")?.map_err(|_| "Не удалось прочитать историю.")?;
        let status = tokio::time::timeout(Duration::from_secs(5), child.wait()).await
            .map_err(|_| "Сборщик не завершил чтение.")?.map_err(|_| "Ошибка сборщика.")?;
        if !status.success() { return Err(if mode == "settings" {"Укажите 1–16 существующих папок внутри домашней папки."} else {"Не удалось прочитать историю проектов."}); }
        serde_json::from_slice(&bytes).map_err(|_| "Некорректный ответ сборщика.")
    }
    async fn ai_status(&self) -> Value {
        self.ai.get_or_init(|| async {
            let path = self.root.join("agent/target/release/workspace-ai");
            match tokio::time::timeout(Duration::from_secs(8), Command::new(path).arg("status").kill_on_drop(true).output()).await {
                Ok(Ok(output)) if output.status.success() => serde_json::from_slice(&output.stdout).unwrap_or(json!({"status":"unavailable"})),
                _ => json!({"status":"helper_unavailable"}),
            }
        }).await.clone()
    }
}

pub fn start(m: Arc<Workspaces>, verify: bool) -> bool {
    if m.busy() || m.running.swap(true, Ordering::SeqCst) { return false; }
    tokio::spawn(async move {
        let mut command = m.command("scan");
        if verify { command.arg("--verify-remote"); }
        // No blanket scan deadline: full inventories may take a long time.
        let outcome = command.stdout(std::process::Stdio::null()).status().await;
        if !matches!(outcome, Ok(ref s) if s.success() || s.code() == Some(75)) {
            let _ = tokio::fs::write(m.data.join("workspace-status.json"), json!({"status":"error","message":"Сборщик завершился с ошибкой. Последний сохранённый снимок остаётся доступен."}).to_string()).await;
        }
        m.running.store(false, Ordering::SeqCst);
    });
    true
}

pub async fn schedule(m: Arc<Workspaces>) {
    // Let startup telemetry settle. Poll due time; restarting doesn't force another full scan.
    tokio::time::sleep(Duration::from_secs(15)).await;
    loop {
        let status = tokio::fs::read(m.data.join("workspace-status.json")).await.ok()
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok()).unwrap_or(Value::Null);
        let last = status["finished"].as_f64().unwrap_or(0.0);
        if crate::collector::now() - last >= 6.0 * 3600.0 { start(m.clone(), false); }
        tokio::time::sleep(Duration::from_secs(300)).await;
    }
}

#[derive(Deserialize, Default)]
pub struct Selection { before: Option<i64>, after: Option<i64> }
type ApiResult = Result<Json<Value>, (StatusCode, Json<Value>)>;
fn error(message: &str) -> (StatusCode, Json<Value>) { (StatusCode::BAD_REQUEST, Json(json!({"error":message}))) }

pub async fn view(State(a): State<App>, Query(q): Query<Selection>) -> ApiResult {
    let mut value = a.workspaces.invoke("view", &q, None).await.map_err(error)?;
    let busy = a.workspaces.busy();
    value["running"] = json!(busy);
    if !busy && value["status"]["status"] == "scanning" {
        value["status"] = json!({"status":"error","message":"Предыдущий обход был прерван. Можно запустить новый снимок."});
    }
    value["ai"] = a.workspaces.ai_status().await;
    let ai_busy = a.workspaces.ai_busy();
    value["gateway"]["running"] = json!(ai_busy);
    if !ai_busy && value["gateway"]["result"]["status"] == "running" {
        value["gateway"]["result"]["status"] = json!("error");
        value["gateway"]["result"]["error"] = json!("Предыдущий запрос был прерван. Автоматический повтор не выполнялся.");
    }
    Ok(Json(value))
}
pub async fn export(State(a): State<App>, Query(q): Query<Selection>) -> ApiResult {
    a.workspaces.invoke("export", &q, None).await.map(Json).map_err(error)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScanRequest { #[serde(default)] verify_remote: bool }
pub async fn scan(State(a): State<App>, Json(body): Json<ScanRequest>) -> ApiResult {
    if start(a.workspaces.clone(), body.verify_remote) { Ok(Json(json!({"started":true}))) }
    else { Err((StatusCode::CONFLICT, Json(json!({"error":"Сканирование уже выполняется."})))) }
}
pub async fn cancel(State(a): State<App>) -> ApiResult {
    tokio::fs::write(a.workspaces.data.join("workspace-cancel"), b"cancel").await.map_err(|_| error("Не удалось остановить обход."))?;
    Ok(Json(json!({"requested":true})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings { roots: Vec<String> }
pub async fn settings(State(a): State<App>, Json(body): Json<Settings>) -> ApiResult {
    // Serialize settings with scan startup, so a snapshot always uses one scope.
    if a.workspaces.busy() || a.workspaces.running.swap(true, Ordering::SeqCst) {
        return Err((StatusCode::CONFLICT, Json(json!({"error":"Сначала дождитесь окончания или остановите сканирование."}))));
    }
    let result = a.workspaces.invoke("settings", &Selection::default(), Some(json!({"roots":body.roots}))).await;
    a.workspaces.running.store(false, Ordering::SeqCst);
    result.map(Json).map_err(error)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AISettings { api_key: Option<String>, model: Option<String> }
fn ai_result(value: Value) -> ApiResult {
    if value.get("error").is_some() { Err((StatusCode::BAD_REQUEST, Json(value))) } else { Ok(Json(value)) }
}
pub async fn ai_settings(State(a): State<App>, Json(body): Json<AISettings>) -> ApiResult {
    if a.workspaces.ai_busy() { return Err(error("Дождитесь завершения AI-разбора.")); }
    let mut input = json!({});
    if let Some(key) = body.api_key { input["api_key"] = json!(key); }
    if let Some(model) = body.model { input["model"] = json!(model); }
    ai_result(a.workspaces.invoke("ai-save", &Selection::default(), Some(input)).await.map_err(error)?)
}
pub async fn ai_models(State(a): State<App>) -> ApiResult {
    ai_result(a.workspaces.invoke("ai-models", &Selection::default(), None).await.map_err(error)?)
}
pub async fn ai_clear(State(a): State<App>) -> ApiResult {
    if a.workspaces.ai_busy() { return Err(error("Дождитесь завершения AI-разбора.")); }
    ai_result(a.workspaces.invoke("ai-clear", &Selection::default(), None).await.map_err(error)?)
}
pub async fn ai_analyze(State(a): State<App>, Query(q): Query<Selection>) -> ApiResult {
    if a.workspaces.ai_busy() || a.workspaces.ai_running.swap(true, Ordering::SeqCst) {
        return Err((StatusCode::CONFLICT, Json(json!({"error":"AI-разбор уже выполняется."}))));
    }
    // Resolve the exact pair now. Never silently analyze a newer snapshot during startup.
    let selected = a.workspaces.invoke("view", &q, None).await;
    let selected = match selected {
        Ok(v) if v["current"]["id"].as_i64().is_some() && v["gateway"]["configured"] == true && v["gateway"]["model"].as_str().is_some_and(|s|!s.is_empty()) => v,
        _ => { a.workspaces.ai_running.store(false, Ordering::SeqCst); return Err(error("Нужен готовый снимок, ключ и модель Timeweb.")); }
    };
    let exact = Selection { before: selected["previous"]["id"].as_i64(), after: selected["current"]["id"].as_i64() };
    let m = a.workspaces.clone();
    tokio::spawn(async move {
        let _ = m.invoke("ai-analyze", &exact, None).await;
        m.ai_running.store(false, Ordering::SeqCst);
    });
    Ok(Json(json!({"started":true})))
}
