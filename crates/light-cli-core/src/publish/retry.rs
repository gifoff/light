use std::collections::BTreeSet;
use std::path::Path;
use std::time::Duration;

use crate::error::Result;
use crate::process::CommandRunner;

use super::gates::{PublishInvocation, publish_args};

/// Причина сбоя `cargo publish`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
    /// Версия уже есть в git-индексе crates.io, хотя API её ещё не показывает.
    AlreadyPublished,
    /// Индекс crates.io ещё не увидел предыдущий крейт — стоит повторить.
    IndexStale,
    /// crates.io ограничил частоту публикаций — ретраи бесполезны.
    RateLimited,
    /// Прочая ошибка — ретраи не помогут.
    Other,
}

/// Паттерны stdout/stderr, означающие, что версия уже есть в git-индексе.
///
/// crates.io API может отставать от индекса, который использует `cargo`;
/// в этом случае `cargo publish` завершается ошибкой, хотя публиковать уже
/// нечего.
pub const ALREADY_PUBLISHED_PATTERNS: &[&str] = &[
    "already exists on crates.io index",
    "already uploaded",
    "already been published",
    "already published",
];

/// Паттерны stderr, означающие «индекс ещё не обновился».
///
/// Список сознательно вынесен в константу и легко расширяется.
pub const INDEX_STALE_PATTERNS: &[&str] = &[
    "failed to select a version",
    "no matching package named",
    "could not be found in registry index",
];

/// Паттерны stderr, означающие превышение лимита публикаций crates.io.
pub const RATE_LIMIT_PATTERNS: &[&str] = &[
    "429 too many requests",
    "too many new crates",
    "rate limit reached",
    "too many requests",
];

/// Классифицирует вывод `cargo publish` (exit code всегда 101, поэтому — по тексту).
pub fn classify_publish_failure(stderr: &str) -> FailureKind {
    let lower = stderr.to_lowercase();
    if RATE_LIMIT_PATTERNS
        .iter()
        .any(|pattern| lower.contains(pattern))
    {
        FailureKind::RateLimited
    } else if ALREADY_PUBLISHED_PATTERNS
        .iter()
        .any(|pattern| lower.contains(pattern))
    {
        FailureKind::AlreadyPublished
    } else if INDEX_STALE_PATTERNS
        .iter()
        .any(|pattern| lower.contains(pattern))
    {
        FailureKind::IndexStale
    } else {
        FailureKind::Other
    }
}

/// Извлекает из stderr дату/время, после которого crates.io снова примет
/// публикацию.
///
/// crates.io кладёт срок в текст ошибки (`retry after <timestamp>`,
/// `try again after <timestamp>`); формат может отличаться, поэтому берём
/// первый похожий на время токен после ключевой фразы.
pub fn rate_limit_reset(stderr: &str) -> Option<String> {
    const KEYS: &[&str] = &[
        "try again after",
        "retry after",
        "available after",
        "wait until",
        "try again at",
        "again after",
    ];
    let lower = stderr.to_lowercase();
    for key in KEYS {
        let Some(position) = lower.find(key) else {
            continue;
        };
        let start = position + key.len();
        if let Some(reset) = stderr.get(start..).and_then(reset_token) {
            return Some(reset);
        }
    }
    None
}

fn reset_token(rest: &str) -> Option<String> {
    let begin = rest.find(|c: char| c.is_ascii_digit())?;
    let tail = &rest[begin..];
    let end = tail
        .find(|c: char| c.is_whitespace() || matches!(c, ',' | ';' | ')' | ']'))
        .unwrap_or(tail.len());
    let token = tail[..end].trim_end_matches(['.', ',', ';']);
    if token.len() < 4 {
        return None;
    }
    let looks_like_time = token.contains('-')
        || token.contains(':')
        || token.chars().any(|c| c.is_ascii_alphabetic());
    looks_like_time.then(|| token.to_string())
}

/// Настройки ретраев на устаревание индекса.
#[derive(Debug, Clone, Copy)]
pub struct RetryConfig {
    /// Сколько раз повторить попытку (не считая первой).
    pub max_retries: u32,
    /// Базовая задержка.
    pub base: Duration,
    /// Множитель экспоненциального backoff.
    pub factor: u32,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 6,
            base: Duration::from_secs(5),
            factor: 2,
        }
    }
}

impl RetryConfig {
    /// Читает настройки из окружения (`LIGHT_PUBLISH_RETRY_*`).
    ///
    /// - `LIGHT_PUBLISH_RETRY_MAX` — число повторов (по умолчанию 6);
    /// - `LIGHT_PUBLISH_RETRY_BASE_MS` — базовая задержка в мс (по умолчанию 5000);
    /// - `LIGHT_PUBLISH_RETRY_FACTOR` — множитель (по умолчанию 2).
    pub fn from_env() -> Self {
        let mut config = Self::default();
        if let Ok(value) = std::env::var("LIGHT_PUBLISH_RETRY_MAX")
            && let Ok(parsed) = value.parse()
        {
            config.max_retries = parsed;
        }
        if let Ok(value) = std::env::var("LIGHT_PUBLISH_RETRY_BASE_MS")
            && let Ok(parsed) = value.parse::<u64>()
        {
            config.base = Duration::from_millis(parsed);
        }
        if let Ok(value) = std::env::var("LIGHT_PUBLISH_RETRY_FACTOR")
            && let Ok(parsed) = value.parse::<u32>()
            && parsed >= 1
        {
            config.factor = parsed;
        }
        config
    }

    /// Задержка перед повтором номер `retry_index` (с нуля).
    pub fn delay(&self, retry_index: u32) -> Duration {
        let mut delay = self.base;
        for _ in 0..retry_index {
            delay = delay.saturating_mul(self.factor);
        }
        delay
    }
}

/// Ожидание между ретраями (инжектируется, чтобы тесты не спали).
pub trait Sleeper: Send + Sync {
    /// Спит `duration`.
    fn sleep(&self, duration: Duration);
}

/// Реальный [`Sleeper`] на `std::thread::sleep`.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemSleeper;

impl Sleeper for SystemSleeper {
    fn sleep(&self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

/// Итог публикации одного крейта.
#[derive(Debug, Clone)]
pub struct PublishAttempt {
    /// Успешно ли опубликован в этот раз.
    pub success: bool,
    /// Версия уже была в git-индексе crates.io, публикация пропущена.
    pub already_published: bool,
    /// Число сделанных повторов.
    pub retries: u32,
    /// stderr последней неудачной попытки.
    pub stderr: String,
}

/// Публикует крейт с ретраями на устаревание индекса crates.io.
///
/// Ретраи делаются только для [`FailureKind::IndexStale`]; прочие ошибки
/// возвращаются сразу. Если версия уже есть в git-индексе, попытка помечается
/// [`PublishAttempt::already_published`] и не считается сбоем.
pub fn publish_with_retries(
    runner: &dyn CommandRunner,
    sleeper: &dyn Sleeper,
    root: &Path,
    name: &str,
    invocation: &PublishInvocation,
    retry: &RetryConfig,
) -> Result<PublishAttempt> {
    let args = publish_args(name, invocation);
    let mut retries = 0;
    loop {
        let output = runner.run("cargo", &args, Some(root))?;
        if output.success() {
            return Ok(PublishAttempt {
                success: true,
                already_published: false,
                retries,
                stderr: String::new(),
            });
        }
        let diagnostics = format!("{}\n{}", output.stdout, output.stderr);
        let kind = classify_publish_failure(&diagnostics);
        if kind == FailureKind::AlreadyPublished {
            return Ok(PublishAttempt {
                success: false,
                already_published: true,
                retries,
                stderr: output.stderr,
            });
        }
        if kind != FailureKind::IndexStale || retries >= retry.max_retries {
            return Ok(PublishAttempt {
                success: false,
                already_published: false,
                retries,
                stderr: output.stderr,
            });
        }
        sleeper.sleep(retry.delay(retries));
        retries += 1;
    }
}

/// Итог проверки crates.io API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiOutcome {
    /// Реестр вернул список версий.
    Versions(Vec<semver::Version>),
    /// Крейт в реестре не найден.
    NotFound,
    /// Реестр недоступен (нет сети, ошибка API).
    Unavailable,
}

/// Источник данных об опубликованных версиях crates.io.
pub trait RegistryProbe: Send + Sync {
    /// Возвращает список версий крейта на crates.io.
    fn versions(&self, name: &str) -> ApiOutcome;
}

/// Реальный опрос crates.io API.
#[derive(Debug, Default, Clone, Copy)]
pub struct CratesIoProbe;

impl RegistryProbe for CratesIoProbe {
    fn versions(&self, name: &str) -> ApiOutcome {
        #[cfg(feature = "network")]
        {
            crates_io_versions(name)
        }
        #[cfg(not(feature = "network"))]
        {
            let _ = name;
            ApiOutcome::Unavailable
        }
    }
}

#[cfg(feature = "network")]
fn crates_io_versions(name: &str) -> ApiOutcome {
    #[derive(Deserialize)]
    struct Response {
        versions: Vec<VersionEntry>,
    }
    #[derive(Deserialize)]
    struct VersionEntry {
        num: String,
    }

    let client = match reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
    {
        Ok(client) => client,
        Err(_) => return ApiOutcome::Unavailable,
    };
    let url = format!("https://crates.io/api/v1/crates/{name}");
    let response = match client
        .get(&url)
        .header("User-Agent", "light-fw-cli (publish pre-flight)")
        .send()
    {
        Ok(response) => response,
        Err(_) => return ApiOutcome::Unavailable,
    };
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return ApiOutcome::NotFound;
    }
    let response = match response.error_for_status() {
        Ok(response) => response,
        Err(_) => return ApiOutcome::Unavailable,
    };
    match response.json::<Response>() {
        Ok(parsed) => ApiOutcome::Versions(
            parsed
                .versions
                .iter()
                .filter_map(|entry| semver::Version::parse(&entry.num).ok())
                .collect(),
        ),
        Err(_) => ApiOutcome::Unavailable,
    }
}

/// Локальные git-теги (используются как fallback к crates.io API).
pub fn git_tags(runner: &dyn CommandRunner, root: &Path) -> Option<BTreeSet<String>> {
    let output = runner
        .run(
            "git",
            &["tag".to_string(), "--list".to_string()],
            Some(root),
        )
        .ok()?;
    if !output.success() {
        return None;
    }
    Some(
        output
            .stdout
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect(),
    )
}

/// Результат проверки «версия уже опубликована».
#[derive(Debug, Clone)]
pub struct PublishedCheck {
    /// Уже опубликована ли версия.
    pub published: bool,
    /// Предупреждения (расхождение источников, недоступность API).
    pub warnings: Vec<String>,
}

/// Сверяет версию с crates.io API и git-тегами `v<version>`.
///
/// Оба источника «уже есть» → публикация запрещена; расхождение — warning и
/// продолжение; недоступность API — fallback на git-теги с warning.
pub fn check_already_published(
    probe: &dyn RegistryProbe,
    git_tags: Option<&BTreeSet<String>>,
    name: &str,
    version: &semver::Version,
) -> PublishedCheck {
    let api = match probe.versions(name) {
        ApiOutcome::Versions(versions) => Some(versions.contains(version)),
        ApiOutcome::NotFound => Some(false),
        ApiOutcome::Unavailable => None,
    };
    let git = git_tags.map(|tags| tags.contains(&format!("v{version}")));

    let mut warnings = Vec::new();
    let published = match (api, git) {
        (Some(true), Some(true)) => true,
        (Some(false), Some(false)) => false,
        (Some(api), Some(git)) if api != git => {
            warnings.push(format!(
                "crates.io API и git-теги расходятся для {name} v{version}; продолжаю"
            ));
            false
        }
        (None, Some(true)) => {
            warnings.push(format!(
                "crates.io API unreachable, falling back to git tags for {name}"
            ));
            true
        }
        (None, Some(false)) => {
            warnings.push(format!(
                "crates.io API unreachable, falling back to git tags for {name}"
            ));
            false
        }
        (Some(true), None) => {
            warnings.push(format!(
                "git-теги недоступны, доверяю crates.io API для {name}"
            ));
            true
        }
        (Some(false), None) => {
            warnings.push(format!(
                "git-теги недоступны, доверяю crates.io API для {name}"
            ));
            false
        }
        (None, None) => {
            warnings.push(format!(
                "crates.io API недоступен и git-теги недоступны для {name}; проверить не удалось"
            ));
            false
        }
        (Some(_), Some(_)) => unreachable!("случай равенства обработан выше"),
    };

    PublishedCheck {
        published,
        warnings,
    }
}
