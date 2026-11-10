use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

/// Версия световой зависимости, которую генерирует `light init`.
///
/// Идентична константе [`light_fw_cli_core::init::LIGHT_DEPENDENCY_VERSION`]
/// (обе — `env!("CARGO_PKG_VERSION")` на уровне воркспейса). Используется,
/// чтобы тесты не падали при bump версии крейтов.
const LIGHT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Minor-диапазон (`major.minor`) — то, что реально пишется в `Cargo.toml`
/// нового проекта. См. [`light_fw_cli_core::init::minor_version`].
fn light_minor_version() -> &'static str {
    static ONCE: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    ONCE.get_or_init(|| {
        LIGHT_VERSION
            .rsplit_once('.')
            .map(|(prefix, _)| prefix.to_string())
            .unwrap_or_else(|| LIGHT_VERSION.to_string())
    })
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn light() -> Command {
    Command::new(env!("CARGO_BIN_EXE_light"))
}

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

/// Копирует фикстуру во временный каталог, чтобы тесты могли мутировать
/// проект, не трогая репозиторий.
fn temp_copy(name: &str) -> TempDir {
    let dir = tempfile::Builder::new()
        .prefix(&format!("light-fw-cli-it-{name}-"))
        .tempdir()
        .expect("create temp dir");
    copy_dir_all(&fixture(name), dir.path());
    dir
}

/// Создаёт пустой временный каталог (для `light init`).
fn scratch_dir(name: &str) -> TempDir {
    tempfile::Builder::new()
        .prefix(&format!("light-fw-cli-scratch-{name}-"))
        .tempdir()
        .expect("create scratch dir")
}

#[test]
fn check_empty_project_succeeds() {
    light()
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
    light()
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
    light()
        .arg("check")
        .arg(fixture("missing-form"))
        .assert()
        .success()
        .stdout(predicate::str::contains("light-fw-element-form"));
}

#[test]
fn check_strict_fails_on_missing_component() {
    light()
        .arg("check")
        .arg("--strict")
        .arg(fixture("missing-form"))
        .assert()
        .failure()
        .code(1);
}

#[test]
fn check_complete_project_has_no_missing() {
    light()
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
    light()
        .arg("check")
        .arg(fixture("broken-template"))
        .assert()
        .failure()
        .code(1)
        .stdout(predicate::str::contains("Проблемы:"));
}

#[test]
fn check_json_reports_missing() {
    let output = light()
        .arg("check")
        .arg("--format")
        .arg("json")
        .arg(fixture("missing-form"))
        .output()
        .expect("run light");
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid json");
    let missing = json["missing"].as_array().expect("missing array");
    assert!(
        missing.iter().any(|v| v == "light-fw-element-form"),
        "ожидался light-fw-element-form в missing: {json}"
    );
}

#[test]
fn doctor_reports_healthy_project() {
    light()
        .arg("doctor")
        .arg("--offline")
        .arg(fixture("basic-project"))
        .assert()
        .success()
        .stdout(predicate::str::contains("Light doctor"))
        .stdout(predicate::str::contains("Компоненты"));
}

#[test]
fn doctor_json_has_sections() {
    let output = light()
        .arg("doctor")
        .arg("--offline")
        .arg("--format")
        .arg("json")
        .arg(fixture("complete-project"))
        .output()
        .expect("run light");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid json");
    assert!(json["environment"].is_object());
    assert!(json["project"].is_object());
    assert!(json["components"].is_array());
    assert!(json["issues"].is_array());
}

#[test]
fn doctor_fails_without_manifest() {
    let dir = std::env::temp_dir().join("light-fw-cli-doctor-missing");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create dir");
    light()
        .arg("doctor")
        .arg("--offline")
        .arg(&dir)
        .assert()
        .failure()
        .code(1);
}

#[test]
fn suggest_lists_recommendations_and_succeeds() {
    light()
        .arg("suggest")
        .arg(fixture("missing-form"))
        .assert()
        .success()
        .stdout(predicate::str::contains("добавить light-fw-element-form"));
}

#[test]
fn suggest_complete_project_reports_nothing_to_add() {
    light()
        .arg("suggest")
        .arg(fixture("complete-project"))
        .assert()
        .success()
        .stdout(predicate::str::contains("уже подключены"));
}

#[test]
fn suggest_always_exits_zero_even_with_broken_template() {
    light()
        .arg("suggest")
        .arg(fixture("broken-template"))
        .assert()
        .success();
}

#[test]
fn add_dry_run_plans_workspace_component() {
    let workspace = temp_copy("component-workspace");
    let app = workspace.path().join("app");
    light()
        .arg("add")
        .arg("light-fw-element-form")
        .arg(&app)
        .arg("--dry-run")
        .arg("--offline")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "План добавления light-fw-element-form",
        ))
        .stdout(predicate::str::contains("dry-run"));
    assert!(!app.join("light.lock").exists(), "dry-run не создаёт lock");
}

#[test]
fn add_unknown_component_fails_offline() {
    let workspace = temp_copy("component-workspace");
    let app = workspace.path().join("app");
    light()
        .arg("add")
        .arg("light-widget-datepicker")
        .arg(&app)
        .arg("--offline")
        .assert()
        .failure()
        .code(1);
}

#[test]
fn add_already_installed_is_idempotent() {
    let workspace = temp_copy("complete-project");
    let project = workspace.path().to_path_buf();
    light()
        .arg("add")
        .arg("light-fw-element-form")
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
    let app = root.path().join("app");

    light()
        .arg("add")
        .arg("light-fw-element-form")
        .arg(&app)
        .arg("--yes")
        .arg("--no-verify")
        .arg("--offline")
        .assert()
        .success();

    let text = std::fs::read_to_string(app.join("Cargo.toml")).expect("read app manifest");
    assert!(
        text.contains("light-fw-element-form = { workspace = true, optional = true }"),
        "{text}"
    );
    assert!(
        text.contains("element-form = [\"dep:light-fw-element-form\"]"),
        "{text}"
    );
    assert!(app.join("light.lock").exists());

    light()
        .arg("remove")
        .arg("light-fw-element-form")
        .arg(&app)
        .arg("--yes")
        .assert()
        .success();

    let text = std::fs::read_to_string(app.join("Cargo.toml")).expect("read app manifest");
    assert!(!text.contains("light-fw-element-form"), "{text}");
}

#[test]
fn lock_creates_then_reports_actual() {
    let workspace = temp_copy("complete-project");
    let project = workspace.path().to_path_buf();
    light()
        .arg("lock")
        .arg(&project)
        .assert()
        .success()
        .stdout(predicate::str::contains("light.lock обновлён"));
    assert!(project.join("light.lock").exists());

    light()
        .arg("lock")
        .arg(&project)
        .assert()
        .success()
        .stdout(predicate::str::contains("актуален"));
}

#[test]
fn lock_dry_run_does_not_write() {
    let workspace = temp_copy("complete-project");
    let project = workspace.path().to_path_buf();
    light()
        .arg("lock")
        .arg(&project)
        .arg("--dry-run")
        .assert()
        .success()
        .stdout(predicate::str::contains("dry-run"));
    assert!(!project.join("light.lock").exists());
}

#[test]
fn update_dry_run_reports_change() {
    let workspace = temp_copy("outdated-project");
    let project = workspace.path().to_path_buf();
    light()
        .arg("update")
        .arg(&project)
        .arg("--dry-run")
        .assert()
        .success()
        .stdout(predicate::str::contains("→ 0.7.0"));
    let text = std::fs::read_to_string(project.join("Cargo.toml")).expect("read");
    assert!(
        text.contains("light-fw-element-form = \"0.0.1\""),
        "не изменено: {text}"
    );
}

#[test]
fn update_applies_new_version() {
    let workspace = temp_copy("outdated-project");
    let project = workspace.path().to_path_buf();
    light()
        .arg("update")
        .arg(&project)
        .arg("--yes")
        .arg("--no-verify")
        .assert()
        .success()
        .stdout(predicate::str::contains("обновлено"));
    let text = std::fs::read_to_string(project.join("Cargo.toml")).expect("read");
    assert!(text.contains("light-fw-element-form = \"0.7.0\""), "{text}");
}

#[test]
fn vendor_copies_component_sources() {
    let workspace = temp_copy("installed-workspace");
    let app = workspace.path().join("app");
    light()
        .arg("vendor")
        .arg("--offline")
        .current_dir(&app)
        .assert()
        .success()
        .stdout(predicate::str::contains("light-fw-element-form"));
    assert!(
        app.join("vendor/light-fw-element-form/Cargo.toml")
            .is_file(),
        "исходники скопированы"
    );
}

#[test]
fn vendor_path_flag_sets_output_dir() {
    let workspace = temp_copy("installed-workspace");
    let app = workspace.path().join("app");
    light()
        .arg("vendor")
        .arg("--path")
        .arg("vendor-test")
        .arg("--offline")
        .current_dir(&app)
        .assert()
        .success();
    assert!(
        app.join("vendor-test/light-fw-element-form/Cargo.toml")
            .is_file(),
        "--path задаёт каталог вывода"
    );
    assert!(!app.join("vendor").exists());
}

#[test]
fn vendor_positional_path_sets_output_dir() {
    let workspace = temp_copy("installed-workspace");
    let app = workspace.path().join("app");
    light()
        .arg("vendor")
        .arg("vendor-positional")
        .arg("--offline")
        .current_dir(&app)
        .assert()
        .success();
    assert!(
        app.join("vendor-positional/light-fw-element-form/Cargo.toml")
            .is_file(),
        "позиционный путь задаёт каталог вывода"
    );
}

#[test]
fn vendor_dry_run_does_not_copy() {
    let workspace = temp_copy("installed-workspace");
    let app = workspace.path().join("app");
    light()
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
    let workspace = temp_copy("aggregated-workspace");
    let app = workspace.path().join("app");
    light()
        .arg("add")
        .arg("light-fw-behavior-validation")
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
        text.contains("light-fw-behavior-validation = { workspace = true, optional = true }"),
        "зависимость из workspace: {text}"
    );
    assert!(
        text.contains("dep:light-fw-behavior-validation"),
        "компонент в агрегирующей фиче: {text}"
    );
    assert!(app.join("light.lock").exists(), "lock создан");

    let backup_dir = app.join("target").join(".light").join("backup");
    let has_backup = std::fs::read_dir(&backup_dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .any(|e| e.file_name().to_string_lossy().starts_with("Cargo.toml."))
        })
        .unwrap_or(false);
    assert!(has_backup, "снимок Cargo.toml в target/.light/backup");
}

#[test]
fn add_preserves_tricky_manifest_formatting() {
    let workspace = temp_copy("aggregated-workspace");
    let app = workspace.path().join("app");
    let manifest = app.join("Cargo.toml");
    let tricky = "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
                  # Зависимости приложения.\n[dependencies]\n\
                  light-fw-element-form = { workspace = true, optional = true } # формы\n\n\
                  [features]\ndefault = [\"full\"]\n\n\
                  # Агрегирующая фича: всё сразу.\n\
                  full = [\n    # формы\n    \"dep:light-fw-element-form\",\n]\n\n\
                  # пустая, но нужная\nminimal = []\n";
    std::fs::write(&manifest, tricky).expect("write tricky manifest");

    light()
        .arg("add")
        .arg("light-fw-behavior-validation")
        .arg(&app)
        .arg("--yes")
        .arg("--no-verify")
        .arg("--offline")
        .assert()
        .success();

    let text = std::fs::read_to_string(&manifest).expect("read manifest");
    assert!(
        text.contains(
            "full = [\n    # формы\n    \"dep:light-fw-element-form\",\n    \
             \"dep:light-fw-behavior-validation\",\n]"
        ),
        "каждый dep: на своей строке, комментарий не продублирован: {text}"
    );
    assert!(
        text.contains("light-fw-element-form = { workspace = true, optional = true } # формы"),
        "inline-комментарий зависимости сохранён: {text}"
    );
    assert!(
        text.contains("# Агрегирующая фича: всё сразу."),
        "комментарий перед фичей сохранён: {text}"
    );
    assert!(
        text.contains("minimal = []"),
        "пустая фича сохранена: {text}"
    );
}

#[test]
fn add_from_project_dir_resolves_workspace_source() {
    let workspace = temp_copy("component-workspace");
    let app = workspace.path().join("app");
    // Путь по умолчанию "." — раньше относительный путь ломал поиск workspace.
    light()
        .arg("add")
        .arg("light-fw-element-form")
        .arg("--dry-run")
        .arg("--offline")
        .current_dir(&app)
        .assert()
        .success()
        .stdout(predicate::str::contains("источник:    workspace"));
}

#[test]
fn doctor_reports_core_version_from_workspace() {
    let workspace = temp_copy("core-workspace");
    let app = workspace.path().join("app");
    light()
        .arg("doctor")
        .arg("--offline")
        .arg(&app)
        .assert()
        .success()
        .stdout(predicate::str::contains("light-fw-core 0.7.0"));
}

#[test]
fn remove_dry_run_reports_installed_version() {
    let workspace = temp_copy("core-workspace");
    let app = workspace.path().join("app");
    light()
        .arg("remove")
        .arg("light-fw-element-form")
        .arg(&app)
        .arg("--dry-run")
        .assert()
        .success()
        .stdout(predicate::str::contains("версия: 0.1.0"));
}

#[test]
fn remove_updates_lockfile() {
    let workspace = temp_copy("core-workspace");
    let app = workspace.path().join("app");
    light().arg("lock").arg(&app).assert().success();
    let before = std::fs::read_to_string(app.join("light.lock")).expect("read lock");
    assert!(
        before.contains("light-fw-element-form"),
        "до удаления: {before}"
    );

    light()
        .arg("remove")
        .arg("light-fw-element-form")
        .arg(&app)
        .arg("--yes")
        .assert()
        .success();

    let after = std::fs::read_to_string(app.join("light.lock")).expect("read lock");
    assert!(
        !after.contains("light-fw-element-form"),
        "компонент убран из lock: {after}"
    );
    assert!(after.contains("core = \"0.7.0\""), "ядро в lock: {after}");
}

#[test]
fn publish_dry_run_orders_crates_and_skips_unpublished() {
    let workspace = temp_copy("publish-workspace");
    let ws = workspace.path().to_path_buf();
    let output = light()
        .arg("publish")
        .arg("--dry-run")
        .arg("--skip-gates")
        .current_dir(&ws)
        .output()
        .expect("run light publish");
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
    let workspace = temp_copy("publish-workspace");
    let ws = workspace.path().to_path_buf();
    light()
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
    let workspace = temp_copy("publish-workspace");
    let ws = workspace.path().to_path_buf();
    light()
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
    let workspace = temp_copy("publish-workspace");
    let ws = workspace.path().to_path_buf();
    light()
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
    let workspace = temp_copy("publish-broken");
    let ws = workspace.path().to_path_buf();
    light()
        .arg("publish")
        .arg("--dry-run")
        .arg("--skip-gates")
        .current_dir(&ws)
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("publish = false"));
}

#[test]
fn init_creates_full_project() {
    let scratch = scratch_dir("full");
    light()
        .arg("init")
        .arg("cool-app")
        .current_dir(scratch.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("cool-app"));

    let project = scratch.path().join("cool-app");
    let manifest = std::fs::read_to_string(project.join("Cargo.toml")).expect("Cargo.toml");
    assert!(manifest.contains("name = \"cool-app\""), "{manifest}");
    assert!(
        manifest.contains(&format!("light-fw = \"{}\"", light_minor_version())),
        "{manifest}"
    );
    let main_rs = std::fs::read_to_string(project.join("src").join("main.rs")).expect("main.rs");
    assert!(main_rs.contains("#[template"), "{main_rs}");
    assert!(main_rs.contains("templates/app.html"), "{main_rs}");
    assert!(main_rs.contains("run_with_config"), "{main_rs}");
    let app_html = project.join("templates").join("app.html");
    assert!(app_html.is_file(), "templates/app.html создаётся");
    let html = std::fs::read_to_string(&app_html).expect("app.html");
    assert!(html.contains("Hello, Light!"), "{html}");
    let gitignore = std::fs::read_to_string(project.join(".gitignore")).expect(".gitignore");
    assert!(gitignore.contains("target/"), "{gitignore}");
    assert!(project.join("light.lock").is_file(), "light.lock создаётся");
}

#[test]
fn init_with_components_adds_dependencies_and_features() {
    let scratch = scratch_dir("with-components");
    light()
        .arg("init")
        .arg("form-app")
        .arg("--with")
        .arg("form")
        .arg("--with")
        .arg("validation")
        .current_dir(scratch.path())
        .assert()
        .success();

    let project = scratch.path().join("form-app");
    let manifest = std::fs::read_to_string(project.join("Cargo.toml")).expect("Cargo.toml");
    let mv = light_minor_version();
    assert!(
        manifest.contains(&format!(
            "light-fw = {{ version = \"{mv}\", features = [\"elements-form\", \"validation\"] }}"
        )),
        "{manifest}"
    );
    assert!(
        manifest.contains(&format!("light-fw-element-form = \"{mv}\"")),
        "{manifest}"
    );
    assert!(
        manifest.contains(&format!("light-fw-behavior-validation = \"{mv}\"")),
        "{manifest}"
    );

    let main_rs = std::fs::read_to_string(project.join("src").join("main.rs")).expect("main.rs");
    assert!(main_rs.contains("Пример формы"), "{main_rs}");
}

#[test]
fn init_with_template_creates_showcase_project() {
    let scratch = scratch_dir("template");
    light()
        .arg("init")
        .arg("todo-app")
        .arg("--template")
        .arg("todo")
        .current_dir(scratch.path())
        .assert()
        .success();

    let project = scratch.path().join("todo-app");
    let main_rs = std::fs::read_to_string(project.join("src").join("main.rs")).expect("main.rs");
    assert!(main_rs.contains("TodoApp"), "{main_rs}");
    let template = std::fs::read_to_string(project.join("templates").join("todo.html"))
        .expect("templates/todo.html");
    assert!(template.contains("Light Todo"), "{template}");
    assert!(project.join("light.lock").is_file(), "light.lock создаётся");
}

#[test]
fn init_with_unknown_template_fails() {
    let scratch = scratch_dir("template-unknown");
    light()
        .arg("init")
        .arg("bad-template")
        .arg("--template")
        .arg("unknown")
        .current_dir(scratch.path())
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("неизвестный шаблон"));
}

#[test]
fn init_with_unknown_component_fails() {
    let scratch = scratch_dir("with-unknown");
    light()
        .arg("init")
        .arg("bad-app")
        .arg("--with")
        .arg("unknown")
        .current_dir(scratch.path())
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("неизвестный компонент"));
}

#[test]
fn init_minimal_skips_extras() {
    let scratch = scratch_dir("minimal");
    light()
        .arg("init")
        .arg("tiny")
        .arg("--minimal")
        .current_dir(scratch.path())
        .assert()
        .success();

    let project = scratch.path().join("tiny");
    assert!(project.join("Cargo.toml").is_file());
    assert!(project.join("src").join("main.rs").is_file());
    assert!(!project.join(".gitignore").exists());
    assert!(!project.join("light.lock").exists());
}

#[test]
fn init_path_decouples_directory_from_name() {
    let scratch = scratch_dir("path");
    light()
        .arg("init")
        .arg("my-app")
        .arg("--path")
        .arg("custom-dir")
        .current_dir(scratch.path())
        .assert()
        .success();

    let project = scratch.path().join("custom-dir");
    let manifest = std::fs::read_to_string(project.join("Cargo.toml")).expect("Cargo.toml");
    assert!(manifest.contains("name = \"my-app\""), "{manifest}");
}

#[test]
fn init_rejects_non_empty_directory() {
    let scratch = scratch_dir("busy");
    let busy = scratch.path().join("busy-app");
    std::fs::create_dir_all(&busy).expect("dir");
    std::fs::write(busy.join("existing.txt"), "x").expect("file");

    light()
        .arg("init")
        .arg("busy-app")
        .current_dir(scratch.path())
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("не пуст"));
}

#[test]
fn init_rejects_invalid_name() {
    let scratch = scratch_dir("badname");
    light()
        .arg("init")
        .arg("2bad")
        .current_dir(scratch.path())
        .assert()
        .failure()
        .code(1);
}

/// ТЗ 0.16.0: `light devtools tree` без запущенного приложения даёт
/// понятную ошибку «Приложение не запущено» (порт никто не слушает).
#[test]
fn devtools_tree_not_running() {
    light()
        .arg("devtools")
        .arg("tree")
        .arg("--port")
        .arg("19229")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Приложение не запущено"));
}

/// ТЗ 0.16.0: clap парсит флаги `find --tag --text --attr`.
///
/// Здесь нет запущенного app, поэтому ожидаем ту же ошибку «не запущено» —
/// главное, что аргументы прошли валидацию clap (иначе exit 2 + «unexpected
/// argument»).
#[test]
fn devtools_find_parses_flags() {
    let output = light()
        .arg("devtools")
        .arg("find")
        .arg("--tag")
        .arg("button")
        .arg("--text")
        .arg("+1")
        .arg("--attr")
        .arg("class=btn")
        .arg("--port")
        .arg("19229")
        .output()
        .expect("run light");
    assert!(
        !output.status.success(),
        "ожидался отказ из-за незапущенного app, stdout={:?}",
        String::from_utf8_lossy(&output.stdout)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Приложение не запущено"),
        "флаги должны пройти парсинг и дойти до подключения, got: {stderr}"
    );
}

/// ТЗ 0.16.0: неизвестный подаргумент → clap exit 2.
#[test]
fn devtools_unknown_subcommand_is_clap_error() {
    light()
        .arg("devtools")
        .arg("bogus")
        .assert()
        .failure()
        .code(2);
}

/// ТЗ 0.16.0: `session start` создаёт каталог сессии, `stop` — summary.md.
///
/// Работает в изолированном temp-каталоге, приложение не требуется.
#[test]
fn devtools_session_roundtrip() {
    let scratch = scratch_dir("agent-session");
    // start
    light()
        .arg("devtools")
        .arg("session")
        .arg("start")
        .current_dir(scratch.path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Session started: .light/agent-sessions/",
        ));

    let sessions = scratch.path().join(".light").join("agent-sessions");
    assert!(sessions.is_dir(), "каталог сессий создан");
    assert!(sessions.join("current").is_file(), "маркер current создан");
    // ровно одна сессия (каталог)
    let entries: Vec<_> = std::fs::read_dir(&sessions)
        .expect("read sessions")
        .filter_map(Result::ok)
        .collect();
    let dirs: Vec<_> = entries
        .iter()
        .filter(|e| e.path().is_dir())
        .map(|e| e.path())
        .collect();
    assert_eq!(dirs.len(), 1, "одна директория сессии: {entries:?}");
    let sess = &dirs[0];
    assert!(sess.join("actions.jsonl").is_file());
    assert!(sess.join("screenshots").is_dir());
    assert!(sess.join("state").is_dir());

    // stop
    light()
        .arg("devtools")
        .arg("session")
        .arg("stop")
        .current_dir(scratch.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Session stopped"));

    assert!(!sessions.join("current").exists(), "маркер удалён");
    assert!(sess.join("summary.md").is_file(), "summary.md сгенерирован");
    let summary = std::fs::read_to_string(sess.join("summary.md")).expect("summary");
    assert!(summary.contains("# Agent Session Summary"));
    assert!(summary.contains("**Actions:**"));
}
