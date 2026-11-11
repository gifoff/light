//! Логика команды `light build`: сборка приложения через `cargo build --release`.
//!
//! Разделение на чистые функции ([`build_args`], [`locate_binary`],
//! [`format_size`]) и запуск процесса ([`run_build`]) позволяет тестировать
//! формирование аргументов и поиск бинарного файла без реального запуска cargo.

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::process::CommandRunner;

/// Опции сборки.
#[derive(Debug, Clone, Default)]
pub struct BuildOptions {
    /// Релизная сборка (по умолчанию).
    pub release: bool,
    /// Фичи для включения (`--features a,b`).
    pub features: Vec<String>,
    /// Не включать дефолтные фичи (`--no-default-features`).
    pub no_default_features: bool,
    /// Дополнительные аргументы, передаваемые в `cargo build` как есть.
    pub extra_cargo_args: Vec<String>,
}

impl BuildOptions {
    /// Опции по умолчанию: релизная сборка без фич.
    pub fn default_release() -> Self {
        Self {
            release: true,
            ..Self::default()
        }
    }
}

/// Формирует вектор аргументов для `cargo build`.
pub fn build_args(opts: &BuildOptions) -> Vec<String> {
    let mut args = vec!["build".to_string()];
    if opts.release {
        args.push("--release".to_string());
    }
    if opts.no_default_features {
        args.push("--no-default-features".to_string());
    }
    if !opts.features.is_empty() {
        args.push("--features".to_string());
        args.push(opts.features.join(","));
    }
    args.extend(opts.extra_cargo_args.iter().cloned());
    args
}

/// Запускает `cargo build` и проверяет код возврата.
pub fn run_build(
    runner: &dyn CommandRunner,
    cwd: &Path,
    opts: &BuildOptions,
) -> Result<crate::process::CommandOutput> {
    let label = if opts.release { "Сборка (cargo build --release)" } else { "Сборка (cargo build)" };
    let output = runner.run_with_progress("cargo", &build_args(opts), Some(cwd), label)?;
    if !output.success() {
        return Err(Error::CommandFailed {
            program: "cargo build".to_string(),
            code: output.status,
            stderr: output.stderr.trim().to_string(),
        });
    }
    Ok(output)
}

/// Определяет путь к собранному бинарнику.
///
/// Ищет `<target_dir>/<release|debug>/<bin_name>` с учётом расширения `.exe`
/// на Windows.
pub fn locate_binary(target_dir: &Path, bin_name: &str, release: bool) -> Result<PathBuf> {
    let profile = if release { "release" } else { "debug" };
    let base = target_dir.join(profile).join(bin_name);
    let candidates = [
        format!("{}.exe", base.display()),
        base.to_string_lossy().into_owned(),
    ];
    for candidate in &candidates {
        let p = PathBuf::from(candidate);
        if p.exists() {
            return Ok(p);
        }
    }
    // Возвращаем ожидаемый путь, даже если файл не найден — вызывающий код
    // может отобразить его пользователю.
    Ok(candidates[0].clone().into())
}

/// Форматирует размер файла в человекочитаемом виде.
pub fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KiB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GiB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_build_args() {
        let opts = BuildOptions::default_release();
        assert_eq!(build_args(&opts), vec!["build", "--release"]);
    }

    #[test]
    fn debug_build_args() {
        let opts = BuildOptions::default();
        assert_eq!(build_args(&opts), vec!["build"]);
    }

    #[test]
    fn build_args_with_features() {
        let mut opts = BuildOptions::default_release();
        opts.features = vec!["a".to_string(), "b".to_string()];
        assert_eq!(
            build_args(&opts),
            vec!["build", "--release", "--features", "a,b"]
        );
    }

    #[test]
    fn build_args_no_default_features() {
        let mut opts = BuildOptions::default_release();
        opts.no_default_features = true;
        assert_eq!(
            build_args(&opts),
            vec!["build", "--release", "--no-default-features"]
        );
    }

    #[test]
    fn build_args_with_extra() {
        let mut opts = BuildOptions::default_release();
        opts.extra_cargo_args = vec!["--jobs".to_string(), "4".to_string()];
        assert_eq!(build_args(&opts), vec!["build", "--release", "--jobs", "4"]);
    }

    #[test]
    fn format_size_bytes() {
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1023), "1023 B");
    }

    #[test]
    fn format_size_kib() {
        assert_eq!(format_size(1024), "1.0 KiB");
        assert_eq!(format_size(1536), "1.5 KiB");
    }

    #[test]
    fn format_size_mib() {
        assert_eq!(format_size(1024 * 1024), "1.0 MiB");
        assert_eq!(format_size(2 * 1024 * 1024 + 512 * 1024), "2.5 MiB");
    }

    #[test]
    fn format_size_gib() {
        assert_eq!(format_size(1024 * 1024 * 1024), "1.0 GiB");
    }

    #[test]
    fn locate_binary_existent() {
        let dir = std::env::temp_dir().join(format!("light-build-test-{}", std::process::id()));
        let release_dir = dir.join("release");
        if let Err(e) = std::fs::create_dir_all(&release_dir) {
            eprintln!("skip: cannot create temp dir: {e}");
            return;
        }
        let bin = release_dir.join("myapp.exe");
        std::fs::write(&bin, b"fake").unwrap();
        let found = locate_binary(&dir, "myapp", true).unwrap();
        assert_eq!(found, bin);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn locate_binary_nonexistent_returns_expected_path() {
        let dir = std::env::temp_dir().join("light-build-missing");
        let expected = dir.join("release").join("nothere.exe");
        let found = locate_binary(&dir, "nothere", true).unwrap();
        assert_eq!(found, expected);
    }
}
