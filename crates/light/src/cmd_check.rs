use std::path::PathBuf;

use crate::OutputFormat;
use light_fw_cli_core::check::build_check_report;

/// Аргументы команды `light check`.
#[derive(clap::Args)]
pub struct CheckArgs {
    /// Каталог проекта (содержит `Cargo.toml`).
    #[arg(default_value = ".")]
    pub path: PathBuf,

    /// Формат вывода.
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    pub format: OutputFormat,

    /// Считать «Доступно», «Неизвестно» или «Конфликты» ошибкой (код возврата 1).
    ///
    /// Установленные/неиспользуемые компоненты **не** делают проверку
    /// проваленной в strict-режиме — это индикаторы, а не блокировщики.
    #[arg(long)]
    pub strict: bool,
}

/// Проверяет проект: читает установленные компоненты через `cargo metadata`,
/// сканирует `templates/**/*.html` и классифицирует использованные теги на
/// три категории:
///
/// * 🟢 **установлено** — тег предоставляют уже подключённые крейты;
/// * 🟡 **доступно (установить)** — тег нет, но он есть во встроенном реестре
///   (`light add <crate>`);
/// * 🔴 **неизвестно** — тег не из стандартного HTML и его нет в реестре
///   (возможно, нужен плагин — см. `light search`).
///
/// Дополнительно отчитывается о 🗑 неиспользуемых компонентах и ⚠️ конфликтах
/// (один тег предоставляют ≥2 установленных компонентов).
///
/// Всегда печатает отчёт. Возвращает код 1 при `--strict` (если есть доступные,
/// неизвестные или конфликты) либо при ошибке разбора шаблонов.
pub fn run(args: CheckArgs) -> anyhow::Result<i32> {
    let report = build_check_report(&args.path)?;
    match args.format {
        OutputFormat::Text => print!("{}", report.to_text()),
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&report.to_json())?),
    }

    if args.strict && report.has_problems() {
        return Ok(1);
    }
    Ok(0)
}
