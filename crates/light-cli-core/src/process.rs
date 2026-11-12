//! Обёртка над запуском внешних процессов.
//!
//! Реальная работа с `cargo`/`rustc`/`rustup` идёт через трейт [`CommandRunner`],
//! что позволяет тестировать логику CLI без запуска настоящих процессов
//! (в тестах подставлятся мок-раннер).
//!
//! ТЗ 0.17.4: [`SystemRunner`] поддерживает индикатор прогресса — во время
//! блокирующего вызова (`output()`, `wait()`) фоновый поток выводит в stderr
//! `\r⏳ <label> (<n>с)` каждые 2 секунды, чтобы пользователь видел, что
//! процесс ещё жив. При завершении строка очищается.
//!
//! ТЗ 0.17.5: при `run_with_progress` stdout/stderr дочернего процесса
//! перехватываются (`Stdio::piped()`) и накапливаются в фоне — на экране
//! виден ТОЛЬКО спиннер. При ненулевом коде выхода накопленный вывод
//! выводится в stderr, чтобы пользователь увидел причину ошибки.

use std::io::{Read as _, Write as _};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use crate::error::{Error, Result};

/// Результат запуска внешней команды.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    /// Код возврата (`-1`, если процесс завершён сигналом).
    pub status: i32,
    /// Стандартный вывод.
    pub stdout: String,
    /// Стандартный поток ошибок.
    pub stderr: String,
}

impl CommandOutput {
    /// Успешно ли завершилась команда.
    pub fn success(&self) -> bool {
        self.status == 0
    }
}

/// Запускает внешние команды. Реализация по умолчанию — [`SystemRunner`].
pub trait CommandRunner: Send + Sync {
    /// Запускает `program` с аргументами `args` в каталоге `cwd`.
    fn run(&self, program: &str, args: &[String], cwd: Option<&Path>) -> Result<CommandOutput>;

    /// Запускает команду с индикатором прогресса в stderr.
    ///
    /// Во время выполнения процесса фоновый поток выводит в stderr строку
    /// `\r⏳ <label> (<n>с)` каждые 2 секунды (обновляя время), и очищает её
    /// при завершении. По умолчанию делегирует в [`Self::run`] (без спиннера)
    /// — достаточно для мок-раннеров в тестах.
    fn run_with_progress(
        &self,
        program: &str,
        args: &[String],
        cwd: Option<&Path>,
        label: &str,
    ) -> Result<CommandOutput> {
        let _ = label;
        self.run(program, args, cwd)
    }
}

/// Раннер, запускающий реальные процессы через [`std::process::Command`].
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemRunner;

impl CommandRunner for SystemRunner {
    fn run(&self, program: &str, args: &[String], cwd: Option<&Path>) -> Result<CommandOutput> {
        let mut command = Command::new(program);
        command.args(args);
        if let Some(dir) = cwd {
            command.current_dir(dir);
        }
        let output = command.output().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Error::CommandNotFound(program.to_string())
            } else {
                Error::Io(e)
            }
        })?;
        Ok(CommandOutput {
            status: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }

    fn run_with_progress(
        &self,
        program: &str,
        args: &[String],
        cwd: Option<&Path>,
        label: &str,
    ) -> Result<CommandOutput> {
        let mut command = Command::new(program);
        command.args(args);
        if let Some(dir) = cwd {
            command.current_dir(dir);
        }

        // Перехватываем stdout/stderr дочернего процесса: на экране виден
        // только спиннер; вывод накапливается в фоне и выводится при ошибке.
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());

        // Спинар: фоновый поток пишет `\r⏳ label (Nс)` в stderr каждые 2 с.
        let stop = Arc::new(AtomicBool::new(false));
        let spinner = start_spinner(label.to_string(), stop.clone());

        // Запускаем процесс; при ошибке запуска — останавливаем спиннер сразу.
        let mut child = match command.spawn() {
            Ok(c) => c,
            Err(e) => {
                stop.store(true, Ordering::Relaxed);
                let _ = spinner.join();
                return if e.kind() == std::io::ErrorKind::NotFound {
                    Err(Error::CommandNotFound(program.to_string()))
                } else {
                    Err(Error::Io(e))
                };
            }
        };

        // Фоновые потоки читают stdout и stderr в буферы (экран чистый).
        let stdout_pipe = child.stdout.take();
        let stderr_pipe = child.stderr.take();
        let stdout_buf: Arc<std::sync::Mutex<Vec<u8>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));
        let stderr_buf: Arc<std::sync::Mutex<Vec<u8>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));

        let out_handle = if let Some(pipe) = stdout_pipe {
            let buf = Arc::clone(&stdout_buf);
            Some(thread::spawn(move || {
                let mut reader = std::io::BufReader::new(pipe);
                let mut locked = buf.lock().unwrap();
                let _ = reader.read_to_end(&mut locked);
            }))
        } else {
            None
        };
        let err_handle = if let Some(pipe) = stderr_pipe {
            let buf = Arc::clone(&stderr_buf);
            Some(thread::spawn(move || {
                let mut reader = std::io::BufReader::new(pipe);
                let mut locked = buf.lock().unwrap();
                let _ = reader.read_to_end(&mut locked);
            }))
        } else {
            None
        };

        // Ждём завершения процесса.
        let wait_result = child.wait();

        // Останавливаем спиннер.
        stop.store(true, Ordering::Relaxed);
        let _ = spinner.join();

        // Дожидаемся потоков-читальщиков.
        if let Some(h) = out_handle {
            let _ = h.join();
        }
        if let Some(h) = err_handle {
            let _ = h.join();
        }

        let stdout_str = String::from_utf8_lossy(&stdout_buf.lock().unwrap()).into_owned();
        let stderr_str = String::from_utf8_lossy(&stderr_buf.lock().unwrap()).into_owned();

        let status = match wait_result {
            Ok(s) => s,
            Err(e) => {
                // При ошибке wait выведим накопленный вывод (если есть).
                if !stdout_str.is_empty() {
                    eprintln!("{stdout_str}");
                }
                if !stderr_str.is_empty() {
                    eprintln!("{stderr_str}");
                }
                return Err(Error::Io(e));
            }
        };

        // При ненулевом коде вывода печатаем накопленный вывод в stderr,
        // чтобы пользователь увидел причину ошибки.
        let code = status.code().unwrap_or(-1);
        if code != 0 && !stdout_str.is_empty() {
            eprintln!("{stdout_str}");
        }
        if code != 0 && !stderr_str.is_empty() {
            eprintln!("{stderr_str}");
        }

        Ok(CommandOutput {
            status: code,
            stdout: stdout_str,
            stderr: stderr_str,
        })
    }
}

/// Запускает спиннер-поток, который каждые 2 секунды печатает в stderr
/// `\r⏳ <label> (<n>с)`. Поток останавливается при `stop == true`.
fn start_spinner(label: String, stop: Arc<AtomicBool>) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let start = Instant::now();
        loop {
            thread::sleep(Duration::from_secs(2));
            if stop.load(Ordering::Relaxed) {
                break;
            }
            let elapsed = start.elapsed().as_secs();
            let line = format!("\r⏳ {} ({}с)", label, elapsed);
            // `\r` + строка перезаписывает текущий статус без переноса строки.
            // Финальный `\x1b[2K` очистит строку при завершении.
            let mut err = std::io::stderr().lock();
            let _ = write!(err, "{}", line);
            let _ = err.flush();
        }
        // Очищаем строку при завершении.
        let mut err = std::io::stderr().lock();
        let _ = write!(err, "\r\x1b[2K");
        let _ = err.flush();
    })
}

/// Извлекает semver-версию из первой строки вывода `* --version`.
///
/// Формат: `<program> <version> (<hash> <date>)`, например
/// `rustc 1.99.0 (5f94df478 2026-08-27)`.
pub fn parse_program_version(stdout: &str) -> Option<semver::Version> {
    let mut parts = stdout.split_whitespace();
    let _program = parts.next()?;
    let version = parts.next()?;
    semver::Version::parse(version).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_version_line() {
        let v =
            parse_program_version("rustc 1.99.0 (5f94df478 2026-08-27)").expect("version parsed");
        assert_eq!(v, semver::Version::new(1, 99, 0));
    }

    #[test]
    fn version_parse_rejects_garbage() {
        assert!(parse_program_version("no version here").is_none());
        assert!(parse_program_version("").is_none());
    }

    #[test]
    fn run_with_progress_delegates_to_run_for_mock() {
        struct Mock;
        impl CommandRunner for Mock {
            fn run(
                &self,
                program: &str,
                args: &[String],
                _cwd: Option<&Path>,
            ) -> Result<CommandOutput> {
                assert_eq!(program, "echo");
                let out = args.first().cloned().unwrap_or_default();
                Ok(CommandOutput {
                    status: 0,
                    stdout: out,
                    stderr: String::new(),
                })
            }
        }
        let r = Mock.run_with_progress("echo", &["hi".into()], None, "Тест");
        assert!(r.unwrap().success());
    }
}
