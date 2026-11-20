use super::installed::{
    check_core, is_empty_provides, manifest_kind_str, manifest_version_from_text,
};
use super::types::{ComponentMetadata, ValidationError, ValidationResult};
use crate::manifest::parse_manifest_from_text;

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
