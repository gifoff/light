//! Интеграционные тесты TCP-сервера DevTools: построчный протокол
//! `SNAPSHOT\n` → JSON / `PING\n` → `PONG` / `QUIT\n` → закрытие /
//! `CLICK <key>\n` → `ok:true`/`ok:false`/`timeout` /
//! неизвестная команда → `unknown command`.

#![cfg(feature = "devtools")]

extern crate light_fw_core as light;

use std::io::{Read, Write};
use std::net::TcpStream;

/// Читает одну строку из TCP-потока.
fn read_line(stream: &mut TcpStream) -> String {
    let mut buf = [0u8; 4096];
    let mut line = String::new();
    loop {
        // Читаем байт за разом до \n или EOF — для тестового размера это ок.
        match stream.read(&mut buf[..1]) {
            Ok(0) => break,
            Ok(_) => {
                if buf[0] == b'\n' {
                    line.push('\n');
                    break;
                }
                line.push(buf[0] as char);
            }
            Err(_) => break,
        }
    }
    line
}

/// `SNAPSHOT` возвращает JSON с ключом `window_token` или `null`.
#[test]
fn server_returns_snapshot_on_request() {
    let port = light::devtools::server::start(0).expect("server starts");
    // Обновляем глобальный снимок, чтобы ответ был не `null`.
    light::update_latest_snapshot("{\"window_token\": 7}".to_owned());
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    stream.write_all(b"SNAPSHOT\n").expect("send");
    let line = read_line(&mut stream);
    assert!(
        line.contains("\"window_token\"") || line == "null\n",
        "got: {line:?}"
    );
}

/// Неизвестная команда → `unknown command`, соединение не рвётся.
#[test]
fn unknown_command_returns_error_and_keeps_connection() {
    let port = light::devtools::server::start(0).expect("server starts");
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    stream.write_all(b"FROBULATE\n").expect("send");
    let line = read_line(&mut stream);
    assert_eq!(line.trim(), "unknown command");
    // Соединение живо: запрашиваем SNAPSHOT и получаем ответ.
    stream.write_all(b"SNAPSHOT\n").expect("send again");
    let line2 = read_line(&mut stream);
    assert!(!line2.is_empty(), "response after unknown command");
}

/// `QUIT` закрывает соединение: следующий `read` получает 0 (EOF).
#[test]
fn quit_closes_connection() {
    let port = light::devtools::server::start(0).expect("server starts");
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    stream.write_all(b"QUIT\n").expect("send");
    let mut buf = [0u8; 1];
    let n = stream.read(&mut buf).expect("read");
    assert_eq!(n, 0, "EOF after QUIT");
}

/// `PING\n` → `PONG\n`: сервер отвечает на проверку живости.
#[test]
fn server_responds_to_ping() {
    let port = light::devtools::server::start(0).expect("server starts");
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    stream.write_all(b"PING\n").expect("send");
    let line = read_line(&mut stream);
    assert_eq!(line.trim(), "PONG", "expected PONG, got: {line:?}");
}

/// Неизвестная команда `BLAH\n` → `unknown command\n`, соединение живёт дальше.
#[test]
fn server_handles_unknown_command() {
    let port = light::devtools::server::start(0).expect("server starts");
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    stream.write_all(b"BLAH\n").expect("send");
    let line = read_line(&mut stream);
    assert_eq!(line.trim(), "unknown command");
    // Соединение живо: PING ещё работает.
    stream.write_all(b"PING\n").expect("ping after unknown");
    let line2 = read_line(&mut stream);
    assert_eq!(line2.trim(), "PONG");
}

/// `CLICK <key>\n` без запущенного цикла событий: прокси не установлен
/// (цикл не создан), команда остаётся в очереди, её никто не дренирует —
/// сервер ждёт ответа 1 с и отвечает `timeout`.
///
/// Это проверяет именно «без прокси» ветку [`send_command`]. Реальный клик
/// (с запущенным циклом) покрывается макросным тестом
/// `explicit_key_addresses_dispatch` в `tests/template.rs`: он доказывает, что
/// `dispatch("inc_btn", Click)` вызывает обработчик, а `UserEvent::DevTools`
/// доставляет именно эту команду в UI-поток.
#[test]
fn click_without_ui_times_out() {
    let port = light::devtools::server::start(0).expect("server starts");
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    stream.write_all(b"CLICK inc_btn\n").expect("send");
    let line = read_line(&mut stream);
    assert_eq!(
        line.trim(),
        "timeout",
        "без UI-потока клик должен завершиться таймаутом, got: {line:?}"
    );
}

/// `SET_STATE` без имени поля (пустое поле) → `missing field`; команда не
/// уходит в UI-поток.
#[test]
fn set_state_empty_field_returns_missing_field() {
    let port = light::devtools::server::start(0).expect("server starts");
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    // Два пробела: имя поля пустое, есть JSON-значение.
    stream.write_all(b"SET_STATE  5\n").expect("send");
    let line = read_line(&mut stream);
    assert_eq!(line.trim(), "missing field");
}

/// `SET_STATE <field>` без значения (одно слово) → `invalid json`: пустое
/// значение не разбирается как JSON.
#[test]
fn set_state_without_value_returns_invalid_json() {
    let port = light::devtools::server::start(0).expect("server starts");
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    stream.write_all(b"SET_STATE count\n").expect("send");
    let line = read_line(&mut stream);
    assert_eq!(line.trim(), "invalid json");
}

/// `SET_STATE <field> <не-json>` → `invalid json` (значение не разобрано).
#[test]
fn set_state_invalid_json_returns_error() {
    let port = light::devtools::server::start(0).expect("server starts");
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    stream
        .write_all(b"SET_STATE count not-a-json\n")
        .expect("send");
    let line = read_line(&mut stream);
    assert_eq!(line.trim(), "invalid json");
}

/// `SET_STATE count 5` без запущенного цикла событий: команда уходит в очередь,
/// её никто не дренирует — сервер ждёт ответа 1 с и отвечает `timeout` (как и
/// для CLICK без UI).
#[test]
fn set_state_without_ui_times_out() {
    let port = light::devtools::server::start(0).expect("server starts");
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    stream.write_all(b"SET_STATE count 5\n").expect("send");
    let line = read_line(&mut stream);
    assert_eq!(
        line.trim(),
        "timeout",
        "без UI-потока SET_STATE должен завершиться таймаутом, got: {line:?}"
    );
}

/// `SAVE_STATE` — асинхронная команда: сервер отвечает `ok` немедленно, не
/// ожидая записи состояния (запись выполняется в UI-потоке; без UI она просто
/// не произойдёт, но протокол-ответ тот же).
#[test]
fn save_state_returns_ok_immediately() {
    let port = light::devtools::server::start(0).expect("server starts");
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    stream.write_all(b"SAVE_STATE\n").expect("send");
    let line = read_line(&mut stream);
    assert_eq!(
        line.trim(),
        "ok",
        "SAVE_STATE обязан ответить ok, got: {line:?}"
    );
}
