use std::io::Read as _;
use std::path::Path;

use crate::error::{Error, Result};
use crate::process::{CommandRunner, SystemRunner};

use super::types::{ComponentMetadata, LIGHT_CORE_VERSION, ValidationError};
use crate::manifest::{Provides, parse_manifest_from_text};

/// Собирает метаданные ВСЕХ установленных Light-компонентов в проекте
/// (published, path, git) через `cargo metadata`.
///
/// Вызываемый **без** `--no-deps`: при этом в `packages` попадают не только
/// root-пакет, но и транзитивные зависимости (включая опциональные, активные
/// в текущей feature-конфигурации). Именно так становится видимым
/// `light-fw-element-form` (`checkbox`/`radio`/`select`/`option`) — при
/// `--no-deps` он пропал бы, и `light dev`/`light check` объявляли `<checkbox>`
/// «не установлен», запуская ложный auto-install (регрессия 0.20.2).
///
/// Для каждого пакета, чей `Cargo.toml` (в `.cargo/registry/...` для
/// опубликованных крейтов это `Cargo.toml.orig`) содержит секцию
/// `[package.metadata.light]`, возвращает [`ComponentMetadata`] (имя, версия и
/// `provides` из манифеста). Пакеты без секции пропускаются; один и тот же
/// пакет может появиться несколько раз (workspace-мембры с одинаковыми
/// name+version), дубликаты удаляются по паре `(name, version)`. Если `cargo
/// metadata` завершился ошибкой (нет cargo в PATH, пустой stdout, битый JSON),
/// функция **не** возвращает ошибку — печатает предупреждение на stderr и
/// возвращает пустой вектор. Это делает `light check` устойчивым: отчёт
/// формируется по данным из bundled registry и сканера шаблонов, а «Установлено»
/// будет просто пусто (или некорректно, если компоненты есть, но `cargo`
/// недоступен). Для интерактивного использования это приемлемо: пользователь
/// видит предупреждение и может повторить команду в рабочей среде.
pub fn load_installed(project_root: &Path) -> Result<Vec<ComponentMetadata>> {
    let result = (|| -> Result<Vec<ComponentMetadata>> {
        let runner = SystemRunner;
        let out = runner.run(
            "cargo",
            &["metadata".into(), "--format-version".into(), "1".into()],
            Some(project_root),
        )?;
        load_installed_from_json(&out.stdout)
    })();
    match result {
        Ok(v) => Ok(v),
        Err(e) => {
            eprintln!("warning: cargo metadata failed: {e}");
            Ok(Vec::new())
        }
    }
}

/// Разбирает вывод `cargo metadata` (JSON, format-version 1) и извлекает
/// метаданные компонентов. Вынесено отдельно от запуска процесса, чтобы логику
/// можно было проверить на фикстуре без реального `cargo`.
///
/// Разбирает JSON-ответ `cargo metadata --format-version 1` и извлекает
/// `[package.metadata.light]` из каждого пакета. Пакеты без метаданных
/// пропускаются (не ошибаются). Без `--no-deps` в `packages` попадают
/// транзитивные зависимости: один и тот же пакет может присутствовать несколько
/// раз (workspace-мембры / разные версии), поэтому дубликаты удаляются по паре
/// `(name, version)`. Если `manifest_path` недоступен на диске (крайне редкий
/// случай) — пакет пропускается, а не роняет весь парсинг. Публичный для
/// интеграционных тестов, которые не хотят запускать реальный `cargo metadata`.
pub fn load_installed_from_json(json: &str) -> Result<Vec<ComponentMetadata>> {
    use std::collections::BTreeSet;

    #[derive(serde::Deserialize)]
    struct Metadata {
        packages: Vec<Pkg>,
    }

    #[derive(serde::Deserialize)]
    struct Pkg {
        name: String,
        version: String,
        manifest_path: String,
    }

    let meta: Metadata = serde_json::from_str(json)?;
    let mut installed = Vec::new();
    let mut seen = BTreeSet::new();
    for pkg in meta.packages {
        // Дедупликация по (name, version): без `--no-deps` пакет может
        // встретиться повторно (workspace-мембры с одинаковыми координатами).
        if !seen.insert((pkg.name.clone(), pkg.version.clone())) {
            continue;
        }
        // Читаем манифест терпимо: если файла нет — пропускаем пакет, не
        // прерывая разбор остальных (в отличие от прежнего жёсткого `?`).
        let Ok(text) = std::fs::read_to_string(&pkg.manifest_path) else {
            continue;
        };
        let Ok(Some(manifest)) = parse_manifest_from_text(&text) else {
            continue;
        };
        installed.push(ComponentMetadata {
            crate_name: pkg.name,
            version: pkg.version,
            kind: Some(manifest_kind_str(manifest.kind)),
            core_version_req: Some(manifest.core.clone()),
            provides_elements: manifest.provides.elements,
            provides_attributes: manifest.provides.attributes,
            provides_directives: manifest.provides.directives,
            requires_components: manifest.requires.components,
        });
    }
    Ok(installed)
}

/// Проверяет `core`-требование против текущей версии ядра.
///
/// Возвращает [`Some(ValidationError::IncompatibleCore)`], если requirement
/// не удовлетворяется текущей версией. Ошибка разбора semver трактуется как
/// несовместимость (требование невозможно проверить).
///
/// **Единое правило pre-release** (сходно с `light-vdom::plugins::parse_triple`,
/// FIX1): pre-release и build-метаданные версии *ядра* игнорируются при
/// проверке — `0.21.0-alpha.1` считается `0.21.0`. Это делает `^0.21`
/// совместимым с альфа-ядром, что согласовано с runtime-поведением
/// `light-vdom` (`check_core_version` уже срезает pre-release через
/// `parse_triple`). Известное ограничение: требование с конкретной
/// pre-release (например `>=0.21.0-alpha.1 <0.21.0`) не сравнивается
/// поточечно — pre-release части требования тоже игнорируются, так как
/// формальный semver не матчит их без pre-release в requirement.
pub(crate) fn check_core(core_req: &str) -> Option<ValidationError> {
    use semver::{BuildMetadata, Prerelease, Version, VersionReq};

    let req: VersionReq = match VersionReq::parse(core_req) {
        Ok(r) => r,
        Err(_) => {
            return Some(ValidationError::IncompatibleCore {
                required: core_req.to_string(),
                current: LIGHT_CORE_VERSION.to_string(),
            });
        }
    };
    let current: Version = match Version::parse(LIGHT_CORE_VERSION) {
        Ok(v) => v,
        // Версию самого CLI разобрали мы сами — fallback к "0.0.0", чтобы
        // требование типа `>=0` прошло, а `^0.19` упало.
        Err(_) => return None,
    };
    // Единое правило: срезаем pre-release и build-метаданные у ТЕКУЩЕЙ
    // версии ядра перед матчем (не у требования). Это согласует CLI-валидацию
    // с light-vdom::plugins::check_core_version (parse_triple, FIX1), который
    // уже игнорирует pre-release. Сообщение об ошибке сохраняет оригинальный
    // `current` (с -alpha.1) для диагностики.
    let mut effective = current.clone();
    effective.pre = Prerelease::EMPTY;
    effective.build = BuildMetadata::EMPTY;
    if !req.matches(&effective) {
        return Some(ValidationError::IncompatibleCore {
            required: core_req.to_string(),
            current: current.to_string(),
        });
    }
    None
}

/// Пусто ли `provides` (нет ни элементов, ни атрибутов, ни директив).
pub(crate) fn is_empty_provides(provides: &Provides) -> bool {
    provides.elements.is_empty() && provides.attributes.is_empty() && provides.directives.is_empty()
}

/// Строковое представление `kind` (та же штриховая нотация, что в TOML).
pub(crate) fn manifest_kind_str(kind: crate::manifest::ComponentKind) -> String {
    use crate::manifest::ComponentKind::*;
    let s = match kind {
        Element => "element",
        Behavior => "behavior",
        Widget => "widget",
        Integration => "integration",
        Subsystem => "subsystem",
        Dev => "dev",
    };
    s.to_string()
}

/// Достаёт `version` из секции `[package]` текста `Cargo.toml` (best-effort).
pub(crate) fn manifest_version_from_text(toml_text: &str) -> Option<String> {
    use std::str::FromStr;
    let manifest: toml_edit::DocumentMut = toml_edit::DocumentMut::from_str(toml_text).ok()?;
    let version = manifest.get("package")?.get("version")?;
    version.as_str().map(|v| v.trim_matches('"').to_string())
}

/// Извлекает содержимое первого `*/Cargo.toml` из архива `.crate` (tar.gz).
///
/// `.crate`-артефакт — это tar.gz с единственным верхним каталогом
/// `<name>-<version>/…`. Ищем первый путь, завершающийся `Cargo.toml`.
/// Возвращает текст `Cargo.toml`; если архив битый или файл не найден —
/// [`Error::Manifest`].
pub fn extract_cargo_toml(crate_bytes: &[u8]) -> Result<String> {
    use flate2::read::GzDecoder;
    use tar::Archive;

    let gz = GzDecoder::new(crate_bytes);
    let mut archive = Archive::new(gz);
    for entry in archive
        .entries()
        .map_err(|e| Error::Manifest(format!("битый tar-архив: {e}")))?
    {
        let mut entry =
            entry.map_err(|e| Error::Manifest(format!("ошибка чтения tar-записи: {e}")))?;
        let path = entry
            .path()
            .map_err(|e| Error::Manifest(format!("некорректный путь в tar: {e}")))?
            .to_path_buf();
        // Первый *Cargo.toml на любом уровне.
        if path.file_name().and_then(|n| n.to_str()) == Some("Cargo.toml") {
            let mut buf = Vec::new();
            entry
                .read_to_end(&mut buf)
                .map_err(|e| Error::Manifest(format!("не удалось прочитать Cargo.toml: {e}")))?;
            let text = String::from_utf8(buf)
                .map_err(|e| Error::Manifest(format!("Cargo.toml не UTF-8: {e}")))?;
            return Ok(text);
        }
    }
    Err(Error::Manifest("в .crate не найден Cargo.toml".to_string()))
}
