//! ТЗ 0.16.0: юнит-тесты session recorder — создание сессии, запись действий,
//! сохранение скриншота, генерация summary.md при stop.

use std::fs;
use std::path::PathBuf;

use serde_json::json;

use light_fw_cli_core::session;

/// Уникальный изолированный каталог «рабочей директории» приложения.
fn scratch() -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("light-session-test-{nanos}"));
    fs::create_dir_all(&dir).expect("create");
    dir
}

/// Минимальный валидный PNG 1x1 (base64), чтобы `save_screenshot` его принял.
const TINY_PNG_B64: &str =
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4z8AAAAMBAQDJ/pLvAAAAAElFTkSuQmCC";

#[test]
fn session_lifecycle_with_actions_and_screenshot() {
    let cwd = scratch();
    let _ = fs::remove_dir_all(cwd.join(".light")); // чистый старт

    // --- start ---
    let id = session::start_session(&cwd).expect("start");
    assert!(!id.is_empty());
    let sessions = cwd.join(".light").join("agent-sessions");
    let sess = sessions.join(&id);
    assert!(sess.is_dir(), "каталог сессии создан: {sess:?}");
    assert!(sess.join("actions.jsonl").is_file(), "actions.jsonl создан");
    assert!(sess.join("screenshots").is_dir(), "screenshots/ создан");
    assert!(sess.join("state").is_dir(), "state/ создан");
    // маркер current
    let marker = sessions.join("current");
    assert_eq!(fs::read_to_string(&marker).expect("marker"), id);

    // --- record_action ×2 ---
    session::record_action(
        &cwd,
        "find",
        &json!({"tag": "button"}),
        &json!({"key": "0.0"}),
        None,
    )
    .expect("record find");
    session::record_action(&cwd, "click", &json!({"key": "0.0"}), &json!("ok"), None)
        .expect("record click");

    // --- shot: сохраняет PNG + логирует действие (как делает CLI) ---
    let rel = session::save_screenshot(&cwd, &format!("base64:{TINY_PNG_B64}"), "shot")
        .expect("save screenshot");
    assert!(rel.is_some(), "путь скриншота возвращён");
    let png_path = sess.join(rel.as_deref().unwrap());
    assert!(png_path.is_file(), "PNG сохранён: {png_path:?}");
    session::record_action(&cwd, "shot", &json!(null), &json!("<png>"), rel.as_deref())
        .expect("record shot");

    // --- stop ---
    let summary = session::stop_session(&cwd, &id).expect("stop");
    assert!(summary.is_file(), "summary.md существует: {summary:?}");
    assert!(!marker.exists(), "маркер current удалён");
    let text = fs::read_to_string(&summary).expect("summary text");
    assert!(text.contains("# Agent Session Summary"));
    // три записи: find + click + shot
    assert!(text.contains("**Actions:** 3"), "счётчик действий: {text}");
    // одна ссылка на скриншот
    assert!(
        text.contains("**Screenshots:** 1"),
        "счётчик скриншотов: {text}"
    );
    // хроника с нумерацией и упоминанием каждого cmd
    assert!(text.contains("## Sequence"), "раздел Sequence: {text}");
    assert!(text.contains("**find**"), "команда find в хронике: {text}");
    assert!(
        text.contains("**click**"),
        "команда click в хронике: {text}"
    );
    assert!(
        text.contains("screenshot:"),
        "ссылка на скриншот в хронике: {text}"
    );
    // нумерованная хроника
    assert!(text.contains("## Sequence"), "раздел Sequence: {text}");

    // --- cleanup ---
    let _ = fs::remove_dir_all(&cwd);
}

#[test]
fn stop_without_active_session_fails() {
    let cwd = scratch();
    let err = session::stop_session(&cwd, "no-such-id").expect_err("нет активной сессии");
    let msg = format!("{err}");
    assert!(!msg.is_empty(), "возвращается осмысленная ошибка");
    let _ = fs::remove_dir_all(&cwd);
}
