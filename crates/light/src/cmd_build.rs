//! Подкоманда `light build`: сборка приложения в release-режиме.
//!
//! Запускает `cargo build --release` (с опциональными фичами и дополнительными
//! аргументами), затем находит собранный бинарный файл и выводит его путь
//! и размер. Перед сборкой автоматически проверяет наличие недостающих
//! компонентов.

use std::path::PathBuf;

use light_fw_cli_core::build::{self, BuildOptions};
use light_fw_cli_core::check::ensure_components_present;
use light_fw_cli_core::process::SystemRunner;

/// Аргументы подкоманды `build`.
#[derive(clap::Args, Debug)]
pub struct BuildArgs {
    /// Каталог проекта (по умолчанию — текущий).
    #[arg(default_value = ".")]
    pub path: PathBuf,
    /// Релизная сборка (по умолчанию). Для отладочной — `--no-release`.
    #[arg(short = 'r', long, default_value_t = true)]
    pub release: bool,
    /// Не релизная сборка.
    #[arg(long, conflicts_with = "release")]
    pub no_release: bool,
    /// Фичи для включения (через запятую).
    #[arg(short = 'f', long)]
    pub features: Vec<String>,
    /// Не включать дефолтные фичи.
    #[arg(long)]
    pub no_default_features: bool,
    /// Не устанавливать недостающие компоненты автоматически (только показать
    /// список и инструкции `light add`).
    #[arg(long, default_value_t = false)]
    pub no_auto_install: bool,
    /// Дополнительные аргументы cargo (передаются как есть).
    #[arg(trailing_var_arg = true)]
    pub extra: Vec<String>,
}

/// Определяет имя бинарного файла через `cargo metadata`.
fn bin_name_from_metadata(cwd: &std::path::Path) -> anyhow::Result<String> {
    let output = std::process::Command::new("cargo")
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(cwd)
        .output()
        .map_err(|e| anyhow::anyhow!("не удалось запустить cargo metadata: {e}"))?;
    if !output.status.success() {
        anyhow::bail!(
            "cargo metadata завершилась с ошибкой: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|e| anyhow::anyhow!("JSON parse: {e}"))?;
    // Берём первый пакет с bintype bin (обычно — корневой проект).
    for pkg in json
        .get("packages")
        .and_then(|p| p.as_array())
        .into_iter()
        .flatten()
    {
        for target in pkg
            .get("targets")
            .and_then(|t| t.as_array())
            .into_iter()
            .flatten()
        {
            if target
                .get("kind")
                .and_then(|k| k.as_array())
                .is_some_and(|kinds| kinds.contains(&serde_json::json!("bin")))
            {
                if let Some(name) = target.get("name").and_then(|n| n.as_str()) {
                    return Ok(name.to_string());
                }
            }
        }
    }
    anyhow::bail!("не удалось определить имя бинарного файла");
}

/// Точка входа подкоманды `light build`.
pub fn run(args: BuildArgs) -> anyhow::Result<i32> {
    let release = args.release && !args.no_release;
    let cwd = PathBuf::from(&args.path);
    if !cwd.exists() {
        anyhow::bail!("каталог {} не существует", args.path.display());
    }

    // Автоматическая проверка компонентов перед сборкой.
    let install_fn = |name: &str| -> light_fw_cli_core::Result<()> {
        // ТЗ 0.17.2: автоподгрузка явно уважает дефолтные фичи `light-fw`
        // (не отключает их); опция прописана в обоих вызовах.
        let opts = light_fw_cli_core::add::AddOptions {
            default_features: true,
            ..Default::default()
        };
        let plan = light_fw_cli_core::add::plan_add(&cwd, name, &opts)?;
        let _ = light_fw_cli_core::add::apply_add(&cwd, &SystemRunner, &plan, &opts)?;
        Ok(())
    };
    let auto_install = !args.no_auto_install;
    let ready = ensure_components_present(&cwd, &install_fn, auto_install)?;
    if !ready {
        return Ok(1);
    }

    let opts = BuildOptions {
        release,
        features: args.features.clone(),
        no_default_features: args.no_default_features,
        extra_cargo_args: args.extra.clone(),
    };

    eprintln!(
        "light build: запускаю `cargo build{}`",
        if release { " --release" } else { "" }
    );

    let runner = SystemRunner;
    let _output = build::run_build(&runner, &cwd, &opts)?;

    // Определяем имя бинарного файла.
    let bin_name = match bin_name_from_metadata(&cwd) {
        Ok(name) => name,
        Err(_) => {
            // Fallback: используем имя пакета.
            eprintln!("light build: не удалось определить имя бинарника, пропускаю поиск файла");
            return Ok(0);
        }
    };

    // Ищем бинарный файл.
    let target_dir = cwd.join("target");
    let binary_path = build::locate_binary(&target_dir, &bin_name, release)?;

    if binary_path.exists() {
        let size = std::fs::metadata(&binary_path)
            .map(|m| m.len())
            .unwrap_or(0);
        println!("Бинарный файл: {}", binary_path.display());
        println!("Размер: {}", build::format_size(size));
    } else {
        eprintln!(
            "light build: бинарный файл не найден по пути {}",
            binary_path.display()
        );
    }

    Ok(0)
}
