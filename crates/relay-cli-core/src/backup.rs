//! Резервное копирование `Cargo.toml` и `relay.lock` перед мутациями.
//!
//! Копии складываются в `.relay/backup/` в корне проекта. Хранятся последние
//! [`MAX_BACKUPS`] снимков; более старые удаляются. При ошибке мутации проект
//! восстанавливается из последнего снимка.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::{Error, Result};

/// Каталог резервных копий (относительно корня проекта).
pub const BACKUP_DIR: &str = ".relay/backup";

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
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("{millis:013}")
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
pub fn restore(backup: &Backup) -> Result<()> {
    for (original, saved) in &backup.entries {
        std::fs::copy(saved, original).map_err(|e| {
            Error::Backup(format!(
                "не удалось восстановить {}: {e}",
                original.display()
            ))
        })?;
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
}
