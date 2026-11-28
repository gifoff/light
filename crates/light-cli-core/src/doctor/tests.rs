#[allow(clippy::module_inception)]
#[cfg(test)]
mod tests {
    use crate::doctor::*;
    use std::fs;
    use std::path::{Path, PathBuf};

    struct MockProbe {
        rustc: Option<semver::Version>,
        cargo: Option<semver::Version>,
        rustup: Option<semver::Version>,
        network: bool,
    }

    impl EnvironmentProbe for MockProbe {
        fn rustc_version(&self) -> Option<semver::Version> {
            self.rustc.clone()
        }
        fn cargo_version(&self) -> Option<semver::Version> {
            self.cargo.clone()
        }
        fn rustup_version(&self) -> Option<semver::Version> {
            self.rustup.clone()
        }
        fn network_reachable(&self) -> bool {
            self.network
        }
    }

    fn ok_probe() -> MockProbe {
        MockProbe {
            rustc: Some(semver::Version::new(1, 80, 0)),
            cargo: Some(semver::Version::new(1, 80, 0)),
            rustup: Some(semver::Version::new(1, 27, 0)),
            network: true,
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("light-fw-cli-doctor-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn write_project(dir: &Path, cargo: &str, templates: &[(&str, &str)]) {
        fs::write(dir.join("Cargo.toml"), cargo).expect("write Cargo.toml");
        if !templates.is_empty() {
            let tdir = dir.join("templates");
            fs::create_dir_all(&tdir).expect("create templates");
            for (name, content) in templates {
                fs::write(tdir.join(name), content).expect("write template");
            }
        }
    }

    #[test]
    fn detects_core_version_in_workspace() {
        let root = temp_dir("workspace-core");
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"app\", \"light\"]\nresolver = \"2\"\n\n\
             [workspace.package]\nversion = \"0.2.3\"\n\n\
             [workspace.dependencies]\nlight = { path = \"light\" }\n",
        )
        .expect("root manifest");
        let app = root.join("app");
        fs::create_dir_all(&app).expect("app");
        fs::write(
            app.join("Cargo.toml"),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
             [dependencies]\nlight = { workspace = true }\n",
        )
        .expect("app manifest");
        let light = root.join("light");
        fs::create_dir_all(light.join("src")).expect("light src");
        fs::write(light.join("src/lib.rs"), "").expect("light lib");
        fs::write(
            light.join("Cargo.toml"),
            "[package]\nname = \"light\"\nversion.workspace = true\nedition = \"2021\"\n",
        )
        .expect("light manifest");

        let report = doctor(&app, &ok_probe()).expect("doctor");
        assert_eq!(report.project.core_version.as_deref(), Some("0.2.3"));
        assert!(
            !report.issues.iter().any(|i| i
                .message
                .contains("не удалось определить версию light-fw-core")),
            "ядро определено: {:?}",
            report.issues
        );
    }

    #[test]
    fn healthy_project_has_no_errors() {
        let dir = temp_dir("healthy");
        write_project(
            &dir,
            "[package]\nname = \"p\"\nversion = \"0.1.0\"\n[workspace]\n\n[dependencies]\nlight = \"0.1\"\n",
            &[("app.html", "<div></div>")],
        );
        let report = doctor(&dir, &ok_probe()).expect("doctor");
        assert!(!report.has_errors(), "issues: {:?}", report.issues);
        assert_eq!(report.project.templates_found, 1);
        assert!(!report.project.lockfile_present);
        assert!(report.components.is_empty());
    }

    #[test]
    fn old_rustc_is_an_error() {
        let dir = temp_dir("old-rustc");
        write_project(
            &dir,
            "[package]\nname = \"p\"\nversion = \"0.1.0\"\n[workspace]\n\n[dependencies]\nlight = \"0.1\"\n",
            &[("app.html", "<div></div>")],
        );
        let mut probe = ok_probe();
        probe.rustc = Some(semver::Version::new(1, 60, 0));
        let report = doctor(&dir, &probe).expect("doctor");
        assert!(report.has_errors());
    }

    #[test]
    fn missing_templates_is_warning_not_error() {
        let dir = temp_dir("no-templates");
        write_project(
            &dir,
            "[package]\nname = \"p\"\nversion = \"0.1.0\"\n[workspace]\n\n[dependencies]\nlight = \"0.1\"\n",
            &[],
        );
        let report = doctor(&dir, &ok_probe()).expect("doctor");
        assert!(!report.has_errors());
        assert_eq!(report.project.templates_found, 0);
        assert!(
            report
                .issues
                .iter()
                .any(|i| i.severity == Severity::Warning && i.message.contains("шаблон"))
        );
    }

    #[test]
    fn incompatible_component_is_error() {
        let dir = temp_dir("incompatible");
        write_project(
            &dir,
            "[package]\nname = \"p\"\nversion = \"0.1.0\"\n[workspace]\n\n[dependencies]\nlight = \"0.1\"\nlight-fw-element-form = \"0.1\"\n",
            &[("app.html", "<checkbox></checkbox>")],
        );
        // Registry требует core ^0.6; проект на light 0.1 — несовместимо.
        let report = doctor(&dir, &ok_probe()).expect("doctor");
        assert!(
            report
                .components
                .iter()
                .any(|c| c.name == "light-fw-element-form")
        );
        assert!(report.has_errors(), "issues: {:?}", report.issues);
    }

    /// FIX8 (N2): проект с `light-fw-ui` обязан показать ровно 1 компонент —
    /// ранее `analyze_project().installed` (фильтр по `ComponentRegistry::builtin()`)
    /// не знал о `light-fw-ui`, поэтому doctor показывал «Компоненты (0)».
    #[test]
    fn project_with_light_fw_ui_shows_one_component() {
        let dir = temp_dir("with-fw-ui");
        write_project(
            &dir,
            "[package]\nname = \"p\"\nversion = \"0.1.0\"\n[workspace]\n\n\
             [dependencies]\nlight = \"^0.21\"\nlight-fw-ui = \"0.21.0-alpha.1\"\n",
            &[("app.html", "<dialog></dialog>")],
        );
        let report = doctor(&dir, &ok_probe()).expect("doctor");
        assert_eq!(
            report.components.len(),
            1,
            "ожидался ровно 1 компонент: {:?}",
            report.components
        );
        let ui = &report.components[0];
        assert_eq!(ui.name, "light-fw-ui");
        // Совместимо: registry core ^0.21, ядро ^0.21 → compatible=true.
        assert!(
            ui.compatible,
            "light-fw-ui должен быть совместим с core ^0.21"
        );
        assert!(
            !report.has_errors(),
            "совместимый проект не обязан иметь ошибки: {:?}",
            report.issues
        );
    }

    /// FIX8 (N2): проект БЕЗ `light-fw-ui` показывает 0 компонентов.
    /// Регресс-контроль: добавление записи в registry.toml не должно
    /// приписывать компоненты, которых нет в Cargo.toml.
    #[test]
    fn project_without_light_fw_ui_shows_zero_components() {
        let dir = temp_dir("without-fw-ui");
        write_project(
            &dir,
            "[package]\nname = \"p\"\nversion = \"0.1.0\"\n[workspace]\n\n\
             [dependencies]\nlight = \"^0.21\"\n",
            &[("app.html", "<div></div>")],
        );
        let report = doctor(&dir, &ok_probe()).expect("doctor");
        assert!(
            report.components.is_empty(),
            "не должно быть компонентов: {:?}",
            report.components
        );
    }

    #[test]
    fn missing_cargo_toml_is_error() {
        let dir = temp_dir("no-manifest");
        let report = doctor(&dir, &ok_probe()).expect("doctor");
        assert!(report.has_errors());
    }

    #[test]
    fn json_serializes_and_round_trips() {
        let dir = temp_dir("json");
        write_project(
            &dir,
            "[package]\nname = \"p\"\nversion = \"0.1.0\"\n[workspace]\n\n[dependencies]\nlight = \"0.1\"\n",
            &[("app.html", "<div></div>")],
        );
        let report = doctor(&dir, &ok_probe()).expect("doctor");
        let value = report.to_json();
        assert_eq!(value["project"]["templates_found"], 1);
        assert_eq!(value["environment"]["cli"], env!("CARGO_PKG_VERSION"));
        assert!(value["issues"].is_array());
    }
}
