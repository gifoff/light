//! TCP-сервер для DevTools: построчный протокол, передающий JSON-снимок
//! состояния UI внешним инструментам (IDE / standalone inspector).
//!
//! Протокол (newline-delimited):
//! * Клиент шлёт `SNAPSHOT\n` — сервер отвечает одной строкой JSON + `\n`
//!   (или `null\n`, если снимка ещё нет).
//! * Клиент шлёт `PING\n` — сервер отвечает `PONG\n`.
//! * Клиент шлёт `CLICK <key>\n` — сервер отправляет команду клика в
//!   UI-поток (имитация нажатия элемента с ключом `key`).
//! * Клиент шлёт `SET_STATE <field> <json>\n` — сервер пытается изменить
//!   реактивное поле (пока заглушка: отвечает `not supported yet\n`).
//! * Клиент шлёт `QUIT\n` — сервер закрывает соединение.
//! * Неизвестная команда — сервер отвечает `unknown command\n` и продолжает
//!   обслуживание.
//!
//! Сервер слушает только на `127.0.0.1` (безопасность). Работа ведётся в
//! отдельном `[std::thread]`, не блокируя UI-поток. Клиенты обрабатываются
//! последовательно (head-of-line blocking) — приемлемо для dev-инструмента.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

/// Запускает TCP-сервер на `127.0.0.1:port`.
///
/// Возвращает фактический порт (полезно при `port = 0`, когда ОС выбирает
/// свободный порт сама). Сервер обрабатывает клиентов в отдельном потоке;
/// сам вызов возвращает немедленно.
///
/// # Ошибки
///
/// Возвращает ошибку, если `TcpListener::bind` не удался (например, порт
/// занят).
pub fn start(port: u16) -> std::io::Result<u16> {
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    let addr = listener.local_addr()?;
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            handle_client(stream);
        }
    });
    Ok(addr.port())
}

/// Читает одну строку (до `\n` или EOF) из TCP-потока.
fn read_line(stream: &mut TcpStream) -> std::io::Result<String> {
    let mut buf = Vec::new();
    // Читаем байт за байтом до `\n` или EOF. Для коротких команд (десятки
    // байт) накладные расходы пренебрежимо малы.
    let mut byte = [0u8; 1];
    loop {
        match stream.read(&mut byte)? {
            0 => return Ok(String::from_utf8_lossy(&buf).into_owned()),
            _ => {
                if byte[0] == b'\n' {
                    return Ok(String::from_utf8_lossy(&buf).into_owned());
                }
                buf.push(byte[0]);
            }
        }
    }
}

/// Обрабатывает одного клиента до закрытия соединения или команды `QUIT`.
fn handle_client(mut stream: TcpStream) {
    while let Some(command) = read_command(&mut stream) {
        if command.is_empty() {
            continue;
        }
        // QUIT не шлёт ответа — просто закрывает соединение (клиент видит EOF).
        if command == "QUIT" {
            break;
        }
        let response = dispatch_command(&command);
        if write_response(&mut stream, &response).is_none() {
            break;
        }
    }
}

/// Выполняет одну команду протокола и возвращает строку-ответ.
fn dispatch_command(command: &str) -> String {
    // Разбираем команду: первое слово — имя, остальное — аргументы.
    let mut parts = command.splitn(3, ' ');
    let name = parts.next().unwrap_or("");
    let arg1 = parts.next().unwrap_or("");
    let _ = parts.next(); // arg2 (для SET_STATE: JSON-значение) пока не используется

    match name {
        "SNAPSHOT" => super::read_latest_snapshot().unwrap_or_else(|| "null".to_owned()),
        "PING" => "PONG".to_owned(),
        "CLICK" => {
            if !arg1.is_empty() {
                super::send_command(super::DevToolsCommand::Click(arg1.to_owned()));
                "ok".to_owned()
            } else {
                "missing key".to_owned()
            }
        }
        "SET_STATE" => {
            // Пока заглушка: прямая запись в реактивные поля через serde
            // потребует downcast к конкретному типу компонента, что реализуется
            // отдельно.
            "not supported yet".to_owned()
        }
        _ => "unknown command".to_owned(),
    }
}

/// Читает следующую команду от клиента; `None` при EOF или ошибке.
fn read_command(stream: &mut TcpStream) -> Option<String> {
    let line = read_line(stream).ok()?;
    if line.is_empty() {
        None
    } else {
        Some(line.trim().to_owned())
    }
}

/// Отправляет строку-ответ клиенту; `None` если соединение обрвано.
fn write_response(stream: &mut TcpStream, text: &str) -> Option<()> {
    writeln!(stream, "{}", text).ok()?;
    stream.flush().ok()?;
    Some(())
}
