#[allow(clippy::module_inception)]
// ────────────────────────────── Тесты ─────────────────────────────────────────
#[cfg(test)]
mod tests {
    #[cfg(test)]
    use crate::metadata::compatible_core_req;
    use crate::metadata::installed::extract_cargo_toml;
    use crate::metadata::*;
    use std::io::Write;

    fn crate_bytes(cargo_toml: &str) -> Vec<u8> {
        use flate2::Compression;
        use flate2::write::GzEncoder;
        use tar::Builder;

        let mut builder = Builder::new(Vec::new());
        let mut header = tar::Header::new_gnu();
        header
            .set_path("light-widget-datepicker-0.20.0/Cargo.toml")
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

    /// Манифест, который **должен** пройти валидацию при любой версии ядра.
    ///
    /// `core` — динамическое требование, гарантированно совместимое с текущей
    /// `LIGHT_CORE_VERSION` (см. [`compatible_core_req`](super::compatible_core_req)).
    /// Без этой правки альфа-ядро (`0.21.0-alpha.1`) не матчит формальный
    /// `^0.20`/`^0.21`, и фикстура падает с `IncompatibleCore`.
    const VALID_MANIFEST_TEMPLATE: &str = r#"
[package]
name = "light-widget-datepicker"
version = "0.20.0"
edition = "2021"

[package.metadata.light]
kind = "widget"
display-name = "Date Picker"
core = "{CORE_REQ}"
[package.metadata.light.provides]
elements = ["datepicker"]
"#;

    /// Возвращает `VALID_MANIFEST_TEMPLATE` с подстановкой актуального
    /// `core`-требования (динамически из `LIGHT_CORE_VERSION`).
    fn valid_manifest() -> String {
        let req = compatible_core_req();
        VALID_MANIFEST_TEMPLATE.replace("{CORE_REQ}", &req)
    }

    const PLAIN_MANIFEST: &str = r#"
[package]
name = "my-app"
version = "0.1.0"
edition = "2021"
"#;

    /// Фикстура `cargo metadata --no-deps`: path-зависимость (path-dependency),
    /// опубликованный компонент и обычный крейт без метаданных.
    #[test]
    fn load_installed_from_json_reads_light_metadata() {
        let valid_manifest = valid_manifest();
        let dir = std::env::temp_dir().join("light-fw-cli-meta-load");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("app")).unwrap();
        std::fs::create_dir_all(dir.join("light-widget-datepicker")).unwrap();
        std::fs::create_dir_all(dir.join("serde")).unwrap();

        std::fs::write(dir.join("app/Cargo.toml"), PLAIN_MANIFEST).unwrap();
        std::fs::write(
            dir.join("light-widget-datepicker/Cargo.toml"),
            &valid_manifest,
        )
        .unwrap();
        std::fs::write(
            dir.join("serde/Cargo.toml"),
            "[package]\nname = \"serde\"\n",
        )
        .unwrap();

        let path = |name: &str| {
            dir.join(name)
                .join("Cargo.toml")
                .to_string_lossy()
                .into_owned()
        };
        // Собираем JSON через serde_json — пути Windows (обратные слэши)
        // экранируются корректно; ручная подстановка строк это сломала бы.
        let json = serde_json::to_string(&serde_json::json!({
            "packages": [
                { "name": "my-app", "version": "0.1.0", "manifest_path": path("app") },
                { "name": "light-widget-datepicker", "version": "0.20.0", "manifest_path": path("light-widget-datepicker") },
                { "name": "serde", "version": "1.0.0", "manifest_path": path("serde") }
            ]
        }))
        .expect("serialize fixture");

        let installed = load_installed_from_json(&json).expect("parse");
        assert_eq!(
            installed.len(),
            1,
            "только пакет с [package.metadata.light]"
        );
        let meta = &installed[0];
        assert_eq!(meta.crate_name, "light-widget-datepicker");
        assert_eq!(meta.version, "0.20.0");
        assert_eq!(meta.kind.as_deref(), Some("widget"));
        assert_eq!(meta.provides_elements, vec!["datepicker".to_string()]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Регрессия 0.20.2: без `--no-deps` в вывод `cargo metadata` попадают и
    /// транзитивные зависимости (например `light-fw-element-form`), поэтому
    /// `<checkbox>`/`<radio>`/`<select>`/`<option>` перестают «пропадать». Здесь
    /// проверяем ровно это: пакет из транзитивных зависимостей с
    /// `[package.metadata.light]` виден, дубликаты по `(name, version)` убираются,
    /// а отсутствующий на диске `manifest_path` не роняет весь парсинг.
    #[test]
    fn load_installed_from_json_sees_transitive_deps_and_dedupes() {
        const FORM_MANIFEST: &str = r#"
[package]
name = "light-fw-element-form"
version = "0.20.2"
edition = "2021"

[package.metadata.light]
kind = "element"
display-name = "Form Elements"
core = "^0.20"
[package.metadata.light.provides]
elements = ["checkbox", "radio", "select", "option"]
"#;

        let dir = std::env::temp_dir().join("light-fw-cli-meta-transitive");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("app")).unwrap();
        std::fs::create_dir_all(dir.join("light-fw-element-form")).unwrap();
        std::fs::write(dir.join("app/Cargo.toml"), PLAIN_MANIFEST).unwrap();
        std::fs::write(dir.join("light-fw-element-form/Cargo.toml"), FORM_MANIFEST).unwrap();

        let path = |name: &str| {
            dir.join(name)
                .join("Cargo.toml")
                .to_string_lossy()
                .into_owned()
        };
        // `packages` — как отдаёт `cargo metadata` БЕЗ `--no-deps`:
        //   1. `light-fw-element-form` один раз (транзитивная зависимость);
        //   2. `light-fw-element-form` повторно (дубль по (name, version));
        //   3. `ghost` — manifest_path отсутствует на диске (крайний случай).
        let json = serde_json::to_string(&serde_json::json!({
            "packages": [
                { "name": "light-fw-element-form", "version": "0.20.2", "manifest_path": path("light-fw-element-form") },
                { "name": "light-fw-element-form", "version": "0.20.2", "manifest_path": path("light-fw-element-form") },
                { "name": "ghost", "version": "9.9.9", "manifest_path": path("missing-dir-missing-file") }
            ]
        }))
        .expect("serialize fixture");

        let installed = load_installed_from_json(&json).expect("parse");
        assert_eq!(
            installed.len(),
            1,
            "один уникальный пакет с метаданными; дубль и ghost исключены"
        );
        let meta = &installed[0];
        assert_eq!(meta.crate_name, "light-fw-element-form");
        assert_eq!(meta.version, "0.20.2");
        assert_eq!(
            meta.provides_elements,
            vec![
                "checkbox".to_string(),
                "radio".to_string(),
                "select".to_string(),
                "option".to_string()
            ]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn validate_ok_for_compatible_component() {
        let valid_manifest = valid_manifest();
        let expected_req = compatible_core_req();
        let result = validate_metadata(&valid_manifest, "light-widget-datepicker");
        match result {
            ValidationResult::Ok(meta) => {
                assert_eq!(meta.crate_name, "light-widget-datepicker");
                assert_eq!(meta.version, "0.20.0");
                assert_eq!(meta.kind.as_deref(), Some("widget"));
                // `core`-requirement в метаданных — то же динамическое
                // требование, которое мы подставили в фикстуру (см. valid_manifest).
                assert_eq!(
                    meta.core_version_req.as_deref(),
                    Some(expected_req.as_str())
                );
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
core = "^0.19"
[package.metadata.light.provides]
attributes = ["data-focus"]
"#;
        let result = validate_metadata(toml, "light-behavior-focus");
        match result {
            ValidationResult::Fatal(ValidationError::IncompatibleCore { required, current }) => {
                assert_eq!(required, "^0.19");
                assert_eq!(current, LIGHT_CORE_VERSION);
            }
            other => panic!("ожидался IncompatibleCore, получен {other:?}"),
        }
    }

    #[test]
    fn validate_empty_provides_is_warning_not_fatal() {
        // Pre-release-ядро (`0.21.0-alpha.1`) требует pre-release в req,
        // иначе формальный semver вернёт IncompatibleCore. Используем хелпер
        // compatible_core_req(), который динамически строит совместимое
        // требование из `LIGHT_CORE_VERSION`.
        let req = compatible_core_req();
        let toml = format!(
            r#"
[package]
name = "light-subsystem-a11y"
version = "0.1.0"
edition = "2021"

[package.metadata.light]
kind = "subsystem"
display-name = "A11y"
core = "{req}"
"#
        );
        let result = validate_metadata(&toml, "light-subsystem-a11y");
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
        let bytes = crate_bytes(&valid_manifest());
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

    /// Единое правило pre-release (FIX7): pre-release и build-метаданные
    /// версии *ядра* игнорируются при проверке `core`-требования.
    ///
    /// Это согласует CLI-валидацию (`check_core`) с runtime-матчером
    /// `light-vdom::plugins::check_core_version` (parse_triple, FIX1),
    /// который уже срезает pre-release. Тест строится без хардкода версий:
    /// все значения выводятся из `LIGHT_CORE_VERSION`.
    ///
    /// **До** FIX7 (формальный semver): `^0.21` НЕ матчил `0.21.0-alpha.1`,
    /// потому что semver 2.0 запрещает матч pre-release-версии release-req'ом.
    /// **После** FIX7: `check_core` срезает pre-release у current до матча,
    /// поэтому `^0.21` матчит `0.21.0` (effective) и валидация проходит.
    #[test]
    fn unified_prerelease_rule() {
        use crate::metadata::types::LIGHT_CORE_VERSION;
        use semver::Version;

        let cur = Version::parse(LIGHT_CORE_VERSION).expect("parse LIGHT_CORE_VERSION");
        let is_prerelease = !cur.pre.is_empty();

        // Вычисляем major/minor из LIGHT_CORE_VERSION (без pre-release).
        let base = LIGHT_CORE_VERSION
            .split(['-', '+'])
            .next()
            .unwrap_or(LIGHT_CORE_VERSION);
        let parts: Vec<&str> = base.split('.').collect();
        assert!(parts.len() >= 2, "LIGHT_CORE_VERSION must have major.minor");
        let (major, minor) = (parts[0].to_string(), parts[1].to_string());

        // 1. ^MAJOR.MINOR — совместимо (основной кейс light-ui: core = "^0.21").
        let current_caret = format!("^{major}.{minor}");
        if is_prerelease {
            // До FIX7 это было Fatal; после — Ok (или Warning, если provides пуст).
            let manifest = VALID_MANIFEST_TEMPLATE.replace("{CORE_REQ}", &current_caret);
            let result = validate_metadata(&manifest, "light-widget-datepicker");
            assert!(
                matches!(result, ValidationResult::Ok(_)),
                "unified rule: {current_caret} должно пройти на {LIGHT_CORE_VERSION} (pre-release ядра игнорируется), получен {result:?}"
            );
        }

        // 2. ^0.(MINOR-1) — несовместимо (негатив сохранён).
        if major == "0" {
            let m: u32 = minor.parse().expect("minor is numeric");
            if m > 0 {
                let prev_caret = format!("^0.{}", m - 1);
                let manifest = VALID_MANIFEST_TEMPLATE.replace("{CORE_REQ}", &prev_caret);
                let result = validate_metadata(&manifest, "light-widget-datepicker");
                match result {
                    ValidationResult::Fatal(ValidationError::IncompatibleCore {
                        required,
                        current,
                    }) => {
                        assert_eq!(required, prev_caret);
                        // В ошибке — оригинальный current (с pre-release), а не effective.
                        assert_eq!(current, LIGHT_CORE_VERSION);
                    }
                    other => panic!(
                        "unified rule: {prev_caret} должно быть IncompatibleCore, получен {other:?}"
                    ),
                }
            }
        }

        // 3. ^(MAJOR+1).MINOR или ^0.(MINOR+1) — несовместимо (требование выше текущей).
        let ahead_caret = if major == "0" {
            let m: u32 = minor.parse().expect("minor is numeric");
            format!("^0.{}", m + 1)
        } else {
            let mj: u32 = major.parse().expect("major is numeric");
            format!("^{}.{}", mj + 1, minor)
        };
        let manifest = VALID_MANIFEST_TEMPLATE.replace("{CORE_REQ}", &ahead_caret);
        let result = validate_metadata(&manifest, "light-widget-datepicker");
        match result {
            ValidationResult::Fatal(ValidationError::IncompatibleCore { required, .. }) => {
                assert_eq!(required, ahead_caret);
            }
            other => panic!(
                "unified rule: {ahead_caret} должно быть IncompatibleCore, получен {other:?}"
            ),
        }

        // 4. "not-a-req" — недопустимое значение в TOML. `parse_manifest_from_text`
        //    возвращает `Ok(None)` (секция `[package.metadata.light]` не распознана),
        //    и `validate_metadata` возвращает `Fatal(NoMetadata)` **до** вызова
        //    `check_core`. Это фиксирует порядок: манифест-парсинг первичен,
        //    semver-валидация вторична.
        let manifest = VALID_MANIFEST_TEMPLATE.replace("{CORE_REQ}", "not-a-req");
        let result = validate_metadata(&manifest, "light-widget-datepicker");
        assert!(
            matches!(result, ValidationResult::Fatal(ValidationError::NoMetadata)),
            "unified rule: \"not-a-req\" должно быть Fatal(NoMetadata) (TOML-ошибка), получен {result:?}"
        );
    }

    /// Фиксатор правила semver 2.0 про pre-release (до FIX7):
    /// формальный semver **не матчит** pre-release-версию release-requirement'ом
    /// без pre-release. После FIX7 `check_core` срезает pre-release у current,
    /// поэтому bare caret ПРОХОДИТ — этот тест фиксирует, что формальный матчер
    /// по-прежнему несовместим (для понимания, почему срезка необходима).
    ///
    /// Все строки выводятся из `LIGHT_CORE_VERSION` — нет хардкода эпохи.
    #[test]
    fn semver_prerelease_rule_pinned() {
        use crate::metadata::types::LIGHT_CORE_VERSION;
        use semver::{BuildMetadata, Prerelease, Version, VersionReq};

        // Строим «голый» caret на текущем major.minor (без pre-release).
        let base = LIGHT_CORE_VERSION
            .split(['-', '+'])
            .next()
            .unwrap_or(LIGHT_CORE_VERSION);
        let parts: Vec<&str> = base.split('.').collect();
        let bare_caret = if parts.len() >= 2 {
            format!("{}.{}", parts[0], parts[1])
        } else {
            LIGHT_CORE_VERSION.to_string()
        };

        // Формальный semver: pre-release не матчит без pre в req.
        let req = VersionReq::parse(&bare_caret).expect("parse req");
        let cur = Version::parse(LIGHT_CORE_VERSION).expect("parse current");
        let is_prerelease = !cur.pre.is_empty();
        if is_prerelease {
            assert!(
                !req.matches(&cur),
                "formal semver: {bare_caret} должно НЕ матчить {LIGHT_CORE_VERSION} (pre-release)"
            );
        } else {
            assert!(
                req.matches(&cur),
                "formal semver: {bare_caret} должно матчить stable {LIGHT_CORE_VERSION}"
            );
        }

        // Единое правило (FIX7): effective = current без pre/build → caret матчит.
        let mut effective = cur.clone();
        effective.pre = Prerelease::EMPTY;
        effective.build = BuildMetadata::EMPTY;
        assert!(
            req.matches(&effective),
            "unified rule: {bare_caret} должно матчить effective {effective:?}"
        );

        // Хелпер [`compatible_core_req`](super::compatible_core_req) —
        // гарантированно совместим с текущей версией.
        let helper_req = compatible_core_req();
        let helper_req = VersionReq::parse(&helper_req).expect("parse helper req");
        assert!(
            helper_req.matches(&cur),
            "compatible_core_req() = {helper_req:?} должно матчить {LIGHT_CORE_VERSION}"
        );

        // validate_metadata с bare caret: ПОСЛЕ FIX7 — всегда Ok
        // (pre-release ядра игнорируется единым правилом).
        let manifest = VALID_MANIFEST_TEMPLATE.replace("{CORE_REQ}", &bare_caret);
        let result = validate_metadata(&manifest, "light-widget-datepicker");
        assert!(
            matches!(result, ValidationResult::Ok(_)),
            "unified rule: bare caret {bare_caret} на {LIGHT_CORE_VERSION} должно быть Ok, получен {result:?}"
        );
    }

    /// Release-gate: реальный манифест `crates/light-ui/Cargo.toml` (с
    /// `core = "^0.21"` и 4 элементами) должен проходить валидацию против
    /// текущей версии ядра (`LIGHT_CORE_VERSION`). До FIX7 это было
    /// `Fatal(IncompatibleCore)` из-за формального semver; после — `Ok`.
    /// Тест падает, если кто-то изменит `core` в light-ui на несовместимое
    /// значение или сломает единственное правило pre-release в `check_core`.
    #[test]
    fn own_light_ui_manifest_validates_against_current_core() {
        let p = concat!(env!("CARGO_MANIFEST_DIR"), "/../light-ui/Cargo.toml");
        let text = std::fs::read_to_string(p).unwrap_or_else(|e| {
            panic!("не удалось прочитать light-ui/Cargo.toml по пути {p}: {e}")
        });
        let result = validate_metadata(&text, "light-ui");
        assert!(
            matches!(result, ValidationResult::Ok(_)),
            "light-ui manifest (core = \"^0.21\") должен быть Ok на ядре {}, получен {result:?}",
            crate::metadata::types::LIGHT_CORE_VERSION
        );
    }
}
