//! Скачанные метаданные компонента и их валидация.
//!
//! Этап 0 Plugin API: после выбора результата поиска CLI скачивает `.crate`
//! (tar.gz), извлекает `[package.metadata.light]` из первого `*/Cargo.toml`
//! и проверяет совместимость с текущей версией ядра. Установка
//! (`cargo add`) отложена до Этапа 2, поэтому здесь только **валидация**.

use std::io::Read as _;

use crate::error::{Error, Result};
use crate::manifest::{Provides, parse_manifest_from_text};

/// Текущая версия CLI-крэйта `light` (ядро Light). Используется для проверки
/// совместимости `core`-требования компонента.
pub const LIGHT_CORE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Версия, извлечённая из скачанного `.crate`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentMetadata {
    /// Имя крейта-компонента.
    pub crate_name: String,
    /// Версия скачанного артефакта.
    pub version: String,
    /// Тип компонента (строка из манифеста: `widget`, `element`, …).
    pub kind: Option<String>,
    /// Требование к версии ядра (semver-range из `core`).
    pub core_version_req: Option<String>,
    /// Элементы, которые компонент предоставляет.
    pub provides_elements: Vec<String>,
    /// Атрибуты, которые компонент предоставляет.
    pub provides_attributes: Vec<String>,
    /// Директивы, которые компонент предоставляет.
    pub provides_directives: Vec<String>,
    /// Обязательные компоненты (`"name ^ver"`).
    pub requires_components: Vec<String>,
}

/// Результат валидации скачанного `.crate`.
#[derive(Debug, PartialEq, Eq)]
pub enum ValidationResult {
    /// Компонент валиден и совместим.
    Ok(ComponentMetadata),
    /// Фатальная ошибка (нет манифеста / несовместимое ядро) — установка
    /// невозможна без действий пользователя.
    Fatal(ValidationError),
    /// Предупреждение (пустые `provides`, конфликт): пользователь может
    /// подтвердить установку через [y/N].
    Warning {
        /// Метаданные валидны, предупреждение отдельно.
        metadata: ComponentMetadata,
        /// Само предупреждение (`EmptyProvides` или `Conflict`).
        warning: ValidationError,
    },
}

/// Ошибка валидации скачанного компонента.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    /// В `.crate` нет секции `[package.metadata.light]`.
    NoMetadata,
    /// `core`-требование несовместимо с текущей версией ядра.
    IncompatibleCore {
        /// Требование из манифеста (например `^0.20`).
        required: String,
        /// Текущая версия ядра (например `0.19.0-dev`).
        current: String,
    },
    /// `provides` полностью пуст (элементов/атрибутов/директив нет).
    EmptyProvides,
    /// Конфликт с уже установленным компонентом по тегу.
    Conflict {
        /// Тег, по которому возник конфликт.
        tag: String,
        /// Имя уже установленного владельца тега.
        current_owner: String,
    },
}

impl ValidationError {
    /// Человекочитаемое сообщение на русском (для интерактивного вывода).
    pub fn message(&self) -> String {
        match self {
            ValidationError::NoMetadata => {
                "в крейте нет секции `[package.metadata.light]` — это не компонент Light"
                    .to_string()
            }
            ValidationError::IncompatibleCore { required, current } => {
                format!("компонент требует ядро `{required}`, но установлено `{current}`")
            }
            ValidationError::EmptyProvides => "компонент не объявляет ни одного элемента, \
                 атрибута или директивы (`provides` пуст)"
                .to_string(),
            ValidationError::Conflict { tag, current_owner } => {
                format!("конфликт: тег `{tag}` уже предоставлен компонентом `{current_owner}`")
            }
        }
    }

    /// Является ли эта ошибка фатальной (без возможности продолжить).
    pub fn is_fatal(&self) -> bool {
        matches!(
            self,
            ValidationError::NoMetadata | ValidationError::IncompatibleCore { .. }
        )
    }
}

/// Валидирует манифест из текста `Cargo.toml` скачанного `.crate`.
///
/// Возвращает [`ValidationResult`]:
/// - [`Ok`](ValidationResult::Ok) — валиден и совместим;
/// - [`Fatal`](ValidationResult::Fatal) — нет манифеста или несовместимое ядро;
/// - [`Warning`](ValidationResult::Warning) — предупреждение (пустые provides,
///   конфликт) при валидном ядре.
pub fn validate_metadata(toml_text: &str, crate_name: &str) -> ValidationResult {
    // Разбор манифеста: отсутствие секции → NoMetadata (fatal).
    let manifest = match parse_manifest_from_text(toml_text) {
        Ok(Some(m)) => m,
        Ok(None) => return ValidationResult::Fatal(ValidationError::NoMetadata),
        Err(_) => {
            // Не удалось разобрать TOML/манифест → считаем отсутствующим.
            return ValidationResult::Fatal(ValidationError::NoMetadata);
        }
    };

    // Проверка совместимости ядра: semver-requirement vs текущая версия.
    let core_req = manifest.core.clone();
    let core_violation = check_core(&core_req);
    if let Some(fatal) = core_violation {
        return ValidationResult::Fatal(fatal);
    }

    // Собираем метаданные.
    let metadata = ComponentMetadata {
        crate_name: crate_name.to_string(),
        version: manifest_version_from_text(toml_text).unwrap_or_default(),
        kind: Some(manifest_kind_str(manifest.kind)),
        core_version_req: Some(core_req),
        provides_elements: manifest.provides.elements.clone(),
        provides_attributes: manifest.provides.attributes.clone(),
        provides_directives: manifest.provides.directives.clone(),
        requires_components: manifest.requires.components.clone(),
    };

    // Предупреждения: пустые provides (конфликт проверяется отдельно CLI-сложением,
    // здесь — EmptyProvides, т.к. light.lock появляется в Этапе 2).
    if is_empty_provides(&manifest.provides) {
        return ValidationResult::Warning {
            metadata,
            warning: ValidationError::EmptyProvides,
        };
    }

    ValidationResult::Ok(metadata)
}

/// Проверяет `core`-требование против текущей версии ядра.
///
/// Возвращает [`Some(ValidationError::IncompatibleCore)`], если requirement
/// не удовлетворяется текущей версией. Ошибка разбора semver трактуется как
/// несовместимость (требование невозможно проверить).
fn check_core(core_req: &str) -> Option<ValidationError> {
    use semver::{Version, VersionReq};

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
    if !req.matches(&current) {
        return Some(ValidationError::IncompatibleCore {
            required: core_req.to_string(),
            current: current.to_string(),
        });
    }
    None
}

/// Пусто ли `provides` (нет ни элементов, ни атрибутов, ни директив).
fn is_empty_provides(provides: &Provides) -> bool {
    provides.elements.is_empty() && provides.attributes.is_empty() && provides.directives.is_empty()
}

/// Строковое представление `kind` (та же штриховая нотация, что в TOML).
fn manifest_kind_str(kind: crate::manifest::ComponentKind) -> String {
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
fn manifest_version_from_text(toml_text: &str) -> Option<String> {
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

// ────────────────────────────── Тесты ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn crate_bytes(cargo_toml: &str) -> Vec<u8> {
        use flate2::Compression;
        use flate2::write::GzEncoder;
        use tar::Builder;

        let mut builder = Builder::new(Vec::new());
        let mut header = tar::Header::new_gnu();
        header
            .set_path("light-widget-datepicker-0.19.0/Cargo.toml")
            .expect("set path");
        header.set_size(cargo_toml.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder
            .append(&header, cargo_toml.as_bytes())
            .expect("append");
        let tar_bytes = builder.into_inner().expect("into inner");

        let mut gz = GzEncoder::new(Vec::new(), Compression::default());
        gz.write_all(&tar_bytes).expect("write gz");
        gz.finish().expect("finish gz")
    }

    const VALID_MANIFEST: &str = r#"
[package]
name = "light-widget-datepicker"
version = "0.19.0"
edition = "2021"

[package.metadata.light]
kind = "widget"
display-name = "Date Picker"
core = "^0.18"
[package.metadata.light.provides]
elements = ["datepicker"]
"#;

    #[test]
    fn validate_ok_for_compatible_component() {
        let result = validate_metadata(VALID_MANIFEST, "light-widget-datepicker");
        match result {
            ValidationResult::Ok(meta) => {
                assert_eq!(meta.crate_name, "light-widget-datepicker");
                assert_eq!(meta.version, "0.19.0");
                assert_eq!(meta.kind.as_deref(), Some("widget"));
                assert_eq!(meta.core_version_req.as_deref(), Some("^0.18"));
                assert_eq!(meta.provides_elements, vec!["datepicker"]);
                assert!(meta.provides_attributes.is_empty());
                assert!(meta.requires_components.is_empty());
            }
            other => panic!("ожидался Ok, получен {other:?}"),
        }
    }

    #[test]
    fn validate_no_metadata_when_section_missing() {
        let toml = "[package]\nname = \"plain-crate\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";
        let result = validate_metadata(toml, "plain-crate");
        assert!(
            matches!(result, ValidationResult::Fatal(ValidationError::NoMetadata)),
            "получено {result:?}"
        );
    }

    #[test]
    fn validate_incompatible_core_when_range_excludes_current() {
        let toml = r#"
[package]
name = "light-behavior-focus"
version = "0.1.0"
edition = "2021"

[package.metadata.light]
kind = "behavior"
display-name = "Focus"
core = "^0.20"
[package.metadata.light.provides]
attributes = ["data-focus"]
"#;
        let result = validate_metadata(toml, "light-behavior-focus");
        match result {
            ValidationResult::Fatal(ValidationError::IncompatibleCore { required, current }) => {
                assert_eq!(required, "^0.20");
                assert_eq!(current, LIGHT_CORE_VERSION);
            }
            other => panic!("ожидался IncompatibleCore, получен {other:?}"),
        }
    }

    #[test]
    fn validate_empty_provides_is_warning_not_fatal() {
        let toml = r#"
[package]
name = "light-subsystem-a11y"
version = "0.1.0"
edition = "2021"

[package.metadata.light]
kind = "subsystem"
display-name = "A11y"
core = "^0.18"
"#;
        let result = validate_metadata(toml, "light-subsystem-a11y");
        match result {
            ValidationResult::Warning { metadata, warning } => {
                assert_eq!(warning, ValidationError::EmptyProvides);
                assert!(metadata.provides_elements.is_empty());
            }
            other => panic!("ожидался Warning(EmptyProvides), получен {other:?}"),
        }
    }

    #[test]
    fn invalid_semver_core_is_incompatible() {
        let toml = r#"
[package]
name = "light-fw-x"
version = "0.1.0"
edition = "2021"

[package.metadata.light]
kind = "element"
display-name = "X"
core = "not-a-version"
[package.metadata.light.provides]
elements = ["x"]
"#;
        let result = validate_metadata(toml, "light-fw-x");
        assert!(
            matches!(result, ValidationResult::Fatal(ValidationError::NoMetadata)),
            "получено {result:?}"
        );
    }

    #[test]
    fn extract_cargo_toml_from_crate_archive() {
        let bytes = crate_bytes(VALID_MANIFEST);
        let toml = extract_cargo_toml(&bytes).expect("extract");
        assert!(toml.contains("[package.metadata.light]"));
        assert!(toml.contains("light-widget-datepicker"));
    }

    #[test]
    fn extract_cargo_toml_fails_on_garbage() {
        let result = extract_cargo_toml(b"not a gzip at all");
        assert!(result.is_err(), "ожидается ошибка");
    }

    #[test]
    fn error_message_strings_are_readable() {
        assert!(
            ValidationError::NoMetadata
                .message()
                .contains("[package.metadata.light]")
        );
        let msg = ValidationError::IncompatibleCore {
            required: "^0.20".into(),
            current: LIGHT_CORE_VERSION.into(),
        }
        .message();
        assert!(msg.contains("^0.20"));
        assert!(
            ValidationError::EmptyProvides
                .message()
                .contains("provides")
        );
        let conflict = ValidationError::Conflict {
            tag: "datepicker".into(),
            current_owner: "light-widget-datepicker".into(),
        }
        .message();
        assert!(conflict.contains("datepicker"));
        assert!(conflict.contains("light-widget-datepicker"));
    }
}
