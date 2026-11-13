//! ТЗ 0.18.0: проксирование HTTP-запросов в TCP DevTools (127.0.0.1:9229).
//!
//! Переиспользует клиент [`light_fw_cli_core::devtools`] — дублировать протокол
//! не нужно. Все ответы оборачиваются в JSON; если TCP-сервер не запущен,
//! возвращается `{"error":"devtools not running"}` (HTTP 200) — фронтенд покажет
//! бейдж «Ожидание приложения…» и продолжит опрашивать.

use light_fw_cli_core::devtools::{DEFAULT_PORT, DevtoolsError, DevtoolsRequest, send_request};

/// Команды, допущенные к проксированию на TCP DevTools (ТЗ v1).
const ALLOWED_COMMANDS: &[&str] = &[
    "tree", "layout", "find", "click", "type", "key", "state", "shot",
];

/// Имя команды, пришедшей из `/api/command`: `cmd` + необязательное `args`.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct CommandBody {
    /// Подкоманда: tree, layout, find, click, type, key, state, shot.
    pub cmd: String,
    /// Аргументы команды (в виде wire-тела строкой или объектом).
    #[serde(default)]
    pub args: Option<serde_json::Value>,
}

impl CommandBody {
    /// Строит тело запроса для [`DevtoolsRequest::to_wire`].
    ///
    /// Правила ТЗ:
    /// - `null`/отсутствует → без тела (TREE, LAYOUT, SNAPSHOT, SHOT);
    /// - строка → передаётся как есть (`click` с `key` уже приходит готовым? нет —
    ///   `key` — объект, см. ниже);
    /// - объект с полем `key` → берём `key` как строку (CLICK/TYPE/KEY);
    /// - прочие объекты → JSON-строка (FIND: `{"tag":...}`).
    pub fn wire_body(&self) -> Option<String> {
        match &self.args {
            None | Some(serde_json::Value::Null) => None,
            Some(serde_json::Value::String(s)) => Some(s.clone()),
            Some(obj @ serde_json::Value::Object(_)) => {
                if let Some(key) = obj.get("key").and_then(|k| k.as_str()) {
                    // CLICK/TYPE/KEY: сервер ждёт `{key}` (и опционально текст/название
                    // уже встроены в key на стороне клиента — упрощение ТЗ v1:
                    // текст клавиши/текст ввода передаётся через отдельное поле,
                    // которое мы здесь не расширяем; типичные кейсы покрыты `key`).
                    Some(key.to_owned())
                } else {
                    Some(obj.to_string())
                }
            }
            Some(v) => Some(v.to_string()),
        }
    }
}

/// Проксирует GET-запрос к подкоманде DevTools (`tree`, `state`, …).
///
/// Возвращает JSON-строку для ответа HTTP. При недоступном TCP-сервере —
/// `{"error":"devtools not running"}`.
pub fn proxy_get(port: u16, cmd: &str) -> String {
    let req = DevtoolsRequest {
        cmd: cmd.to_owned(),
        body: None,
        save_path: None,
        json: false,
    };
    match send_request(port, &req) {
        Ok(raw) => {
            if raw.starts_with("base64:") {
                // Скриншот по GET — редкий случай; отдаём raw-строку как data-URL.
                serde_json::json!({ "ok": true, "result": raw }).to_string()
            } else if is_devtools_error(&raw) {
                serde_json::json!({ "ok": false, "result": raw }).to_string()
            } else {
                // JSON от сервера (TREE / LAYOUT / SNAPSHOT) → пробрасываем в data.
                match serde_json::from_str::<serde_json::Value>(&raw) {
                    Ok(value) => serde_json::json!({ "ok": true, "result": value }).to_string(),
                    // Сервер вернул plain text ("no ui", "not found", …).
                    Err(_) => serde_json::json!({ "ok": false, "result": raw }).to_string(),
                }
            }
        }
        Err(e) => devtools_error_json(&e),
    }
}

/// Проксирует POST `/api/command`.
///
/// Возвращает JSON `{"ok":bool,"result":...}`; при неизвестной команде или
/// недоступном сервере — `ok:false`.
pub fn proxy_command(port: u16, body: &str) -> String {
    let parsed: CommandBody = match serde_json::from_str(body) {
        Ok(b) => b,
        Err(e) => {
            return serde_json::json!({
                "ok": false,
                "error": format!("invalid body: {e}")
            })
            .to_string();
        }
    };
    // Клиентский allowlist (ТЗ): неизвестная команда не уходит в TCP —
    // сразу `{"ok":false,"error":"unknown command: <cmd>"}`.
    let cmd = parsed.cmd.to_ascii_lowercase();
    if !ALLOWED_COMMANDS.contains(&cmd.as_str()) {
        return serde_json::json!({
            "ok": false,
            "error": format!("unknown command: {}", parsed.cmd)
        })
        .to_string();
    }
    let req = DevtoolsRequest {
        body: parsed.wire_body(),
        save_path: None,
        json: false,
        cmd: parsed.cmd,
    };
    match send_request(port, &req) {
        Ok(raw) => {
            if raw.starts_with("base64:") {
                // SHOT: отдаём base64, чтобы фронт мог скачать PNG.
                serde_json::json!({ "ok": true, "result": raw }).to_string()
            } else if is_devtools_error(&raw) {
                serde_json::json!({ "ok": false, "error": raw }).to_string()
            } else {
                // TCP-сервер вернул JSON-строку (например `{"key":".0.1"}`) —
                // парсим её в Value, чтобы `result` всегда был объектом/массивом/null,
                // а не строкой. Если парсинг не удался (plain text "ok", "not found")
                // — оставляем как есть.
                let value = match serde_json::from_str::<serde_json::Value>(&raw) {
                    Ok(v) => v,
                    Err(_) => serde_json::Value::String(raw),
                };
                serde_json::json!({ "ok": true, "result": value }).to_string()
            }
        }
        Err(e) => devtools_error_json(&e),
    }
}

/// Формирует JSON-ошибку для случая, когда приложение не запущено.
/// Контракт: `{"ok": false, "error": "<msg>"}` (поле `result` при ошибке отсутствует).
fn devtools_error_json(e: &DevtoolsError) -> String {
    match e {
        DevtoolsError::NotRunning => {
            serde_json::json!({ "ok": false, "error": "devtools not running" }).to_string()
        }
        other => serde_json::json!({
            "ok": false,
            "error": format!("devtools error: {other}")
        })
        .to_string(),
    }
}

/// Признаки того, что сервер вернул ошибку, а не данные.
fn is_devtools_error(raw: &str) -> bool {
    let t = raw.trim();
    t == "unknown command" || t == "timeout" || t.starts_with("error:")
}

/// Порт DevTools, указанный явно; иначе дефолт (9229).
pub fn resolve_port(custom: Option<u16>) -> u16 {
    custom.filter(|p| *p != 0).unwrap_or(DEFAULT_PORT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_body_wire_variants() {
        // Без аргументов.
        let b: CommandBody = serde_json::from_str(r#"{"cmd":"tree"}"#).expect("parse");
        assert_eq!(b.wire_body(), None);
        // Нулевые аргументы.
        let b: CommandBody = serde_json::from_str(r#"{"cmd":"tree","args":null}"#).expect("parse");
        assert_eq!(b.wire_body(), None);
        // Строка (например, FIND-JSON передан как строка).
        let b: CommandBody =
            serde_json::from_str(r#"{"cmd":"find","args":{"tag":"button","text":"+1"}}"#)
                .expect("parse");
        assert_eq!(
            b.wire_body().as_deref(),
            Some(r#"{"tag":"button","text":"+1"}"#)
        );
        // Клик: `key` как объект.
        let b: CommandBody =
            serde_json::from_str(r#"{"cmd":"click","args":{"key":".0.1"}}"#).expect("parse");
        assert_eq!(b.wire_body().as_deref(), Some(".0.1"));
        // Готовая строка.
        let b: CommandBody =
            serde_json::from_str(r#"{"cmd":"type","args":"0.1 hello"}"#).expect("parse");
        assert_eq!(b.wire_body().as_deref(), Some("0.1 hello"));
    }

    #[test]
    fn not_running_returns_error_json() {
        // Порт, который никто не слушает (уникальный) → NotRunning → JSON-error.
        let port: u16 = 39_527; // маловероятно занят
        let out = proxy_get(port, "tree");
        assert!(out.contains("devtools not running"), "{out}");
    }

    #[test]
    fn invalid_command_body_returns_ok_false() {
        let out = proxy_command(9229, "это не json");
        assert!(out.contains("\"ok\":false"), "{out}");
        assert!(out.contains("invalid body"), "{out}");
    }

    /// ТЗ: неизвестная команда → `{"ok":false,"error":"unknown command: X"}`
    /// без обращения к TCP (hermetic: тест не зависит от того, запущено ли приложение).
    #[test]
    fn unknown_cmd_is_rejected_client_side() {
        let out = proxy_command(9229, r#"{"cmd":"unknown","args":null}"#);
        assert!(out.contains("\"ok\":false"), "{out}");
        assert!(out.contains("unknown command: unknown"), "{out}");
    }

    #[test]
    fn allowed_commands_pass_allowlist() {
        for cmd in ALLOWED_COMMANDS {
            // Не должен быть «unknown command» даже без запущенного сервера —
            // в этом случае будет «devtools not running».
            let out = proxy_command(39_527, &format!(r#"{{"cmd":"{cmd}"}}"#));
            assert!(
                !out.contains("unknown command"),
                "команда `{cmd}` была отклонена allowlist'ом: {out}"
            );
        }
    }

    /// ТЗ: `proxy_get` возвращает JSON с полями `ok` и `result` (не `data`).
    #[test]
    fn proxy_get_returns_ok_result_contract() {
        let port: u16 = 39_527; // никто не слушает → NotRunning
        let raw = proxy_get(port, "tree");
        let v: serde_json::Value = serde_json::from_str(&raw).expect("valid json: {raw}");
        assert!(v.get("ok").is_some(), "поле `ok` отсутствует: {raw}");
        assert!(
            !v.get("data").is_some(),
            "поле `data` не должно присутствовать в HTTP-контракте: {raw}"
        );
        assert!(
            v.get("error").is_some() || v.get("result").is_some(),
            "ожидался `error` или `result`: {raw}"
        );
    }

    /// ТЗ: `proxy_command` для invalid body возвращает `{ok:false, error:"..."}`.
    #[test]
    fn proxy_command_invalid_body_has_error_field() {
        let out = proxy_command(9229, "не json вообще");
        let v: serde_json::Value = serde_json::from_str(&out).expect("valid json: {out}");
        assert_eq!(v["ok"], false);
        assert!(
            v.get("error").is_some(),
            "при ошибке должно быть поле `error`: {out}"
        );
    }

    /// ТЗ: unknown command → `{ok:false, error:"unknown command: X"}`.
    #[test]
    fn proxy_command_unknown_cmd_has_error_field() {
        let out = proxy_command(9229, r#"{"cmd":"nonsense","args":null}"#);
        let v: serde_json::Value = serde_json::from_str(&out).expect("valid json: {out}");
        assert_eq!(v["ok"], false);
        assert!(
            v["error"]
                .as_str()
                .unwrap_or("")
                .contains("unknown command"),
            "ошибка должна содержать 'unknown command': {out}"
        );
    }
}
