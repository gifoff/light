//! ТЗ 0.16.0: подкоманда `light devtools` — агентное управление DevTools по TCP.
//!
//! Каждый подподкоманда отправляет одну команду на `127.0.0.1:<port>` (по
//! умолчанию 9229) и печатает ответ. При активной сессии (`session start`)
//! все команды логируются в `.light/agent-sessions/<id>/actions.jsonl`, а
//! скриншоты сохраняются в `screenshots/`.

use std::path::PathBuf;

use clap::{Args, Subcommand, ValueEnum};
use light_fw_cli_core::devtools::{
    DEFAULT_PORT, DevtoolsRequest, parse_attr, save_png, send_request,
};
use serde_json::Value;

/// Аргументы подкоманды `devtools`.
#[derive(Args, Debug)]
pub struct DevtoolsArgs {
    #[command(subcommand)]
    pub cmd: DevtoolsCmd,
}

/// Подкоманды `light devtools`.
#[derive(Subcommand, Debug)]
pub enum DevtoolsCmd {
    /// Вывести дерево VDOM (ключи, теги, текст, дети).
    Tree(Shell),
    /// Вывести дерево layout (прямые углы x,y,w,h).
    Layout(Shell),
    /// Найти узел по запросу JSON `{"tag":..,"text":..,"attr":{..}}`.
    Find(FindArgs),
    /// Клик по ключу или координатам.
    Click(ClickArgs),
    /// Ввести текст в поле.
    Type(TypeArgs),
    /// Отправить именованную клавишу (Enter/Tab/Escape).
    Key(KeyArgs),
    /// Снимок состояния приложения (JSON).
    State(Shell),
    /// Скриншот (base64 PNG); при `--path` сохраняет файл.
    Shot(ShotArgs),
    /// Управление session recorder: start / stop.
    Session(SessionArgs),
}

/// Общий «холщовой» аргумент для команд без параметров.
#[derive(Args, Debug)]
pub struct Shell {
    /// Порт DevTools (по умолчанию 9229).
    #[arg(long, default_value_t = DEFAULT_PORT)]
    port: u16,
    /// Выводить JSON как есть (без форматирования).
    #[arg(long, default_value_t = false)]
    json: bool,
}

#[derive(Args, Debug)]
pub struct FindArgs {
    /// Порт DevTools (по умолчанию 9229).
    #[arg(long, default_value_t = DEFAULT_PORT)]
    port: u16,
    /// Выдавать JSON как есть.
    #[arg(long, default_value_t = false)]
    json: bool,
    /// Фильтр по тегу.
    #[arg(long)]
    tag: Option<String>,
    /// Фильтр по точному тексту.
    #[arg(long)]
    text: Option<String>,
    /// Фильтр по атрибутам `k=v` (повторяемо).
    #[arg(long, num_args(0..))]
    attr: Vec<String>,
}

#[derive(Args, Debug)]
pub struct ClickArgs {
    /// Порт DevTools (по умолчанию 9229).
    #[arg(long, default_value_t = DEFAULT_PORT)]
    port: u16,
    /// Выдавать JSON как есть.
    #[arg(long, default_value_t = false)]
    json: bool,
    /// Ключ узла (стабильный путь от корня VDOM).
    #[arg(long, required = true)]
    key: String,
}

#[derive(Args, Debug)]
pub struct TypeArgs {
    /// Порт DevTools (по умолчанию 9229).
    #[arg(long, default_value_t = DEFAULT_PORT)]
    port: u16,
    /// Выдавать JSON как есть.
    #[arg(long, default_value_t = false)]
    json: bool,
    /// Ключ текстового поля.
    #[arg(long, required = true)]
    key: String,
    /// Текст для ввода.
    #[arg(required = true)]
    text: Vec<String>,
}

#[derive(Args, Debug)]
pub struct KeyArgs {
    /// Порт DevTools (по умолчанию 9229).
    #[arg(long, default_value_t = DEFAULT_PORT)]
    port: u16,
    /// Выдавать JSON как есть.
    #[arg(long, default_value_t = false)]
    json: bool,
    /// Ключ фокусного узла.
    #[arg(long, required = true)]
    key: String,
    /// Именованная клавиша.
    #[arg(required = true, value_enum)]
    named: NamedKey,
}

#[derive(Clone, Copy, ValueEnum, Debug)]
enum NamedKey {
    Enter,
    Tab,
    Escape,
}

#[derive(Args, Debug)]
pub struct ShotArgs {
    /// Порт DevTools (по умолчанию 9229).
    #[arg(long, default_value_t = DEFAULT_PORT)]
    port: u16,
    /// Выдавать JSON как есть.
    #[arg(long, default_value_t = false)]
    json: bool,
    /// Каталог, куда сохранить PNG (иначе — только base64 в stdout).
    #[arg(long)]
    path: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub struct SessionArgs {
    /// Действие: start / stop.
    #[arg(value_enum)]
    action: SessionAction,
}

#[derive(Clone, Copy, ValueEnum, Debug)]
enum SessionAction {
    Start,
    Stop,
}

/// Точка входа подкоманды `light devtools`.
pub fn run(args: DevtoolsArgs) -> anyhow::Result<i32> {
    let cwd = std::env::current_dir()?;
    if let DevtoolsCmd::Session(s) = &args.cmd {
        // session start/stop не логятся сами (и не требуют запущенного app).
        match s.action {
            SessionAction::Start => {
                let id = light_fw_cli_core::session::start_session(&cwd)?;
                println!("Session started: .light/agent-sessions/{id}");
            }
            SessionAction::Stop => {
                let cur = light_fw_cli_core::session::active_session_id(&cwd)
                    .ok_or_else(|| anyhow::anyhow!("no active session"))?;
                let summary = light_fw_cli_core::session::stop_session(&cwd, &cur)?;
                println!(
                    "Session stopped. Summary: {:?}",
                    summary.strip_prefix(&cwd)?
                );
            }
        }
        return Ok(0);
    }

    let (port, json, req) = build_request(&args.cmd)?;
    let raw = match send_request(port, &req) {
        Ok(r) => r,
        Err(e) => {
            // Команда пишется в actions.jsonl вместе с ошибкой (например,
            // «Приложение не запущено»), чтобы журнал отражал реальную работу агента.
            log_error(&cwd, &req, &e.to_string())?;
            return Err(anyhow::Error::from(e));
        }
    };
    // shot: при --path сохраняем PNG в пользовательский каталог; в активной
    // сессии дополнительно копируем в screenshots/ и логируем в actions.jsonl.
    if req.cmd == "shot" {
        if let Some(rel) = light_fw_cli_core::session::save_screenshot(&cwd, &raw, "shot")? {
            log_shot_with_screenshot(&cwd, &rel, &raw)?;
        } else {
            log_action(&cwd, &req, &raw)?;
        }
        if let Some(path) = &req.save_path {
            let p = save_png(&raw, path)?;
            println!("{}", p.display());
        } else if json {
            println!("{raw}");
        } else {
            let b64 = raw.strip_prefix("base64:").unwrap_or(&raw);
            let n = b64.len();
            println!("screenshot: {n} base64 chars (~{} KB)", n * 3 / 4 / 1024);
        }
        return Ok(0);
    }
    log_action(&cwd, &req, &raw)?;
    print_result(&raw, json);
    Ok(0)
}

fn build_request(cmd: &DevtoolsCmd) -> anyhow::Result<(u16, bool, DevtoolsRequest)> {
    Ok(match cmd {
        DevtoolsCmd::Tree(s) => (s.port, s.json, simple("tree")),
        DevtoolsCmd::Layout(s) => (s.port, s.json, simple("layout")),
        // CLI `state` → серверная команда SNAPSHOT (глобальный снимок JSON).
        DevtoolsCmd::State(s) => (
            s.port,
            s.json,
            DevtoolsRequest {
                cmd: "state".into(),
                body: None,
                save_path: None,
                json: s.json,
            },
        ),
        DevtoolsCmd::Find(f) => {
            let mut obj = serde_json::Map::new();
            if let Some(t) = &f.tag {
                obj.insert("tag".into(), Value::String(t.clone()));
            }
            if let Some(t) = &f.text {
                obj.insert("text".into(), Value::String(t.clone()));
            }
            if !f.attr.is_empty() {
                let mut attrs = serde_json::Map::new();
                for a in &f.attr {
                    let (k, v) = parse_attr(a)?;
                    attrs.insert(k, Value::String(v));
                }
                obj.insert("attr".into(), Value::Object(attrs));
            }
            (
                f.port,
                f.json,
                DevtoolsRequest {
                    cmd: "find".into(),
                    body: Some(Value::Object(obj).to_string()),
                    save_path: None,
                    json: f.json,
                },
            )
        }
        DevtoolsCmd::Click(c) => (
            c.port,
            c.json,
            DevtoolsRequest {
                cmd: "click".into(),
                body: Some(format!("click {}", c.key)),
                save_path: None,
                json: c.json,
            },
        ),
        DevtoolsCmd::Type(t) => {
            let text = t.text.join(" ");
            (
                t.port,
                t.json,
                DevtoolsRequest {
                    cmd: "type".into(),
                    body: Some(format!("type {} {text}", t.key)),
                    save_path: None,
                    json: t.json,
                },
            )
        }
        DevtoolsCmd::Key(k) => {
            let name = match k.named {
                NamedKey::Enter => "Enter",
                NamedKey::Tab => "Tab",
                NamedKey::Escape => "Escape",
            };
            (
                k.port,
                k.json,
                DevtoolsRequest {
                    cmd: "key".into(),
                    body: Some(format!("key {} {name}", k.key)),
                    save_path: None,
                    json: k.json,
                },
            )
        }
        DevtoolsCmd::Shot(s) => (
            s.port,
            s.json,
            DevtoolsRequest {
                cmd: "shot".into(),
                body: None,
                save_path: s.path.clone(),
                json: s.json,
            },
        ),
        DevtoolsCmd::Session(_) => unreachable!("handled earlier"),
    })
}

fn simple(cmd: &str) -> DevtoolsRequest {
    DevtoolsRequest {
        cmd: cmd.to_owned(),
        body: None,
        save_path: None,
        json: false,
    }
}

fn print_result(raw: &str, json: bool) {
    if json {
        println!("{raw}");
        return;
    }
    // Понятный вывод для человека.
    match raw {
        "ok" | "null" | "timeout" => println!("{raw}"),
        s if s.starts_with("error") => eprintln!("{s}"),
        s => {
            // Пытаемся красиво отформатировать JSON.
            if let Ok(v) = serde_json::from_str::<Value>(s) {
                match serde_json::to_string_pretty(&v) {
                    Ok(p) => println!("{p}"),
                    Err(_) => println!("{s}"),
                }
            } else {
                println!("{s}");
            }
        }
    }
}

fn log_action(cwd: &std::path::Path, req: &DevtoolsRequest, raw: &str) -> anyhow::Result<()> {
    use light_fw_cli_core::session;
    let Some(_id) = session::active_session_id(cwd) else {
        return Ok(());
    };
    // args: ре-парсим из body (лучшие усилия).
    let args_val = match req.body.as_deref() {
        None => Value::Null,
        Some(b) if req.cmd == "find" => {
            serde_json::from_str::<Value>(b).unwrap_or(Value::String(b.to_owned()))
        }
        Some(b) => Value::String(b.to_owned()),
    };
    // result: для JSON-ответов парсим, иначе строка.
    let result_val = if raw.starts_with("base64:") {
        Value::String("<png>".into())
    } else {
        serde_json::from_str::<Value>(raw).unwrap_or(Value::String(raw.to_owned()))
    };
    session::record_action(cwd, &req.cmd, &args_val, &result_val, None)?;
    Ok(())
}

/// Логгирует `shot`, уже сохранивший PNG в `screenshots/` активной сессии,
/// ссылаясь на относительный путь `rel`.
fn log_shot_with_screenshot(cwd: &std::path::Path, rel: &str, raw: &str) -> anyhow::Result<()> {
    use light_fw_cli_core::session;
    let result_val = Value::String("<png>".into());
    session::record_action(cwd, "shot", &Value::Null, &result_val, Some(rel))?;
    let _ = raw;
    Ok(())
}

/// Логгирует команду с ошибкой (без активного соединения) — журнал отражает
/// и неудачные действия агента. Результат записывается как строка с ошибкой.
fn log_error(cwd: &std::path::Path, req: &DevtoolsRequest, err: &str) -> anyhow::Result<()> {
    use light_fw_cli_core::session;
    let args_val = match req.body.as_deref() {
        None => Value::Null,
        Some(b) if req.cmd == "find" => {
            serde_json::from_str::<Value>(b).unwrap_or(Value::String(b.to_owned()))
        }
        Some(b) => Value::String(b.to_owned()),
    };
    let result_val = Value::String(format!("error: {err}"));
    session::record_action(cwd, &req.cmd, &args_val, &result_val, None)?;
    Ok(())
}
