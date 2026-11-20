use std::path::{Path, PathBuf};

#[cfg(feature = "network")]
use super::http::{fetch_crate_archive, fetch_crates_io, fetch_crates_io_search, fetch_owners};
use crate::error::{Error, Result};
use crate::manifest::Provides;
#[cfg(feature = "network")]
use crate::registry_config::RegistryConfig;
/// Источник, из которого разрешён компонент.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    /// Локальный член workspace.
    Workspace,
    /// Локальная копия из `vendor/`.
    Vendor,
    /// Опубликованный крейт на crates.io.
    CratesIo,
}

impl SourceKind {
    /// Строковое имя для `light.lock`.
    pub fn as_str(self) -> &'static str {
        match self {
            SourceKind::Workspace => "workspace",
            SourceKind::Vendor => "vendor",
            SourceKind::CratesIo => "crates.io",
        }
    }
}

/// Разрешённый компонент с метаданными.
#[derive(Debug, Clone)]
pub struct ResolvedComponent {
    /// Имя крейта.
    pub name: String,
    /// Версия компонента.
    pub version: semver::Version,
    /// Требование к версии ядра.
    pub core: semver::VersionReq,
    /// Фичи мета-крейта `light-fw`, которые включает компонент.
    pub light_features: Vec<String>,
    /// Что предоставляет компонент.
    pub provides: Provides,
    /// Обязательные компоненты (`"name ^ver"`).
    pub requires: Vec<String>,
    /// Несовместимые компоненты.
    pub conflicts: Vec<String>,
    /// Источник.
    pub source: SourceKind,
    /// Путь к локальному компоненту (для workspace/vendor).
    pub path: Option<PathBuf>,
}

impl ResolvedComponent {
    pub(crate) fn from_registry(info: &crate::registry::ComponentInfo) -> Self {
        Self {
            name: info.name.clone(),
            version: info.latest_version.clone(),
            core: info.core_requirement.clone(),
            light_features: info.light_features.clone(),
            provides: info.provides.clone(),
            requires: Vec::new(),
            conflicts: Vec::new(),
            source: SourceKind::CratesIo,
            path: None,
        }
    }
}

/// Результат поиска крейта в реестре.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CrateSearchResult {
    /// Имя крейта.
    pub name: String,
    /// Последняя стабильная версия.
    pub version: String,
    /// Краткое описание.
    pub description: Option<String>,
    /// Ссылка на страницу крейта.
    pub url: String,
    /// Количество загрузок крейта (из API; 0 если недоступно).
    pub downloads: u64,
    /// Владельцы крейта (logins из API).
    pub owners: Vec<String>,
}

/// Источник данных crates.io: версии и поиск.
///
/// Трейт позволяет подменять сетевые запросы в тестах.
pub trait RemoteRegistry: Send + Sync {
    /// Возвращает последнюю стабильную версию или `None`, если крейт не найден.
    fn latest_version(&self, name: &str) -> Result<Option<semver::Version>> {
        let _ = name;
        Ok(None)
    }

    /// Ищет крейты по запросу.
    fn search(&self, query: &str, per_page: usize) -> Result<Vec<CrateSearchResult>> {
        let _ = (query, per_page);
        Ok(Vec::new())
    }

    /// Возвращает список login'ов владельцев крейта.
    ///
    /// При ошибке сети/404 возвращает пустой вектор — вызывающий
    /// интерпретирует это как «неизвестно», и компонент не считается
    /// официальным.
    fn owners(&self, name: &str) -> Result<Vec<String>> {
        let _ = name;
        Ok(Vec::new())
    }

    /// Скачивает `.crate`-архив (tar.gz) крейта в память.
    fn download_crate(&self, name: &str, version: &str) -> Result<Vec<u8>> {
        let _ = (name, version);
        Err(Error::Offline(
            "скачивание крейта недоступно без фичи `network`".to_string(),
        ))
    }
}

/// Реальный клиент crates.io API (работает только с фичей `network`).
#[derive(Debug, Default, Clone, Copy)]
pub struct CratesIoRegistry;

impl RemoteRegistry for CratesIoRegistry {
    fn latest_version(&self, name: &str) -> Result<Option<semver::Version>> {
        #[cfg(feature = "network")]
        {
            fetch_crates_io(name, &RegistryConfig::default_registry())
        }
        #[cfg(not(feature = "network"))]
        {
            let _ = name;
            Ok(None)
        }
    }

    fn search(&self, query: &str, per_page: usize) -> Result<Vec<CrateSearchResult>> {
        #[cfg(feature = "network")]
        {
            fetch_crates_io_search(query, per_page, &RegistryConfig::default_registry())
        }
        #[cfg(not(feature = "network"))]
        {
            let _ = (query, per_page);
            Ok(Vec::new())
        }
    }
}

/// Реестровый клиент с настраиваемым базовым URL.
///
/// Обёртка вокруг [`crate::registry_config::RegistryConfig`] для
/// `light search`: использует URLs из конфига вместо хардкода
/// `https://crates.io`. Позволяет подменять сервер в wiremock-тестах.
#[derive(Debug, Clone)]
pub struct RegistryClient {
    config: crate::registry_config::RegistryConfig,
}

impl RegistryClient {
    /// Создаёт клиент из [`RegistryConfig`](crate::registry_config::RegistryConfig).
    pub fn new(config: crate::registry_config::RegistryConfig) -> Self {
        Self { config }
    }

    /// Доступ к конфигурации реестра (URLs, source).
    pub fn config(&self) -> &crate::registry_config::RegistryConfig {
        &self.config
    }

    /// Загружает конфиг из проекта (файл `light.config.json`),
    /// при отсутствии — дефолтный crates.io с предупреждением.
    pub fn from_project(project_root: &Path) -> (Self, Option<String>) {
        let (config, warning) = crate::registry_config::RegistryConfig::load(project_root);
        (Self::new(config), warning)
    }
}

impl RemoteRegistry for RegistryClient {
    fn latest_version(&self, name: &str) -> Result<Option<semver::Version>> {
        #[cfg(feature = "network")]
        {
            fetch_crates_io(name, &self.config)
        }
        #[cfg(not(feature = "network"))]
        {
            let _ = name;
            Ok(None)
        }
    }

    fn search(&self, query: &str, per_page: usize) -> Result<Vec<CrateSearchResult>> {
        #[cfg(feature = "network")]
        {
            fetch_crates_io_search(query, per_page, &self.config)
        }
        #[cfg(not(feature = "network"))]
        {
            let _ = (query, per_page);
            Ok(Vec::new())
        }
    }

    fn owners(&self, name: &str) -> Result<Vec<String>> {
        #[cfg(feature = "network")]
        {
            fetch_owners(name, &self.config)
        }
        #[cfg(not(feature = "network"))]
        {
            let _ = name;
            Ok(Vec::new())
        }
    }

    fn download_crate(&self, name: &str, version: &str) -> Result<Vec<u8>> {
        #[cfg(feature = "network")]
        {
            fetch_crate_archive(name, version, &self.config)
        }
        #[cfg(not(feature = "network"))]
        {
            let _ = (name, version);
            Err(Error::Offline(
                "скачивание крейта недоступно без фичи `network`".to_string(),
            ))
        }
    }
}
