//! ТЗ 0.16.0: Session Recorder — журнал действий агента.
//!
//! Когда активна сессия, любая команда `light devtools <cmd>` (кроме
//! `session start`/`stop`) записывается в
//! `.light/agent-sessions/<session-id>/actions.jsonl`. Каждая запись —
//! одна строка JSON:
//!
//! ```json
//! {"ts":"...Z","cmd":"find","args":{...},"result":{...},"screenshot":"screenshots/001.png"}
//! ```
//!
//! Команда `shot` в активной сессии сохраняет PNG в `screenshots/<n>_<cmd>.png`
//! и автоматически дописывает ссылку на него в actions.jsonl.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Имя маркера активной сессии.
const CURRENT_MARKER: &str = "current";
/// Подкаталоги внутри каждой сессии.
const SCREENSHOTS_DIR: &str = "screenshots";
const STATE_DIR: &str = "state";
/// Имя журнала действий.
const ACTIONS_FILE: &str = "actions.jsonl";
/// Имя итогового отчёта.
const SUMMARY_FILE: &str = "summary.md";

/// Ошибка работы с session recorder.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    /// Файловая операция не удалась.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// Нет активной сессии.
    #[error("no active session")]
    NoActiveSession,
    /// JSON-ошибка (например, при записи строки).
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    /// Ошибка декодирования base64 (скриншот).
    #[error("base64: {0}")]
    Decode(String),
}

/// Результат операций session recorder.
pub type Result<T> = std::result::Result<T, SessionError>;

fn sessions_dir(cwd: &Path) -> PathBuf {
    cwd.join(".light").join("agent-sessions")
}

fn current_marker_path(cwd: &Path) -> PathBuf {
    sessions_dir(cwd).join(CURRENT_MARKER)
}

/// ID активной сессии (содержимое файла `current`), если сессия запущена.
pub fn active_session_id(cwd: &Path) -> Option<String> {
    fs::read_to_string(current_marker_path(cwd))
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}

fn rand_suffix() -> String {
    // Достаём энтропию из времени + PID; для имени каталога более чем
    // достаточно. Формат: 6 hex-символов.
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let pid = std::process::id() as u64;
    let x = (now ^ pid.wrapping_mul(0x9E37_79B9_7F4A_7C15)) & 0xFFFFF;
    format!("{x:05x}")
}

/// Создаёт новую сессию, возвращает её id (имя каталога).
///
/// Структура: `<id>/actions.jsonl`, `<id>/screenshots/`, `<id>/state/`, а id
/// записывается в `.light/agent-sessions/current`.
pub fn start_session(cwd: &Path) -> Result<String> {
    let root = sessions_dir(cwd);
    fs::create_dir_all(&root)?;
    // id: `<timestamp>_<rand>`.
    let now = SystemTime::now();
    let secs = now
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Локальное время в формате YYYY-MM-DDTHH-MM-SS (достаточно для имени).
    let ts = format_unix(secs);
    let id = format!("{ts}_{}", rand_suffix());
    let session_dir = root.join(&id);
    fs::create_dir_all(&session_dir)?;
    fs::create_dir_all(session_dir.join(SCREENSHOTS_DIR))?;
    fs::create_dir_all(session_dir.join(STATE_DIR))?;
    let mut actions = File::create(session_dir.join(ACTIONS_FILE))?;
    actions.flush()?;
    fs::write(current_marker_path(cwd), &id)?;
    Ok(id)
}

/// Глубоко-существующий путь к файлу действий текущей сессии.
fn actions_path(cwd: &Path, id: &str) -> PathBuf {
    sessions_dir(cwd).join(id).join(ACTIONS_FILE)
}

/// Закрывает активную сессию: снимает маркер `current`, генерирует
/// `summary.md` (хронологию) и возвращает путь к нему.
pub fn stop_session(cwd: &Path, id: &str) -> Result<PathBuf> {
    let marker = current_marker_path(cwd);
    if let Ok(cur) = fs::read_to_string(&marker)
        && cur.trim() == id
    {
        let _ = fs::remove_file(&marker);
    }
    let session_dir = sessions_dir(cwd).join(id);
    let actions = actions_path(cwd, id);
    let lines = fs::read_to_string(&actions).unwrap_or_default();
    let mut entries: Vec<serde_json::Value> = Vec::new();
    for line in lines.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
            entries.push(v);
        }
    }
    let started = entries
        .first()
        .and_then(|v| v.get("ts").and_then(|t| t.as_str()))
        .unwrap_or("?");
    let ended = entries
        .last()
        .and_then(|v| v.get("ts").and_then(|t| t.as_str()))
        .unwrap_or(started);
    let screenshots = entries
        .iter()
        .filter_map(|e| e.get("screenshot").and_then(|s| s.as_str()))
        .count();
    let mut md = String::new();
    md.push_str("# Agent Session Summary\n\n");
    md.push_str(&format!("- **Started:** {started}\n"));
    md.push_str(&format!("- **Ended:** {ended}\n"));
    md.push_str(&format!("- **Actions:** {}\n", entries.len()));
    md.push_str(&format!("- **Screenshots:** {screenshots}\n\n"));
    md.push_str("## Sequence\n\n");
    for (i, e) in entries.iter().enumerate() {
        let ts = e.get("ts").and_then(|t| t.as_str()).unwrap_or("?");
        let cmd = e.get("cmd").and_then(|c| c.as_str()).unwrap_or("?");
        let shot = e
            .get("screenshot")
            .and_then(|s| s.as_str())
            .map(|s| format!(" (screenshot: `{s}`)").to_owned())
            .unwrap_or_default();
        md.push_str(&format!("{}) `{ts}` — **{cmd}**{shot}\n", i + 1));
    }
    let summary_path = session_dir.join(SUMMARY_FILE);
    fs::write(&summary_path, md)?;
    Ok(summary_path)
}

/// Записывает одно действие в actions.jsonl активной сессии (если она есть).
///
/// `cmd` — имя команды; `args` — аргументы (JSON-объект или null); `result`
/// — результат (JSON-объект или null, либо `true`/строка). Возвращает относительный
/// путь к скриншоту (если был сохранён), иначе `None`.
pub fn record_action(
    cwd: &Path,
    cmd: &str,
    args: &serde_json::Value,
    result: &serde_json::Value,
    screenshot_rel: Option<&str>,
) -> Result<Option<String>> {
    let Some(id) = active_session_id(cwd) else {
        return Ok(None);
    };
    let path = actions_path(cwd, &id);
    let ts = now_rfc3339();
    let mut obj = serde_json::Map::new();
    obj.insert("ts".into(), serde_json::Value::String(ts));
    obj.insert("cmd".into(), serde_json::Value::String(cmd.to_owned()));
    obj.insert("args".into(), args.clone());
    obj.insert("result".into(), result.clone());
    if let Some(s) = screenshot_rel {
        obj.insert("screenshot".into(), serde_json::Value::String(s.to_owned()));
    }
    append_line(&path, &serde_json::to_string(&obj)?)?;
    Ok(screenshot_rel.map(String::from))
}

/// Сохраняет base64-PNG (`"base64:<data>"`) в `screenshots/<counter>_<cmd>.png`
/// активной сессии и возвращает относительный путь (для записи в actions.jsonl).
///
/// Если активной сессии нет — `Ok(None)` (скриншот не сохраняется).
pub fn save_screenshot(cwd: &Path, base64_payload: &str, cmd: &str) -> Result<Option<String>> {
    use base64::Engine;
    let Some(id) = active_session_id(cwd) else {
        return Ok(None);
    };
    let dir = sessions_dir(cwd).join(&id).join(SCREENSHOTS_DIR);
    fs::create_dir_all(&dir)?;
    // Номер счётчика = количество файлов png в каталоге (просто и устойчиво).
    let counter = fs::read_dir(&dir).map(|rd| rd.count()).unwrap_or(0) + 1;
    let fname = format!("{counter:03}_{cmd}.png");
    let target = dir.join(&fname);
    let b64 = base64_payload
        .strip_prefix("base64:")
        .unwrap_or(base64_payload);
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| SessionError::Decode(e.to_string()))?;
    fs::write(&target, &bytes)?;
    Ok(Some(format!("{SCREENSHOTS_DIR}/{fname}")))
}

/// Читает actions.jsonl активной сессии в вектор объектов (для тестов/отчётов).
pub fn read_actions(cwd: &Path, id: &str) -> Result<Vec<serde_json::Value>> {
    let path = actions_path(cwd, id);
    let text = fs::read_to_string(&path).unwrap_or_default();
    Ok(text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .collect())
}

fn append_line(path: &Path, line: &str) -> std::io::Result<()> {
    let mut f = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(f, "{line}")?;
    Ok(())
}

fn now_rfc3339() -> String {
    let d = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format_unix(d.as_secs()).replace('_', "T") + ":00Z"
}

fn format_unix(secs: u64) -> String {
    // Гражданская дата из unix-секунд (просто, без зависимостей).
    let days = (secs / 86400) as i64;
    let sod = (secs % 86400) as u32;
    let h = sod / 3600;
    let m = (sod % 3600) / 60;
    let s = sod % 60;
    let (y, mo, day) = civil_from_days(days);
    format!("{y:04}-{mo:02}-{day:02}_{h:02}-{m:02}-{s:02}")
}

// Алгоритм «civil from days» (Howard Hinnant), без зависимостей.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if mo <= 2 { y + 1 } else { y };
    (y, mo, d)
}
