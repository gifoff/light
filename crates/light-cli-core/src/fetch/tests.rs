#[cfg(test)]
#[allow(clippy::module_inception)]
mod tests {
    use crate::error::Error;
    #[cfg(feature = "network")]
    use crate::fetch::http::parse_search_response;
    use crate::fetch::{Catalog, DependencyEntry, RemoteRegistry, SourceKind};
    use crate::registry::ComponentRegistry;
    use std::fs;
    use std::path::{Path, PathBuf};

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("light-fw-cli-fetch-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn write_component(dir: &Path, name: &str) {
        fs::create_dir_all(dir.join("src")).expect("src");
        fs::write(dir.join("src/lib.rs"), "").expect("lib");
        fs::write(
            dir.join("Cargo.toml"),
            format!(
                "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
                 [package.metadata.light]\nkind = \"element\"\ndisplay-name = \"Test\"\ncore = \"^0.1\"\n\n\
                 [package.metadata.light.provides]\nelements = [\"checkbox\"]\n"
            ),
        )
        .expect("write component");
    }

    #[test]
    fn resolves_workspace_component() {
        let root = temp_dir("workspace");
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"app\", \"comp\"]\n\n\
             [workspace.dependencies]\nlight-fw-element-form = { path = \"comp\", version = \"0.1.0\" }\n",
        )
        .expect("root manifest");
        let app = root.join("app");
        fs::create_dir_all(&app).expect("app");
        fs::write(
            app.join("Cargo.toml"),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .expect("app manifest");
        write_component(&root.join("comp"), "light-fw-element-form");

        let catalog = Catalog::discover(&app, false);
        let resolved = catalog.resolve("light-fw-element-form").expect("resolve");
        assert_eq!(resolved.source, SourceKind::Workspace);
        assert_eq!(resolved.version, semver::Version::new(0, 1, 0));
        assert_eq!(resolved.provides.elements, vec!["checkbox"]);
        assert_eq!(
            catalog.dependency_entry(&resolved).expect("entry"),
            DependencyEntry::Workspace
        );
    }

    struct NoRemote;

    impl RemoteRegistry for NoRemote {}

    #[test]
    fn resolves_registry_fallback_without_network() {
        let dir = temp_dir("registry-fallback");
        fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[workspace]\n",
        )
        .expect("manifest");
        let catalog = Catalog::with_remote(&dir, false, Box::new(NoRemote));
        let resolved = catalog
            .resolve("light-fw-element-form")
            .expect("registry fallback");
        assert_eq!(resolved.source, SourceKind::CratesIo);
        // Ожидаемая версия — из bundled-реестра (CLI резолвит её офлайн).
        let expected = ComponentRegistry::builtin()
            .by_name("light-fw-element-form")
            .expect("bundled component")
            .latest_version
            .clone();
        assert_eq!(resolved.version, expected);
    }

    #[test]
    fn offline_without_local_is_error() {
        let dir = temp_dir("offline");
        fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[workspace]\n",
        )
        .expect("manifest");
        let catalog = Catalog::discover(&dir, true);
        let err = catalog
            .resolve("light-fw-element-form")
            .expect_err("offline error");
        assert!(matches!(err, Error::Offline(_)));
    }

    #[test]
    fn unknown_component_is_not_found() {
        let dir = temp_dir("unknown");
        fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[workspace]\n",
        )
        .expect("manifest");
        let catalog = Catalog::discover(&dir, false);
        let err = catalog
            .resolve("light-fw-datepicker")
            .expect_err("not found");
        assert!(matches!(err, Error::ComponentNotFound(_)));
    }

    #[test]
    fn non_component_crate_name_is_rejected() {
        let dir = temp_dir("not-component");
        fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[workspace]\n",
        )
        .expect("manifest");
        let catalog = Catalog::discover(&dir, false);
        let err = catalog.resolve("serde").expect_err("not a component");
        assert!(matches!(err, Error::NotComponent(_)));
    }

    struct MockRemote {
        name: &'static str,
        version: semver::Version,
    }

    impl RemoteRegistry for MockRemote {
        fn latest_version(
            &self,
            name: &str,
        ) -> std::result::Result<Option<semver::Version>, crate::error::Error> {
            if name == self.name {
                Ok(Some(self.version.clone()))
            } else {
                Ok(None)
            }
        }
    }

    #[cfg(feature = "network")]
    #[test]
    fn parses_crates_io_search_response() {
        let json = r#"{
            "crates": [
                {
                    "name": "light-fw-datepicker",
                    "max_stable_version": "0.3.0",
                    "max_version": "0.4.0-beta.1",
                    "description": "Date picker for Light"
                },
                {
                    "name": "light-fw-no-version",
                    "max_version": "1.0.0",
                    "description": null
                }
            ]
        }"#;
        let results = parse_search_response(
            json,
            &crate::registry_config::RegistryConfig::default_registry(),
        )
        .expect("parse");
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].name, "light-fw-datepicker");
        assert_eq!(results[0].version, "0.3.0");
        assert_eq!(
            results[0].url,
            "https://crates.io/crates/light-fw-datepicker"
        );
        assert_eq!(
            results[0].description.as_deref(),
            Some("Date picker for Light")
        );
        assert_eq!(results[1].version, "1.0.0");
        assert!(results[1].description.is_none());
    }

    #[test]
    fn resolves_external_component_from_remote() {
        let dir = temp_dir("external");
        fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[workspace]\n",
        )
        .expect("manifest");
        let catalog = Catalog::with_remote(
            &dir,
            false,
            Box::new(MockRemote {
                name: "light-fw-datepicker",
                version: semver::Version::new(1, 2, 3),
            }),
        );
        let resolved = catalog
            .resolve("light-fw-datepicker")
            .expect("external component");
        assert_eq!(resolved.source, SourceKind::CratesIo);
        assert_eq!(resolved.version, semver::Version::new(1, 2, 3));
        assert_eq!(
            catalog.dependency_entry(&resolved).expect("entry"),
            DependencyEntry::Version("1.2.3".to_string())
        );
    }
}
