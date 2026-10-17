//! Резервное копирование `Cargo.toml` и `relay.lock` перед мутациями.
//!
//! Копии складываются в `target/.relay/backup/` внутри проекта и именуются по
//! RFC3339-времени (`Cargo.toml.2026-10-17T13-45-00Z`). Хранятся последние
//! [`MAX_BACKUPS`] снимков; более старые удаляются. При ошибке мутации проект
//! восстанавливается из снимка.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::{Error, Result};

/// Каталог резервных копий (относительно корня проекта).
pub const BACKUP_DIR: &str = "target/.relay/backup";

/// Сколько снимков хранить.
pub const MAX_BACKUPS: usize = 10;

/// Файлы, которые бэкапим.
const TRACKED_FILES: [&str; 2] = ["Cargo.toml", "relay.lock"];

/// Снимок состояния файлов проекта.
#[derive(Debug, Clone)]
pub struct Backup {
    /// Корень проекта, к которому относится снимок.
    pub project_root: PathBuf,
    /// Метка времени снимка.
    pub stamp: String,
    /// Пары `(путь_в_проекте, путь_копии)`.
    pub entries: Vec<(PathBuf, PathBuf)>,
}

fn now_stamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format_timestamp(secs)
}

/// Форматирует время Unix (UTC) как `2026-10-17T13-45-00Z`.
///
/// Двоеточия заменены на дефисы, чтобы метка была допустимым именем файла в
/// том числе в Windows.
pub fn format_timestamp(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (year, month, day) = civil_from_days(days);
    let hour = rem / 3_600;
    let minute = (rem % 3_600) / 60;
    let second = rem % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}-{minute:02}-{second:02}Z")
}

/// Преобразует число дней от 1970-01-01 в `(год, месяц, день)`.
///
/// Алгоритм Говарда Хиннанта «civil_from_days» (public domain).
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (if month <= 2 { year + 1 } else { year }, month, day)
}

/// Создаёт снимок `Cargo.toml` (и `relay.lock`, если есть).
pub fn create(project_root: &Path) -> Result<Backup> {
    let dir = project_root.join(BACKUP_DIR);
    std::fs::create_dir_all(&dir)?;
    let stamp = now_stamp();

    let mut entries = Vec::new();
    for file in TRACKED_FILES {
        let original = project_root.join(file);
        if !original.is_file() {
            continue;
        }
        let target = dir.join(format!("{file}.{stamp}"));
        std::fs::copy(&original, &target)
            .map_err(|e| Error::Backup(format!("не удалось сохранить {file}: {e}")))?;
        entries.push((original, target));
    }

    let backup = Backup {
        project_root: project_root.to_path_buf(),
        stamp,
        entries,
    };
    prune(project_root)?;
    Ok(backup)
}

/// Восстанавливает файлы из снимка.
///
/// Файлы, которые существовали на момент снимка, возвращаются к прежнему
/// содержимому; файлы, которых тогда не было (например, только что созданный
/// `relay.lock`), удаляются, чтобы откат был полным.
pub fn restore(backup: &Backup) -> Result<()> {
    for (original, saved) in &backup.entries {
        std::fs::copy(saved, original).map_err(|e| {
            Error::Backup(format!(
                "не удалось восстановить {}: {e}",
                original.display()
            ))
        })?;
    }
    for file in TRACKED_FILES {
        let original = backup.project_root.join(file);
        let existed = backup.entries.iter().any(|(path, _)| path == &original);
        if !existed && original.exists() {
            std::fs::remove_file(&original).map_err(|e| {
                Error::Backup(format!("не удалось удалить {}: {e}", original.display()))
            })?;
        }
    }
    Ok(())
}

/// Удаляет старые снимки, оставляя последние [`MAX_BACKUPS`].
pub fn prune(project_root: &Path) -> Result<()> {
    let dir = project_root.join(BACKUP_DIR);
    if !dir.is_dir() {
        return Ok(());
    }
    let mut stamps: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if let Some((_, stamp)) = name.rsplit_once('.') {
            stamps.push(stamp.to_string());
        }
    }
    stamps.sort();
    stamps.dedup();
    if stamps.len() <= MAX_BACKUPS {
        return Ok(());
    }
    let remove_count = stamps.len() - MAX_BACKUPS;
    let to_remove: Vec<String> = stamps.into_iter().take(remove_count).collect();
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if let Some((_, stamp)) = name.rsplit_once('.')
            && to_remove.iter().any(|s| s == stamp)
        {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("relay-cli-backup-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn creates_and_restores_backup() {
        let dir = temp_dir("restore");
        fs::write(dir.join("Cargo.toml"), "original").expect("write");
        let backup = create(&dir).expect("create");
        assert_eq!(backup.entries.len(), 1);

        fs::write(dir.join("Cargo.toml"), "modified").expect("write");
        restore(&backup).expect("restore");
        assert_eq!(
            fs::read_to_string(dir.join("Cargo.toml")).expect("read"),
            "original"
        );
    }

    #[test]
    fn restore_removes_created_files() {
        let dir = temp_dir("restore-new");
        fs::write(dir.join("Cargo.toml"), "c").expect("write");
        let backup = create(&dir).expect("create");
        fs::write(dir.join("relay.lock"), "generated").expect("write");
        restore(&backup).expect("restore");
        assert!(!dir.join("relay.lock").exists(), "созданный lock удалён");
    }

    #[test]
    fn backs_up_lockfile_when_present() {
        let dir = temp_dir("lock");
        fs::write(dir.join("Cargo.toml"), "c").expect("write");
        fs::write(dir.join("relay.lock"), "l").expect("write");
        let backup = create(&dir).expect("create");
        assert_eq!(backup.entries.len(), 2);
    }

    #[test]
    fn prune_keeps_recent() {
        let dir = temp_dir("prune");
        let bdir = dir.join(BACKUP_DIR);
        fs::create_dir_all(&bdir).expect("backup dir");
        for i in 0..(MAX_BACKUPS + 5) {
            fs::write(bdir.join(format!("Cargo.toml.{i:013}")), "x").expect("write");
        }
        prune(&dir).expect("prune");
        let count = fs::read_dir(&bdir).expect("read").count();
        assert_eq!(count, MAX_BACKUPS);
    }

    #[test]
    fn formats_rfc3339_timestamp() {
        assert_eq!(format_timestamp(0), "1970-01-01T00-00-00Z");
        assert_eq!(format_timestamp(1_000_000_000), "2001-09-09T01-46-40Z");
        assert_eq!(format_timestamp(1_795_268_700), "2026-11-21T13-45-00Z");
    }

    #[test]
    fn stores_backups_under_target_relay() {
        let dir = temp_dir("location");
        fs::write(dir.join("Cargo.toml"), "c").expect("write");
        let backup = create(&dir).expect("create");
        let saved = &backup.entries[0].1;
        assert!(
            saved.starts_with(dir.join("target").join(".relay").join("backup")),
            "копия внутри target/.relay/backup: {}",
            saved.display()
        );
    }
}
