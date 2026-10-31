//! TCP-сервер для DevTools: построчный протокол, передающий JSON-снимок
//! состояния UI внешним инструментам (IDE / standalone inspector).
//!
//! Протокол (newline-delimited):
//! * Клиент шлёт `SNAPSHOT\n` — сервер отвечает одной строкой JSON + `\n`
//!   (или `null\n`, если снимка ещё нет).
//! * Клиент шлёт `QUIT\n` — сервер закрывает соединение.
//! * Неизвестная команда — сервер отвечает `unknown command\n` и продолжает
//!   обслуживание.
//!
//! Сервер слушает только на `127.0.0.1` (безопасность). Работа ведётся в
//! отдельном `[std::thread]`, не блокируя UI-поток.

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
        match command.as_str() {
            "SNAPSHOT" => {
                let response = super::read_latest_snapshot().unwrap_or_else(|| "null".to_owned());
                if write_response(&mut stream, &response).is_none() {
                    break;
                }
            }
            "QUIT" => break,
            "" => continue,
            _ => {
                if write_response(&mut stream, "unknown command").is_none() {
                    break;
                }
            }
        }
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
