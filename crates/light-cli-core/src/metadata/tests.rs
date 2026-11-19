#[allow(clippy::module_inception)]
// ────────────────────────────── Тесты ─────────────────────────────────────────
#[cfg(test)]
mod tests {
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
core = "^0.19"
[package.metadata.light.provides]
elements = ["datepicker"]
"#;

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
        let dir = std::env::temp_dir().join("light-fw-cli-meta-load");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("app")).unwrap();
        std::fs::create_dir_all(dir.join("light-widget-datepicker")).unwrap();
        std::fs::create_dir_all(dir.join("serde")).unwrap();

        std::fs::write(dir.join("app/Cargo.toml"), PLAIN_MANIFEST).unwrap();
        std::fs::write(
            dir.join("light-widget-datepicker/Cargo.toml"),
            VALID_MANIFEST,
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
                { "name": "light-widget-datepicker", "version": "0.19.0", "manifest_path": path("light-widget-datepicker") },
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
        assert_eq!(meta.version, "0.19.0");
        assert_eq!(meta.kind.as_deref(), Some("widget"));
        assert_eq!(meta.provides_elements, vec!["datepicker".to_string()]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn validate_ok_for_compatible_component() {
        let result = validate_metadata(VALID_MANIFEST, "light-widget-datepicker");
        match result {
            ValidationResult::Ok(meta) => {
                assert_eq!(meta.crate_name, "light-widget-datepicker");
                assert_eq!(meta.version, "0.19.0");
                assert_eq!(meta.kind.as_deref(), Some("widget"));
                assert_eq!(meta.core_version_req.as_deref(), Some("^0.19"));
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
core = "^0.19"
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
