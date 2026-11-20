use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Путь к файлу build-info.json в target/.light/.
pub fn build_info_path(target_root: &Path) -> PathBuf {
    target_root.join(".light").join("build-info.json")
}

/// Читает и парсит `build-info.json`. Повреждённый файл — `None` (перезапишется),
/// отсутствующий файл — `None` (первая сборка: кэш не инвалидируется, просто
/// фиксируем текущую версию). Ошибки записи игнорируются: при неудаче запись
/// кэш не сломает, но следующая сборка будет повторять попытку записи.
pub fn save_build_info(path: &Path, version: &str, lock_hash: &str) {
    if let Some(parent) = path.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        eprintln!("light dev: не удалось создать каталог для build-info: {e}");
        return;
    }
    let v = serde_json::json!({
        "light-fw-core-version": version,
        "cargo-lock-hash": lock_hash,
    });
    match serde_json::to_string_pretty(&v) {
        Ok(s) => {
            if let Err(e) = std::fs::write(path, s) {
                eprintln!("light dev: не удалось записать build-info.json: {e}");
            }
        }
        Err(e) => eprintln!("light dev: не удалось сериализовать build-info: {e}"),
    }
}

/// Текущая версия `light-fw-core` в `Cargo.lock` проекта. Читаем `Cargo.lock`
/// напрямую (без запуска `cargo metadata`): файл маленький, а `cargo metadata`
/// иногда долго и может не сработать без сети.
pub fn read_fw_core_version_from_lock(lock_path: &Path) -> Option<String> {
    let data = std::fs::read_to_string(lock_path).ok()?;
    parse_fw_core_version_from_lock_text(&data)
}

/// Парсит версию `light-fw-core` из текста `Cargo.lock` (только чистая функция,
/// без I/O): ищем `[dependencies] name="light-fw-core"` ... `version="..."` и
/// берём первый попавшийся блок.
pub(super) fn parse_fw_core_version_from_lock_text(text: &str) -> Option<String> {
    // Ищем блок, начинающийся с `name = "light-fw-core"`, затем в нём ищем
    // первую строку `version = "..."`.
    let start = text.find("name = \"light-fw-core\"")?;
    let after = &text[start..];
    let end = after.find("\n\n").unwrap_or(after.len());
    let block = &after[..end];
    let ver_start = block.find("version = \"")? + "version = \"".len();
    let rest = &block[ver_start..];
    let ver_end = rest.find('"')?;
    Some(rest[..ver_end].to_string())
}

/// Текущая версия фреймворка в проекте. Порядок:
/// 1. `[workspace.package].version` (workspace-проект).
/// 2. `[dependencies] light = "0.15"` (обёрточный крейт — версия идентична).
/// 3. `[dependencies] light-fw-core = "0.15"` (прямая зависимость).
pub fn read_fw_core_version_from_workspace_toml(toml_path: &Path) -> Option<String> {
    let data = std::fs::read_to_string(toml_path).ok()?;
    parse_workspace_package_version(&data).or_else(|| parse_light_dep_version(&data))
}

/// Парсит `[workspace.package] version = "..."` из текста `Cargo.toml`
/// (чистая функция, без I/O). Использует `toml_edit` для корректного разбора.
pub(super) fn parse_workspace_package_version(text: &str) -> Option<String> {
    let doc: toml_edit::DocumentMut = text.parse().ok()?;
    let ver = doc
        .get("workspace")?
        .get("package")?
        .get("version")?
        .as_str()?
        .to_string();
    Some(ver)
}

/// Парсит версию обёрточного крейта из `[dependencies]` (`light-fw`, затем старый `light`).
/// Возвращает `None`, если ни одна из зависимостей отсутствует (не workspace-проект
/// и нет прямой зависимости).
pub(super) fn parse_light_dep_version(text: &str) -> Option<String> {
    let doc: toml_edit::DocumentMut = text.parse().ok()?;
    let deps = doc.get("dependencies")?.as_table()?;
    // Первый приоритет: обёрточный крейт `light-fw` (новый ключ), затем
    // старый `light` для уже опубликованных проектов.
    if let Some(entry) = deps.get("light-fw") {
        return extract_dep_version(entry);
    }
    // Обратная совместимость: старый обёрточный крейт `light`.
    if let Some(entry) = deps.get("light") {
        return extract_dep_version(entry);
    }
    // Fallback: прямая зависимость `light-fw-core`.
    deps.get("light-fw-core").and_then(extract_dep_version)
}

/// Извлекает версионную строку из TOML-записи зависимости:
/// `light = "0.15"` → `"0.15"`, `light = { version = "0.15" }` → `"0.15"`.
pub(super) fn extract_dep_version(entry: &toml_edit::Item) -> Option<String> {
    match entry {
        toml_edit::Item::Value(v) => v.as_str().map(str::to_string),
        toml_edit::Item::Table(t) => t.get("version")?.as_str().map(str::to_string),
        _ => None,
    }
}

/// Удаляет всё, что нужно для полного сброса кэша dev-сборки (ТЗ 0.14.4):
/// `target/hot-build`, `target/hot-run`, `.light/state.json`,
/// `.light/build-info.json` и артефакты зависимостей текущего пакета из
/// `target/debug/deps/` (только `<package>*`).
/// Возвращает список удаляемых путей (для логирования и теста).
pub fn clean_dev_cache(target_root: &Path, package: Option<&str>) -> std::vec::Vec<PathBuf> {
    let mut removed = Vec::new();
    // 1) target/hot-build
    let hot_build = target_root.join("target").join("hot-build");
    remove_path_quietly(&hot_build, &mut removed);
    // 2) target/hot-run
    let hot_run = target_root.join("target").join("hot-run");
    remove_path_quietly(&hot_run, &mut removed);
    // 3) .light/state.json
    let state = target_root.join(".light").join("state.json");
    remove_path_quietly(&state, &mut removed);
    // 4) .light/build-info.json
    let build_info = build_info_path(target_root);
    remove_path_quietly(&build_info, &mut removed);
    // 5) target/debug/deps/<package>*
    if let Some(pkg) = package {
        let deps = target_root.join("target").join("debug").join("deps");
        if let Ok(entries) = std::fs::read_dir(&deps) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if name.starts_with(pkg) {
                    remove_path_quietly(&entry.path(), &mut removed);
                }
            }
        }
    }
    removed
}

/// Рекурсивно удаляет `path` (если он существует), тихо игнорируя ошибки
/// (файл мог быть уже занят автовикором или ОС). Успешные удаления
/// добавляются в `removed`.
pub(super) fn remove_path_quietly(path: &Path, removed: &mut Vec<PathBuf>) {
    if !path.exists() {
        return;
    }
    let kind = std::fs::symlink_metadata(path).map(|m| m.file_type()).ok();
    if matches!(kind, Some(t) if t.is_dir()) {
        if std::fs::remove_dir_all(path).is_ok() {
            removed.push(path.to_path_buf());
        }
    } else if std::fs::remove_file(path).is_ok() {
        removed.push(path.to_path_buf());
    }
}

/// Убирает из `run_dir` устаревшие копии исполняемых файлов:
/// * любой `.exe` с mtime старше `STALE_RUN_AGE`;
/// * либо любой `.exe`, чей суффикс-timestamp (после `<bin>-`) **не совпадает**
///   с «сегодня» (это защищает от старых артефактов, которые не успели
///   подхватить автовикором).
///
/// Файл `keep` (сейчас работающий) НИКОГДА не удаляется, независимо от age.
/// Возвращает число удалённых копий.
pub fn prune_stale_run_exes(run_dir: &Path, keep: &Path) -> usize {
    let now = SystemTime::now();
    let today: u128 = now
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() / 86_400)
        .unwrap_or(0) as u128;
    let mut pruned = 0usize;
    let Ok(entries) = std::fs::read_dir(run_dir) else {
        return 0;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path == keep {
            continue;
        }
        let is_exe = path.extension().and_then(|e| e.to_str()) == Some("exe");
        if !is_exe {
            continue;
        }
        let meta = match std::fs::metadata(&path) {
            Ok(m) => m,
            Err(_) => continue,
        };
        let mtime = meta.modified().ok();
        let old_by_age = mtime
            .and_then(|t| now.duration_since(t).ok())
            .map(|d| d >= STALE_RUN_AGE)
            .unwrap_or(false);
        // Суффикс timestamp: `<bin>-<ts>.exe` → ts в миллисекундах. День
        // считается как floor(ts_ms / 86_400_000); сравним с «сегодня».
        let stale_by_date = {
            let file = path.file_name().and_then(|f| f.to_str()).unwrap_or("");
            let stem = file.strip_suffix(".exe").unwrap_or(file);
            match stem.rfind('-') {
                None => false,
                Some(dash_pos) => {
                    let ts_str = &stem[dash_pos + 1..];
                    ts_str
                        .parse::<u128>()
                        .map(|ts| (ts / 86_400_000) != today)
                        .unwrap_or(false)
                }
            }
        };
        if (old_by_age || stale_by_date) && std::fs::remove_file(&path).is_ok() {
            pruned += 1;
        }
    }
    pruned
}

/// Лимит возраста (mtime) для старых копий в `target/hot-run`.
pub(super) const STALE_RUN_AGE: Duration = Duration::from_secs(3600);
/// Детектирует рассинхрон между установленным CLI (`env!("CARGO_PKG_VERSION")`
/// бинарника) и версией `light-fw-core` в проекте (первоисточник —
/// `[workspace.package].version` из `Cargo.toml`; fallback — `Cargo.lock`).
/// Возвращает `(installed, project)` только если они различаются.
/// Сравнивает две версионные строки по `major.minor`: `"0.15.3"` vs `"0.15"` →
/// `true` (одна и та же минорная ветка); `"0.14"` vs `"0.15"` → `false`.
pub(super) fn same_minor(a: &str, b: &str) -> bool {
    minor_key(a) == minor_key(b)
}

/// Извлекает `major.minor` из версионной строки: `"0.15.3"` → `"0.15"`,
/// `"0.15"` → `"0.15"`.
pub(super) fn minor_key(v: &str) -> String {
    let parts: Vec<&str> = v.splitn(3, '.').collect();
    if parts.len() >= 2 {
        format!("{}.{}", parts[0], parts[1])
    } else {
        v.to_string()
    }
}

/// Предупреждение при рассинхроне между установленным CLI и версией
/// light-fw-core в проекте (сравнение по `major.minor`).
pub fn check_cli_version_mismatch(
    installed: &str,
    toml_path: &Path,
    lock_path: &Path,
) -> Option<(String, String)> {
    let project = read_fw_core_version_from_workspace_toml(toml_path)
        .or_else(|| read_fw_core_version_from_lock(lock_path))?;
    if same_minor(installed, &project) {
        None
    } else {
        Some((installed.to_string(), project))
    }
}
