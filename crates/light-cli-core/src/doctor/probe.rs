use crate::process::{CommandRunner, SystemRunner, parse_program_version};

/// Минимальная поддерживаемая версия Rust.
pub const MIN_RUST_VERSION: (u64, u64, u64) = (1, 75, 0);

/// Источник данных об окружении.
pub trait EnvironmentProbe: Send + Sync {
    /// Версия `rustc`.
    fn rustc_version(&self) -> Option<semver::Version>;
    /// Версия `cargo`.
    fn cargo_version(&self) -> Option<semver::Version>;
    /// Версия `rustup` (опционально).
    fn rustup_version(&self) -> Option<semver::Version>;
    /// Доступна ли сеть (crates.io).
    fn network_reachable(&self) -> bool;
}

/// Реальный опрос окружения через запуск процессов.
#[derive(Debug, Clone, Copy)]
pub struct SystemProbe {
    runner: SystemRunner,
    assume_offline: bool,
}

impl Default for SystemProbe {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemProbe {
    /// Создаёт пробник на базе системного раннера.
    pub fn new() -> Self {
        Self {
            runner: SystemRunner,
            assume_offline: false,
        }
    }

    /// Пробник, который не проверяет сеть (режим `--offline`).
    pub fn offline() -> Self {
        Self {
            runner: SystemRunner,
            assume_offline: true,
        }
    }

    fn probe_version(&self, program: &str) -> Option<semver::Version> {
        let output = self
            .runner
            .run(program, &["--version".to_string()], None)
            .ok()?;
        if !output.success() {
            return None;
        }
        parse_program_version(&output.stdout)
    }
}

impl EnvironmentProbe for SystemProbe {
    fn rustc_version(&self) -> Option<semver::Version> {
        self.probe_version("rustc")
    }

    fn cargo_version(&self) -> Option<semver::Version> {
        self.probe_version("cargo")
    }

    fn rustup_version(&self) -> Option<semver::Version> {
        self.probe_version("rustup")
    }

    fn network_reachable(&self) -> bool {
        if self.assume_offline {
            return false;
        }
        #[cfg(feature = "network")]
        {
            crate::fetch::ping_crates_io()
        }
        #[cfg(not(feature = "network"))]
        {
            false
        }
    }
}
