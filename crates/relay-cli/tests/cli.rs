use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use assert_cmd::Command;
use predicates::prelude::*;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn relay() -> Command {
    Command::cargo_bin("relay").expect("bin relay")
}

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn copy_dir_all(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).expect("create dst");
    for entry in std::fs::read_dir(src).expect("read dir") {
        let entry = entry.expect("entry");
        let target = dst.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            copy_dir_all(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).expect("copy file");
        }
    }
}

/// Копирует фикстуру в уникальный временный каталог, чтобы тесты могли
/// мутировать проект, не трогая репозиторий.
fn temp_copy(name: &str) -> PathBuf {
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "relay-cli-it-{name}-{}-{unique}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    copy_dir_all(&fixture(name), &dir);
    dir
}

#[test]
fn check_empty_project_succeeds() {
    relay()
        .arg("check")
        .arg(fixture("empty-project"))
        .assert()
        .success()
        .stdout(predicate::str::contains("Шаблонов: 0"))
        .stdout(predicate::str::contains(
            "Отсутствующие компоненты:\n  (нет)",
        ));
}

#[test]
fn check_basic_project_succeeds() {
    relay()
        .arg("check")
        .arg(fixture("basic-project"))
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Отсутствующие компоненты:\n  (нет)",
        ));
}

#[test]
fn check_reports_missing_component() {
    relay()
        .arg("check")
        .arg(fixture("missing-form"))
        .assert()
        .success()
        .stdout(predicate::str::contains("relay-element-form"));
}

#[test]
fn check_strict_fails_on_missing_component() {
    relay()
        .arg("check")
        .arg("--strict")
        .arg(fixture("missing-form"))
        .assert()
        .failure()
        .code(1);
}

#[test]
fn check_complete_project_has_no_missing() {
    relay()
        .arg("check")
        .arg(fixture("complete-project"))
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Отсутствующие компоненты:\n  (нет)",
        ));
}

#[test]
fn check_fails_on_broken_template() {
    relay()
        .arg("check")
        .arg(fixture("broken-template"))
        .assert()
        .failure()
        .code(1)
        .stdout(predicate::str::contains("Проблемы:"));
}

#[test]
fn check_json_reports_missing() {
    let output = relay()
        .arg("check")
        .arg("--format")
        .arg("json")
        .arg(fixture("missing-form"))
        .output()
        .expect("run relay");
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid json");
    let missing = json["missing"].as_array().expect("missing array");
    assert!(
        missing.iter().any(|v| v == "relay-element-form"),
        "ожидался relay-element-form в missing: {json}"
    );
}

#[test]
fn doctor_reports_healthy_project() {
    relay()
        .arg("doctor")
        .arg("--offline")
        .arg(fixture("basic-project"))
        .assert()
        .success()
        .stdout(predicate::str::contains("Relay doctor"))
        .stdout(predicate::str::contains("Компоненты"));
}

#[test]
fn doctor_json_has_sections() {
    let output = relay()
        .arg("doctor")
        .arg("--offline")
        .arg("--format")
        .arg("json")
        .arg(fixture("complete-project"))
        .output()
        .expect("run relay");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid json");
    assert!(json["environment"].is_object());
    assert!(json["project"].is_object());
    assert!(json["components"].is_array());
    assert!(json["issues"].is_array());
}

#[test]
fn doctor_fails_without_manifest() {
    let dir = std::env::temp_dir().join("relay-cli-doctor-missing");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create dir");
    relay()
        .arg("doctor")
        .arg("--offline")
        .arg(&dir)
        .assert()
        .failure()
        .code(1);
}

#[test]
fn suggest_lists_recommendations_and_succeeds() {
    relay()
        .arg("suggest")
        .arg(fixture("missing-form"))
        .assert()
        .success()
        .stdout(predicate::str::contains("добавить relay-element-form"));
}

#[test]
fn suggest_complete_project_reports_nothing_to_add() {
    relay()
        .arg("suggest")
        .arg(fixture("complete-project"))
        .assert()
        .success()
        .stdout(predicate::str::contains("уже подключены"));
}

#[test]
fn suggest_always_exits_zero_even_with_broken_template() {
    relay()
        .arg("suggest")
        .arg(fixture("broken-template"))
        .assert()
        .success();
}

#[test]
fn add_dry_run_plans_workspace_component() {
    let app = temp_copy("component-workspace").join("app");
    relay()
        .arg("add")
        .arg("relay-element-form")
        .arg(&app)
        .arg("--dry-run")
        .arg("--offline")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "План добавления relay-element-form",
        ))
        .stdout(predicate::str::contains("dry-run"));
    assert!(!app.join("relay.lock").exists(), "dry-run не создаёт lock");
}

#[test]
fn add_unknown_component_fails_offline() {
    let app = temp_copy("component-workspace").join("app");
    relay()
        .arg("add")
        .arg("relay-widget-datepicker")
        .arg(&app)
        .arg("--offline")
        .assert()
        .failure()
        .code(1);
}

#[test]
fn add_already_installed_is_idempotent() {
    let project = temp_copy("complete-project");
    relay()
        .arg("add")
        .arg("relay-element-form")
        .arg(&project)
        .arg("--yes")
        .arg("--offline")
        .assert()
        .success()
        .stdout(predicate::str::contains("уже установлен"));
}

#[test]
fn add_then_remove_roundtrip() {
    let root = temp_copy("component-workspace");
    let app = root.join("app");

    relay()
        .arg("add")
        .arg("relay-element-form")
        .arg(&app)
        .arg("--yes")
        .arg("--no-verify")
        .arg("--offline")
        .assert()
        .success();

    let text = std::fs::read_to_string(app.join("Cargo.toml")).expect("read app manifest");
    assert!(
        text.contains("relay-element-form = { workspace = true, optional = true }"),
        "{text}"
    );
    assert!(
        text.contains("element-form = [\"dep:relay-element-form\"]"),
        "{text}"
    );
    assert!(app.join("relay.lock").exists());

    relay()
        .arg("remove")
        .arg("relay-element-form")
        .arg(&app)
        .arg("--yes")
        .assert()
        .success();

    let text = std::fs::read_to_string(app.join("Cargo.toml")).expect("read app manifest");
    assert!(!text.contains("relay-element-form"), "{text}");
}

#[test]
fn lock_creates_then_reports_actual() {
    let project = temp_copy("complete-project");
    relay()
        .arg("lock")
        .arg(&project)
        .assert()
        .success()
        .stdout(predicate::str::contains("relay.lock обновлён"));
    assert!(project.join("relay.lock").exists());

    relay()
        .arg("lock")
        .arg(&project)
        .assert()
        .success()
        .stdout(predicate::str::contains("актуален"));
}

#[test]
fn lock_dry_run_does_not_write() {
    let project = temp_copy("complete-project");
    relay()
        .arg("lock")
        .arg(&project)
        .arg("--dry-run")
        .assert()
        .success()
        .stdout(predicate::str::contains("dry-run"));
    assert!(!project.join("relay.lock").exists());
}

#[test]
fn update_dry_run_reports_change() {
    let project = temp_copy("outdated-project");
    relay()
        .arg("update")
        .arg(&project)
        .arg("--dry-run")
        .assert()
        .success()
        .stdout(predicate::str::contains("→ 0.2.0"));
    let text = std::fs::read_to_string(project.join("Cargo.toml")).expect("read");
    assert!(
        text.contains("relay-element-form = \"0.0.1\""),
        "не изменено: {text}"
    );
}

#[test]
fn update_applies_new_version() {
    let project = temp_copy("outdated-project");
    relay()
        .arg("update")
        .arg(&project)
        .arg("--yes")
        .arg("--no-verify")
        .assert()
        .success()
        .stdout(predicate::str::contains("обновлено"));
    let text = std::fs::read_to_string(project.join("Cargo.toml")).expect("read");
    assert!(text.contains("relay-element-form = \"0.2.0\""), "{text}");
}

#[test]
fn vendor_copies_component_sources() {
    let app = temp_copy("installed-workspace").join("app");
    relay()
        .arg("vendor")
        .arg("--offline")
        .current_dir(&app)
        .assert()
        .success()
        .stdout(predicate::str::contains("relay-element-form"));
    assert!(
        app.join("vendor/relay-element-form/Cargo.toml").is_file(),
        "исходники скопированы"
    );
}

#[test]
fn vendor_path_flag_sets_output_dir() {
    let app = temp_copy("installed-workspace").join("app");
    relay()
        .arg("vendor")
        .arg("--path")
        .arg("vendor-test")
        .arg("--offline")
        .current_dir(&app)
        .assert()
        .success();
    assert!(
        app.join("vendor-test/relay-element-form/Cargo.toml")
            .is_file(),
        "--path задаёт каталог вывода"
    );
    assert!(!app.join("vendor").exists());
}

#[test]
fn vendor_positional_path_sets_output_dir() {
    let app = temp_copy("installed-workspace").join("app");
    relay()
        .arg("vendor")
        .arg("vendor-positional")
        .arg("--offline")
        .current_dir(&app)
        .assert()
        .success();
    assert!(
        app.join("vendor-positional/relay-element-form/Cargo.toml")
            .is_file(),
        "позиционный путь задаёт каталог вывода"
    );
}

#[test]
fn vendor_dry_run_does_not_copy() {
    let app = temp_copy("installed-workspace").join("app");
    relay()
        .arg("vendor")
        .arg("--dry-run")
        .current_dir(&app)
        .assert()
        .success()
        .stdout(predicate::str::contains("dry-run"));
    assert!(!app.join("vendor").exists());
}

#[test]
fn add_workspace_component_joins_aggregating_feature() {
    let app = temp_copy("aggregated-workspace").join("app");
    relay()
        .arg("add")
        .arg("relay-behavior-validation")
        .arg(&app)
        .arg("--yes")
        .arg("--no-verify")
        .arg("--offline")
        .assert()
        .success()
        .stdout(predicate::str::contains("источник:    workspace"))
        .stdout(predicate::str::contains("фича:        full"));

    let text = std::fs::read_to_string(app.join("Cargo.toml")).expect("read app manifest");
    assert!(
        text.contains("relay-behavior-validation = { workspace = true, optional = true }"),
        "зависимость из workspace: {text}"
    );
    assert!(
        text.contains("dep:relay-behavior-validation"),
        "компонент в агрегирующей фиче: {text}"
    );
    assert!(app.join("relay.lock").exists(), "lock создан");

    let backup_dir = app.join("target").join(".relay").join("backup");
    let has_backup = std::fs::read_dir(&backup_dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .any(|e| e.file_name().to_string_lossy().starts_with("Cargo.toml."))
        })
        .unwrap_or(false);
    assert!(has_backup, "снимок Cargo.toml в target/.relay/backup");
}

#[test]
fn add_from_project_dir_resolves_workspace_source() {
    let app = temp_copy("component-workspace").join("app");
    // Путь по умолчанию "." — раньше относительный путь ломал поиск workspace.
    relay()
        .arg("add")
        .arg("relay-element-form")
        .arg("--dry-run")
        .arg("--offline")
        .current_dir(&app)
        .assert()
        .success()
        .stdout(predicate::str::contains("источник:    workspace"));
}

#[test]
fn doctor_reports_core_version_from_workspace() {
    let app = temp_copy("core-workspace").join("app");
    relay()
        .arg("doctor")
        .arg("--offline")
        .arg(&app)
        .assert()
        .success()
        .stdout(predicate::str::contains("relay-core 0.2.0"));
}

#[test]
fn remove_dry_run_reports_installed_version() {
    let app = temp_copy("core-workspace").join("app");
    relay()
        .arg("remove")
        .arg("relay-element-form")
        .arg(&app)
        .arg("--dry-run")
        .assert()
        .success()
        .stdout(predicate::str::contains("версия: 0.1.0"));
}

#[test]
fn remove_updates_lockfile() {
    let app = temp_copy("core-workspace").join("app");
    relay().arg("lock").arg(&app).assert().success();
    let before = std::fs::read_to_string(app.join("relay.lock")).expect("read lock");
    assert!(
        before.contains("relay-element-form"),
        "до удаления: {before}"
    );

    relay()
        .arg("remove")
        .arg("relay-element-form")
        .arg(&app)
        .arg("--yes")
        .assert()
        .success();

    let after = std::fs::read_to_string(app.join("relay.lock")).expect("read lock");
    assert!(
        !after.contains("relay-element-form"),
        "компонент убран из lock: {after}"
    );
    assert!(after.contains("core = \"0.2.0\""), "ядро в lock: {after}");
}

#[test]
fn publish_dry_run_orders_crates_and_skips_unpublished() {
    let ws = temp_copy("publish-workspace");
    let output = relay()
        .arg("publish")
        .arg("--dry-run")
        .arg("--skip-gates")
        .current_dir(&ws)
        .output()
        .expect("run relay publish");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "stdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("skipped 1 crates (publish = false)"),
        "{stdout}"
    );
    let a = stdout.find("pub-a").expect("pub-a in output");
    let b = stdout.find("pub-b").expect("pub-b in output");
    assert!(a < b, "pub-a должна идти раньше pub-b:\n{stdout}");
    assert!(stdout.contains("packaged: 2/2"), "{stdout}");
}

#[test]
fn publish_dry_run_single_crate() {
    let ws = temp_copy("publish-workspace");
    relay()
        .arg("publish")
        .arg("pub-a")
        .arg("--dry-run")
        .arg("--skip-gates")
        .current_dir(&ws)
        .assert()
        .success()
        .stdout(predicate::str::contains("packaged: 1/1"))
        .stdout(predicate::str::contains("pub-a"));
}

#[test]
fn publish_only_flag_selects_single_crate() {
    let ws = temp_copy("publish-workspace");
    relay()
        .arg("publish")
        .arg("--only")
        .arg("pub-b")
        .arg("--dry-run")
        .arg("--skip-gates")
        .current_dir(&ws)
        .assert()
        .success()
        .stdout(predicate::str::contains("packaged: 1/1"));
}

#[test]
fn publish_requires_yes_in_non_interactive_mode() {
    let ws = temp_copy("publish-workspace");
    relay()
        .arg("publish")
        .arg("--skip-gates")
        .current_dir(&ws)
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains(
            "refusing to publish in non-interactive mode without --yes",
        ));
}

#[test]
fn publish_rejects_publishable_depending_on_unpublished() {
    let ws = temp_copy("publish-broken");
    relay()
        .arg("publish")
        .arg("--dry-run")
        .arg("--skip-gates")
        .current_dir(&ws)
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("publish = false"));
}
