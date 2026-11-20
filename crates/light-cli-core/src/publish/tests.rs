#[cfg(test)]
#[allow(clippy::module_inception)]
mod tests {
    use crate::process::{CommandOutput, CommandRunner};
    use std::collections::{BTreeMap, BTreeSet, VecDeque};
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;
    use std::time::Duration;

    use crate::publish::base::*;
    use crate::publish::gates::*;
    use crate::publish::retry::*;

    fn package(name: &str, publishable: bool, deps: &[(&str, DepKind, &str)]) -> PackageInfo {
        PackageInfo {
            name: name.to_string(),
            version: semver::Version::new(0, 2, 0),
            manifest_path: PathBuf::from(format!("{name}/Cargo.toml")),
            publishable,
            description: None,
            license: None,
            repository: None,
            readme: None,
            deps: deps
                .iter()
                .map(|(dep_name, kind, req)| InternalDep {
                    name: (*dep_name).to_string(),
                    req_raw: (*req).to_string(),
                    req: semver::VersionReq::parse(req).expect("req"),
                    kind: *kind,
                })
                .collect(),
        }
    }

    fn workspace(packages: Vec<PackageInfo>) -> Workspace {
        let mut by_name = BTreeMap::new();
        for (index, package) in packages.iter().enumerate() {
            by_name.insert(package.name.clone(), index);
        }
        Workspace {
            root: PathBuf::from("."),
            packages,
            by_name,
        }
    }

    fn ok_output() -> CommandOutput {
        CommandOutput {
            status: 0,
            stdout: String::new(),
            stderr: String::new(),
        }
    }

    fn fail_output(stderr: &str) -> CommandOutput {
        CommandOutput {
            status: 101,
            stdout: String::new(),
            stderr: stderr.to_string(),
        }
    }

    struct ScriptedRunner {
        outputs: Mutex<VecDeque<CommandOutput>>,
        calls: Mutex<Vec<Vec<String>>>,
    }

    impl ScriptedRunner {
        fn new(outputs: Vec<CommandOutput>) -> Self {
            Self {
                outputs: Mutex::new(outputs.into()),
                calls: Mutex::new(Vec::new()),
            }
        }
        fn call_count(&self) -> usize {
            self.calls.lock().expect("calls").len()
        }
    }

    impl CommandRunner for ScriptedRunner {
        fn run(
            &self,
            _program: &str,
            args: &[String],
            _cwd: Option<&Path>,
        ) -> std::result::Result<CommandOutput, crate::error::Error> {
            self.calls.lock().expect("calls").push(args.to_vec());
            Ok(self
                .outputs
                .lock()
                .expect("outputs")
                .pop_front()
                .expect("scripted output"))
        }
    }

    struct RecordingSleeper {
        sleeps: Mutex<Vec<Duration>>,
    }

    impl RecordingSleeper {
        fn new() -> Self {
            Self {
                sleeps: Mutex::new(Vec::new()),
            }
        }
        fn sleeps(&self) -> Vec<Duration> {
            self.sleeps.lock().expect("sleeps").clone()
        }
    }

    impl Sleeper for RecordingSleeper {
        fn sleep(&self, duration: Duration) {
            self.sleeps.lock().expect("sleeps").push(duration);
        }
    }

    struct MockProbe {
        outcome: ApiOutcome,
    }

    impl RegistryProbe for MockProbe {
        fn versions(&self, _name: &str) -> ApiOutcome {
            self.outcome.clone()
        }
    }

    #[test]
    fn topological_order_puts_dependencies_first() {
        let ws = workspace(vec![
            package(
                "light-fw-cli",
                true,
                &[("light-fw-cli-core", DepKind::Normal, "^0.2.0")],
            ),
            package("light-fw-core", true, &[]),
            package(
                "light-fw-cli-core",
                true,
                &[("light-fw-core", DepKind::Normal, "^0.2.0")],
            ),
        ]);
        let plan = plan(&ws, None).expect("plan");
        assert_eq!(
            plan.order,
            vec!["light-fw-core", "light-fw-cli-core", "light-fw-cli"]
        );
    }

    #[test]
    fn dev_dependencies_do_not_create_edges() {
        let ws = workspace(vec![
            package("a", true, &[]),
            package("b", true, &[("a", DepKind::Dev, "^0.2.0")]),
        ]);
        let plan = plan(&ws, None).expect("plan");
        assert_eq!(plan.order, vec!["a", "b"]);
    }

    #[test]
    fn publish_false_is_skipped() {
        let ws = workspace(vec![
            package("light-fw-core", true, &[]),
            package(
                "light-demo",
                false,
                &[("light-fw-core", DepKind::Normal, "^0.2.0")],
            ),
        ]);
        let plan = plan(&ws, None).expect("plan");
        assert_eq!(plan.order, vec!["light-fw-core"]);
        assert_eq!(plan.skipped, vec!["light-demo"]);
    }

    #[test]
    fn publishable_depending_on_unpublished_is_error() {
        let ws = workspace(vec![
            package("app", true, &[("dep", DepKind::Normal, "^0.2.0")]),
            package("dep", false, &[]),
        ]);
        let error = plan(&ws, None).expect_err("должна быть ошибка");
        assert!(
            error.to_string().contains("publish = false"),
            "неожиданная ошибка: {error}"
        );
    }

    #[test]
    fn dependency_cycle_is_error() {
        let ws = workspace(vec![
            package("a", true, &[("b", DepKind::Normal, "^0.2.0")]),
            package("b", true, &[("a", DepKind::Normal, "^0.2.0")]),
        ]);
        let error = plan(&ws, None).expect_err("цикл");
        assert!(error.to_string().contains("цикл"), "{error}");
    }

    #[test]
    fn single_crate_selects_one() {
        let ws = workspace(vec![
            package("a", true, &[]),
            package("b", true, &[("a", DepKind::Normal, "^0.2.0")]),
        ]);
        let plan = plan(&ws, Some("b")).expect("plan");
        assert_eq!(plan.order, vec!["b"]);
    }

    #[test]
    fn unknown_single_crate_is_error() {
        let ws = workspace(vec![package("a", true, &[])]);
        let error = plan(&ws, Some("missing")).expect_err("не найден");
        assert!(error.to_string().contains("не является членом"), "{error}");
    }

    #[test]
    fn single_unpublished_crate_is_error() {
        let ws = workspace(vec![package("demo", false, &[])]);
        let error = plan(&ws, Some("demo")).expect_err("publish = false");
        assert!(error.to_string().contains("publish = false"), "{error}");
    }

    #[test]
    fn version_consistency_detects_mismatch() {
        let ws = workspace(vec![
            package("core", true, &[]),
            package("comp", true, &[("core", DepKind::Normal, "^0.1.0")]),
        ]);
        let plan = plan(&ws, None).expect("plan");
        let error = check_version_consistency(&ws, &plan).expect_err("mismatch");
        assert!(error.to_string().contains("должны совпадать"), "{error}");
    }

    #[test]
    fn version_consistency_accepts_matching_dependency() {
        let ws = workspace(vec![
            package("core", true, &[]),
            package("comp", true, &[("core", DepKind::Normal, "^0.2.0")]),
        ]);
        let plan = plan(&ws, None).expect("plan");
        assert!(check_version_consistency(&ws, &plan).is_ok());
    }

    #[test]
    fn metadata_warnings_list_missing_fields() {
        let ws = workspace(vec![package("bare", true, &[]), {
            let mut p = package("full", true, &[]);
            p.description = Some("d".into());
            p.license = Some("MIT".into());
            p.repository = Some("https://example.com".into());
            p.readme = Some("README.md".into());
            p
        }]);
        let plan = plan(&ws, None).expect("plan");
        let warnings = metadata_warnings(&ws, &plan);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].starts_with("bare: missing "), "{warnings:?}");
        assert!(warnings[0].contains("readme"), "{warnings:?}");
    }

    #[test]
    fn classify_detects_index_staleness() {
        assert_eq!(
            classify_publish_failure("error: failed to select a version for the requirement"),
            FailureKind::IndexStale
        );
        assert_eq!(
            classify_publish_failure("no matching package named `light-fw-core` found"),
            FailureKind::IndexStale
        );
        assert_eq!(
            classify_publish_failure("error: api token not provided"),
            FailureKind::Other
        );
    }

    #[test]
    fn classify_detects_rate_limit() {
        assert_eq!(
            classify_publish_failure(
                "the remote server responded with an error (status 429 Too Many Requests)"
            ),
            FailureKind::RateLimited
        );
        assert_eq!(
            classify_publish_failure(
                "You have hit the limit for the number of too many new crates"
            ),
            FailureKind::RateLimited
        );
    }

    #[test]
    fn classify_detects_already_published() {
        assert_eq!(
            classify_publish_failure(
                "error: crate light-fw-html@0.6.1 already exists on crates.io index"
            ),
            FailureKind::AlreadyPublished
        );
        assert_eq!(
            classify_publish_failure("error: crate version `0.6.1` is already uploaded"),
            FailureKind::AlreadyPublished
        );
    }

    #[test]
    fn rate_limit_reset_extracts_timestamp() {
        let stderr = "error: the remote server responded with an error (status 429 Too Many \
                      Requests): too many new crates. Please try again after \
                      2026-10-20T12:34:56Z.";
        assert_eq!(
            rate_limit_reset(stderr).as_deref(),
            Some("2026-10-20T12:34:56Z")
        );
    }

    #[test]
    fn rate_limit_reset_is_optional() {
        assert_eq!(rate_limit_reset("status 429 Too Many Requests"), None);
        assert_eq!(rate_limit_reset("api token not provided"), None);
    }

    #[test]
    fn publish_does_not_retry_rate_limit() {
        let runner = ScriptedRunner::new(vec![fail_output(
            "error: status 429 Too Many Requests: too many new crates",
        )]);
        let sleeper = RecordingSleeper::new();
        let attempt = publish_with_retries(
            &runner,
            &sleeper,
            Path::new("."),
            "light-fw",
            &PublishInvocation::default(),
            &RetryConfig::default(),
        )
        .expect("publish");
        assert!(!attempt.success);
        assert_eq!(attempt.retries, 0);
        assert!(sleeper.sleeps().is_empty());
        assert_eq!(runner.call_count(), 1);
        assert_eq!(
            classify_publish_failure(&attempt.stderr),
            FailureKind::RateLimited
        );
    }

    #[test]
    fn publish_treats_already_published_as_skip_without_retry() {
        let runner = ScriptedRunner::new(vec![fail_output(
            "error: crate light-fw-html@0.6.1 already exists on crates.io index",
        )]);
        let sleeper = RecordingSleeper::new();
        let attempt = publish_with_retries(
            &runner,
            &sleeper,
            Path::new("."),
            "light-fw-html",
            &PublishInvocation::default(),
            &RetryConfig::default(),
        )
        .expect("publish");
        assert!(!attempt.success);
        assert!(attempt.already_published);
        assert_eq!(attempt.retries, 0);
        assert!(sleeper.sleeps().is_empty());
        assert_eq!(runner.call_count(), 1);
    }

    #[test]
    fn publish_detects_already_published_in_stdout() {
        let runner = ScriptedRunner::new(vec![CommandOutput {
            status: 101,
            stdout: "error: crate version `0.6.1` is already uploaded".to_string(),
            stderr: String::new(),
        }]);
        let sleeper = RecordingSleeper::new();
        let attempt = publish_with_retries(
            &runner,
            &sleeper,
            Path::new("."),
            "light-fw-html",
            &PublishInvocation::default(),
            &RetryConfig::default(),
        )
        .expect("publish");
        assert!(!attempt.success);
        assert!(attempt.already_published);
        assert!(sleeper.sleeps().is_empty());
    }

    #[test]
    fn retry_backoff_is_exponential() {
        let config = RetryConfig {
            max_retries: 4,
            base: Duration::from_secs(5),
            factor: 2,
        };
        assert_eq!(config.delay(0), Duration::from_secs(5));
        assert_eq!(config.delay(1), Duration::from_secs(10));
        assert_eq!(config.delay(2), Duration::from_secs(20));
        assert_eq!(config.delay(3), Duration::from_secs(40));
    }

    #[test]
    fn publish_retries_index_staleness_then_succeeds() {
        let runner = ScriptedRunner::new(vec![
            fail_output("no matching package named `light-fw-core` found"),
            ok_output(),
        ]);
        let sleeper = RecordingSleeper::new();
        let config = RetryConfig {
            max_retries: 3,
            base: Duration::from_secs(5),
            factor: 2,
        };
        let attempt = publish_with_retries(
            &runner,
            &sleeper,
            Path::new("."),
            "light-fw",
            &PublishInvocation::default(),
            &config,
        )
        .expect("publish");
        assert!(attempt.success);
        assert_eq!(attempt.retries, 1);
        assert_eq!(sleeper.sleeps(), vec![Duration::from_secs(5)]);
        assert_eq!(runner.call_count(), 2);
    }

    #[test]
    fn publish_stops_immediately_on_other_error() {
        let runner = ScriptedRunner::new(vec![fail_output("error: api token not provided")]);
        let sleeper = RecordingSleeper::new();
        let attempt = publish_with_retries(
            &runner,
            &sleeper,
            Path::new("."),
            "light-fw",
            &PublishInvocation::default(),
            &RetryConfig::default(),
        )
        .expect("publish");
        assert!(!attempt.success);
        assert_eq!(attempt.retries, 0);
        assert!(sleeper.sleeps().is_empty());
        assert_eq!(runner.call_count(), 1);
    }

    #[test]
    fn publish_exhausts_retries() {
        let runner = ScriptedRunner::new(vec![
            fail_output("failed to select a version"),
            fail_output("failed to select a version"),
        ]);
        let sleeper = RecordingSleeper::new();
        let config = RetryConfig {
            max_retries: 1,
            base: Duration::from_secs(5),
            factor: 2,
        };
        let attempt = publish_with_retries(
            &runner,
            &sleeper,
            Path::new("."),
            "light-fw",
            &PublishInvocation::default(),
            &config,
        )
        .expect("publish");
        assert!(!attempt.success);
        assert_eq!(attempt.retries, 1);
        assert_eq!(sleeper.sleeps(), vec![Duration::from_secs(5)]);
        assert_eq!(runner.call_count(), 2);
    }

    #[test]
    fn publish_args_shape() {
        let invocation = PublishInvocation {
            verify: false,
            allow_dirty: true,
            token: Some("secret".into()),
        };
        assert_eq!(
            publish_args("light-fw", &invocation),
            vec![
                "publish",
                "-p",
                "light-fw",
                "--no-verify",
                "--allow-dirty",
                "--token",
                "secret"
            ]
        );
        let verifying = PublishInvocation {
            verify: true,
            ..PublishInvocation::default()
        };
        assert_eq!(
            publish_args("light-fw", &verifying),
            vec!["publish", "-p", "light-fw"]
        );
    }

    #[test]
    fn package_list_args_shape() {
        assert_eq!(
            package_list_args("light-fw-core"),
            vec!["package", "--list", "-p", "light-fw-core", "--allow-dirty"]
        );
    }

    #[test]
    fn gate_commands_apply_exclude_to_build_test_clippy() {
        let commands = gate_commands(Some("light-fw-cli"));
        let names: Vec<&str> = commands.iter().map(|(name, _)| *name).collect();
        assert_eq!(names, vec!["build", "test", "clippy", "fmt"]);
        for (name, args) in &commands {
            let has_exclude = args.iter().any(|arg| arg == "--exclude");
            if *name == "fmt" {
                assert!(!has_exclude, "cargo fmt не поддерживает --exclude");
            } else {
                assert!(has_exclude, "{name} должен исключать запущенный крейт");
            }
            if *name == "clippy" {
                assert_eq!(args.last().map(String::as_str), Some("warnings"));
            }
        }
    }

    #[test]
    fn gate_commands_without_exclude_are_plain() {
        let commands = gate_commands(None);
        assert!(
            commands
                .iter()
                .all(|(_, args)| !args.iter().any(|arg| arg == "--exclude"))
        );
    }

    #[test]
    fn already_published_when_both_sources_agree() {
        let probe = MockProbe {
            outcome: ApiOutcome::Versions(vec![semver::Version::new(0, 2, 0)]),
        };
        let mut tags = BTreeSet::new();
        tags.insert("v0.2.0".to_string());
        let check = check_already_published(
            &probe,
            Some(&tags),
            "light-fw",
            &semver::Version::new(0, 2, 0),
        );
        assert!(check.published);
        assert!(check.warnings.is_empty());
    }

    #[test]
    fn already_published_disagreement_warns_and_continues() {
        let probe = MockProbe {
            outcome: ApiOutcome::Versions(vec![semver::Version::new(0, 2, 0)]),
        };
        let tags = BTreeSet::new();
        let check = check_already_published(
            &probe,
            Some(&tags),
            "light-fw",
            &semver::Version::new(0, 2, 0),
        );
        assert!(!check.published);
        assert_eq!(check.warnings.len(), 1);
        assert!(
            check.warnings[0].contains("расходятся"),
            "{:?}",
            check.warnings
        );
    }

    #[test]
    fn api_unreachable_falls_back_to_git_tags() {
        let probe = MockProbe {
            outcome: ApiOutcome::Unavailable,
        };
        let mut tags = BTreeSet::new();
        tags.insert("v0.2.0".to_string());
        let check = check_already_published(
            &probe,
            Some(&tags),
            "light-fw",
            &semver::Version::new(0, 2, 0),
        );
        assert!(check.published);
        assert!(
            check.warnings[0].contains("falling back to git tags"),
            "{:?}",
            check.warnings
        );
    }

    #[test]
    fn not_found_on_registry_is_not_published() {
        let probe = MockProbe {
            outcome: ApiOutcome::NotFound,
        };
        let tags = BTreeSet::new();
        let check = check_already_published(
            &probe,
            Some(&tags),
            "light-fw",
            &semver::Version::new(0, 2, 0),
        );
        assert!(!check.published);
        assert!(check.warnings.is_empty());
    }
}
