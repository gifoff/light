use std::path::PathBuf;

use dialoguer::Select;
use light_fw_cli_core::fetch::{RegistryClient, RemoteRegistry};
use light_fw_cli_core::search::{SearchOptions, SearchResult};
use light_fw_cli_core::search::{
    filter_by_prefix, is_already_installed, rank, truncate_description,
};
use serde::Serialize;

/// Аргументы команды `light search`.
#[derive(clap::Args)]
pub struct SearchArgs {
    /// Поисковый запрос (например, `datepicker`).
    pub query: String,

    /// Максимум результатов (по умолчанию 30).
    #[arg(long, default_value_t = 30)]
    pub limit: usize,

    /// Автоматически выбрать первый официальный компонент.
    #[arg(long)]
    pub yes: bool,

    /// Вывод в JSON-формате (для скриптов).
    #[arg(long)]
    pub json: bool,

    /// Каталог проекта (содержит `Cargo.toml`), для проверки `already_installed`.
    #[arg(long, default_value = ".")]
    pub path: PathBuf,
}

/// Корневой объект JSON-вывода `--json`.
#[derive(Serialize)]
struct JsonOutput {
    query: String,
    registry: String,
    results: Vec<JsonResult>,
}

/// Один результат в JSON-выводе.
#[derive(Serialize)]
struct JsonResult {
    name: String,
    version: String,
    description: String,
    official: bool,
    downloads: u64,
    link: String,
    already_installed: bool,
}

/// Ищет компоненты Light в настраиваемом реестре.
pub fn run(args: SearchArgs) -> anyhow::Result<i32> {
    // Загружаем конфиг реестра из проекта.
    let (client, config_warning) = RegistryClient::from_project(&args.path);
    if let Some(w) = config_warning {
        eprintln!("⚠️ {w}");
    }
    let config = client.config().clone();

    let options = SearchOptions {
        query: args.query.clone(),
        limit: args.limit,
        yes: args.yes,
        json: args.json,
    };

    let results = execute_search(&client, &options, &args.path)?;

    if args.json {
        return print_json(&results, &options.query, &config.url);
    }

    if results.is_empty() {
        println!("Ничего не найдено. Попробуйте `light search <другой запрос>`.");
        return Ok(0);
    }

    if args.yes {
        return auto_select_official(&client, &results, &args.path);
    }

    interactive_loop(&client, &results, &args.path)
}

/// Выполняет поиск: fetch → prefix filter → owners check → rank.
fn execute_search(
    client: &RegistryClient,
    options: &SearchOptions,
    project_root: &std::path::Path,
) -> anyhow::Result<Vec<SearchResult>> {
    let raw = client.search(&options.query, options.limit)?;
    let filtered = filter_by_prefix(&raw);

    // Параллельная проверка owners (2s timeout, failure → official=false).
    let mut results: Vec<SearchResult> = Vec::with_capacity(filtered.len());
    for item in &filtered {
        let owners = client.owners(&item.name).unwrap_or_default();
        let official = owners.iter().any(|o| o == "gifoff");
        let installed = is_already_installed(&item.name, project_root);
        let link = if item.url.is_empty() {
            format!(
                "{}/crates/{}",
                client.config().url.trim_end_matches('/'),
                item.name
            )
        } else {
            item.url.clone()
        };
        results.push(SearchResult {
            name: item.name.clone(),
            version: item.version.clone(),
            description: truncate_description(item.description.as_deref()),
            official,
            downloads: item.downloads,
            link,
            already_installed: installed,
        });
    }

    Ok(rank(&results, &options.query))
}

/// Печатает JSON-вывод.
fn print_json(results: &[SearchResult], query: &str, registry_url: &str) -> anyhow::Result<i32> {
    let json_out = JsonOutput {
        query: query.to_string(),
        registry: registry_url.to_string(),
        results: results
            .iter()
            .map(|r| JsonResult {
                name: r.name.clone(),
                version: r.version.clone(),
                description: r.description.clone(),
                official: r.official,
                downloads: r.downloads,
                link: r.link.clone(),
                already_installed: r.already_installed,
            })
            .collect(),
    };
    let out = serde_json::to_string_pretty(&json_out)?;
    println!("{out}");
    Ok(0)
}

/// Ненормативный режим: автоматически выбирает первый официальный компонент.
fn auto_select_official(
    client: &RegistryClient,
    results: &[SearchResult],
    project_root: &std::path::Path,
) -> anyhow::Result<i32> {
    use crate::cmd_search::cmd_search_internal::validate_and_report;

    let first_official = results.iter().find(|r| r.official);
    match first_official {
        Some(r) => {
            println!("✓ Выбран {name} (официальный)", name = r.name);
            println!("  Версия: {}", r.version);
            println!("  Описание: {}", r.description);

            match validate_and_report(client, r, project_root) {
                Ok(meta) => {
                    println!(
                        "✅ {} v{} прошёл валидацию. Готов к установке (Этап 2).",
                        meta.crate_name, meta.version
                    );
                    Ok(0)
                }
                Err(ChoiceResult::Fatal(msg)) => {
                    eprintln!("❌ {} отклонён: {msg}", r.name);
                    Ok(1)
                }
                Err(ChoiceResult::Declined | ChoiceResult::WarningAcknowledged) => {
                    eprintln!(
                        "❌ {}: требуется интерактивное подтверждение. Запустите без --yes.",
                        r.name
                    );
                    Ok(1)
                }
            }
        }
        None => {
            eprintln!("Не найден ни один официальный компонент для запроса.");
            Ok(1)
        }
    }
}

/// Интерактивный цикл выбора компонентов.
fn interactive_loop(
    client: &RegistryClient,
    initial_results: &[SearchResult],
    project_root: &std::path::Path,
) -> anyhow::Result<i32> {
    use crate::cmd_search::cmd_search_internal::validate_and_report;

    // Список ошибок по индексам (для повторного выбора ⛔).
    let mut error_cache: std::collections::HashMap<usize, String> =
        std::collections::HashMap::new();
    let current: Vec<SearchResult> = initial_results.to_vec();

    loop {
        // Строим строки для Select.
        let mut lines: Vec<String> = Vec::with_capacity(current.len());
        for (idx, r) in current.iter().enumerate() {
            let badge = if r.official { "✅" } else { "⬜" };
            let err_mark = if error_cache.contains_key(&idx) {
                " ⛔"
            } else {
                ""
            };
            let installed_mark = if r.already_installed {
                " (уже установлен)"
            } else {
                ""
            };
            lines.push(format!(
                "{badge} {} v{}{err_mark}{installed_mark}",
                r.name, r.version
            ));
        }

        // Добавляем "skip" как последний пункт.
        lines.push("(Пропустить — ничего не устанавливать)".into());

        let n = current.len();
        let selection = match Select::new()
            .with_prompt("Какой установить?")
            .items(&lines)
            .default(n) // default = skip
            .interact_opt()
        {
            Ok(Some(idx)) => idx,
            _ => {
                // Ctrl+C или Esc
                println!("Отменено.");
                return Ok(0);
            }
        };

        if selection == n {
            // Skip
            println!("Ничего не установлено.");
            return Ok(0);
        }

        let chosen = current[selection].clone();

        // Если был кэшированный error для этого индекса — показываем сразу.
        if let Some(err_msg) = error_cache.get(&selection) {
            eprintln!("⛔ Ошибка при выборе {name}:", name = chosen.name);
            eprintln!("   {err_msg}");
            continue;
        }

        println!("Загрузка {} v{}…", chosen.name, chosen.version);

        match validate_and_report(client, &chosen, project_root) {
            Ok(meta) => {
                println!(
                    "✓ Валидация пройдена: {} v{}",
                    meta.crate_name, meta.version
                );
                if !meta.provides_elements.is_empty() {
                    println!("  elements: {}", meta.provides_elements.join(", "));
                }
                if !meta.provides_attributes.is_empty() {
                    println!("  attributes: {}", meta.provides_attributes.join(", "));
                }
                if !meta.provides_directives.is_empty() {
                    println!("  directives: {}", meta.provides_directives.join(", "));
                }
                if !meta.requires_components.is_empty() {
                    println!("  requires: {}", meta.requires_components.join(", "));
                }
                println!("\nКомпонент готов к установке (Этап 2 — `light add`).");
                return Ok(0);
            }
            Err(ChoiceResult::WarningAcknowledged) => {
                // Пользователь подтвердил warning — продолжаем с этим же выбором.
                println!("✓ Принято. Компонент готов к установке (Этап 2 — `light add`).");
                return Ok(0);
            }
            Err(ChoiceResult::Declined) => {
                // N — возвращаемся к списку.
                println!("Выбор отклонён. Вернитесь к списку или пропустите.");
                continue;
            }
            Err(ChoiceResult::Fatal(msg)) => {
                error_cache.insert(selection, msg.clone());
                eprintln!("⛔ {msg}");
                continue;
            }
        }
    }
}

mod cmd_search_internal {
    use super::{ChoiceResult, RegistryClient, SearchResult};
    use dialoguer::Confirm;
    use light_fw_cli_core::fetch::RemoteRegistry;
    use light_fw_cli_core::metadata::ComponentMetadata;

    /// Валидирует выбранный компонент и обрабатывает warnings.
    pub fn validate_and_report(
        client: &RegistryClient,
        choice: &SearchResult,
        _project_root: &std::path::Path,
    ) -> anyhow::Result<ComponentMetadata, ChoiceResult> {
        let bytes = match client.download_crate(&choice.name, &choice.version) {
            Ok(b) => b,
            Err(e) => {
                return Err(ChoiceResult::Fatal(format!(
                    "Не удалось скачать {name}: {e}",
                    name = choice.name
                )));
            }
        };
        let toml_text = light_fw_cli_core::metadata::extract_cargo_toml(&bytes)
            .map_err(|e| ChoiceResult::Fatal(format!("Ошибка чтения архива: {e}")))?;
        let result = light_fw_cli_core::metadata::validate_metadata(&toml_text, &choice.name);
        match result {
            light_fw_cli_core::metadata::ValidationResult::Ok(meta) => Ok(meta),
            light_fw_cli_core::metadata::ValidationResult::Fatal(err) => {
                Err(ChoiceResult::Fatal(err.message().to_string()))
            }
            light_fw_cli_core::metadata::ValidationResult::Warning {
                metadata: _,
                warning,
            } => {
                let msg = warning.message();
                eprintln!("⚠️ {msg}");
                let confirmed = Confirm::new()
                    .with_prompt("Установить несмотря на предупреждение?")
                    .default(false)
                    .interact()
                    .map_err(|e| ChoiceResult::Fatal(format!("Ошибка ввода: {e}")))?;
                if confirmed {
                    Err(ChoiceResult::WarningAcknowledged)
                } else {
                    Err(ChoiceResult::Declined)
                }
            }
        }
    }
}

/// Результат обработки выбранного компонента.
pub enum ChoiceResult {
    /// Фатальная ошибка — показываем и возвращаемся к списку.
    Fatal(String),
    /// Предупреждение подтверждено пользователем.
    WarningAcknowledged,
    /// Предупреждение отклонено — возвращаемся к списку.
    Declined,
}
