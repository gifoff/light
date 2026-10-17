use std::path::PathBuf;

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
