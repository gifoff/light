//! Интеграционные тесты TCP-сервера DevTools: построчный протокол
//! `SNAPSHOT\n` → JSON / `QUIT\n` → закрытие / неизвестная команда.

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
