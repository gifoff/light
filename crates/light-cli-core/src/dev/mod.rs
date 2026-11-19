//! Команда `light dev`: супервизор горячей перезагрузки приложения Light.
///
/// Запускает приложение (копию из `target/hot-run/`) как дочерний процесс,
/// следит за исходниками (`src/`, `templates/`, `Cargo.toml`). Две реакции:
/// * **HTML** (файлы `*.html`) → команда `RELOAD_TEMPLATE` через TCP-сервер
///   DevTools на порту 9229: приложение перечитывает HTML-шаблон с диска и
///   перерисовывает UI **без пересборки**. При неудачной отправки (app умер,
///   порт недоступен — os error 10061) **не триггерим полную пересборку**:
///   ждём до 3 с (poll каждые 200 мс), пока TCP снова станет доступен, и
///   повторяем `RELOAD_TEMPLATE`.
/// * **Source** (файлы `*.rs` / `Cargo.toml`) → полная пересборка + рестарт:
///   сохраняем состояние (`SAVE_STATE`), убиваем процесс, перезапускаем.
///
/// Race-защита: во время initial build (окно `is_starting`) изменения не
/// рассматриваются — процесс ещё не запущен, и `RELOAD_TEMPLATE` бесполезен.
///
/// Подмодули: [`args`] (константы и аргументы), [`watch`] (наблюдение за
/// файлами и портами), [`build`] (пересборка и управление процессом),
/// [`version`] (версионирование и кэш), [`run`] (главный цикл).

/// Константы и аргументы команды `light dev`.
///
/// Определяет порты, интервалы опроса, дебаунс-задержки и структуру
/// [`DevArgs`] с парсером [`parse_dev_command`].
pub mod args;
/// Пересборка и управление дочерним процессом.
///
/// Хеширование файлов, классификация изменений, машина состояний пересборки
/// и утилиты для работы с бинарником (`bin_name`, `fresh_bin_path`).
pub mod build;
/// Главный цикл супервизора `light dev`.
///
/// Функция [`run`] запускает дочерний процесс, устанавливает watcher на
/// файлы и обрабатывает события (перезагрузка HTML / полная пересборка).
pub mod run;
#[cfg(test)]
mod tests;
/// Версионирование и кэш_DEV_данных.
///
/// Проверка совместимости версий `light-fw-core`, чтение lock-файлов,
/// очистка кэша старых бинарников из `target/hot-run/`.
pub mod version;
/// Наблюдение за файлами и портами.
///
/// Определение корня пакета, построение списка путей для watch, применение
/// переменных окружения состояния/окна и запросы сохранения через DevTools.
pub mod watch;

pub use args::{DevArgs, parse_dev_command};
pub use build::{
    ChangeKind, RebuildOutcome, RebuildStateMachine, WatchAction, WatchState, classify_change,
    cleanup_run_dir, copy_fresh_to_run, file_hash, fresh_bin_path, handle_watch_event, kill_child,
    request_reload_template_with_retry, resolve_startup_completion, run_exe_path,
    wait_port_available,
};
pub use run::run;
pub use version::{
    build_info_path, check_cli_version_mismatch, clean_dev_cache, prune_stale_run_exes,
    read_fw_core_version_from_lock, read_fw_core_version_from_workspace_toml, save_build_info,
};
pub use watch::{
    detect_package_from_cwd, is_port_open, request_save_state, watch_paths, window_file_path,
};
