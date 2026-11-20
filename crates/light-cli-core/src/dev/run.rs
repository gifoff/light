use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(unix)]
use std::os::unix::process::ExitStatusExt as _;
#[cfg(windows)]
use std::os::windows::process::ExitStatusExt as _;

use notify::{Config, PollWatcher, RecursiveMode, Watcher as _};
use sha2::Digest as _;

use super::args::{
    DEVTOOLS_PORT, DevArgs, HTML_DEBOUNCE, POLL_INTERVAL, RS_DEBOUNCE, STARTUP_IGNORE,
};
use super::build::{
    ChangeKind, RebuildOutcome, RebuildStateMachine, WatchAction, WatchState, bin_name,
    classify_change, cleanup_run_dir, copy_fresh_to_run, file_hash, fresh_bin_path,
    handle_watch_event, kill_child, request_reload_template_with_retry, resolve_startup_completion,
};
use super::version::{
    build_info_path, check_cli_version_mismatch, clean_dev_cache, prune_stale_run_exes,
    read_fw_core_version_from_lock, read_fw_core_version_from_workspace_toml, same_minor,
    save_build_info,
};
use super::watch::{
    apply_state_env, apply_window_env, detect_package_from_cwd, is_port_open, request_save_all,
    request_save_state, resolve_package_root, resolve_workspace_root, state_file_path, watch_paths,
    window_file_path,
};

/// Точка входа команды `light dev`: запускает дочерний процесс и цикл
/// наблюдения. Возвращает код завершения приложения (не `anyhow`, чтобы ядро
/// CLI не тягло зависимость, которой нет в его манифесте).
///
/// Не вызывается автоматически из других частей библиотеки — только из
/// бинарного `light-fw-cli` (см. `cmd_dev` в `light-cli`).
pub fn run(args: &DevArgs) -> std::result::Result<i32, std::io::Error> {
    // ТЗ 0.16.0: `--agent` — автоматически создаём session recorder ДО запуска
    // приложения и закрываем его (сгенерировав summary.md) при выходе/kill.
    // Обёртка гарантирует завершение сессии на любом пути выхода (в т.ч. panic
    // через `?`), потому что `run_inner` вызывается здесь единственно.
    let cwd = std::env::current_dir()?;
    let agent_session = if args.agent {
        match crate::session::start_session(&cwd) {
            Ok(id) => {
                eprintln!("light dev: agent session started → {:?}", id);
                Some(id)
            }
            Err(e) => {
                eprintln!("light dev: не удалось создать agent session: {e}");
                None
            }
        }
    } else {
        None
    };
    let code = run_inner(args)?;
    if let Some(id) = agent_session
        && let Ok(summary) = crate::session::stop_session(&cwd, &id)
    {
        eprintln!("light dev: agent session summary → {:?}", summary);
    }
    Ok(code)
}

fn run_inner(args: &DevArgs) -> std::result::Result<i32, std::io::Error> {
    let cwd = std::env::current_dir()?;
    // Имя пакета: явное значение из CLI или автоопределение по cwd.
    let resolved = args
        .package
        .clone()
        .or_else(|| detect_package_from_cwd(&cwd));
    let Some(pkg_name) = resolved else {
        eprintln!(
            "light dev: cannot determine package; use `light dev <name>` or run inside crate directory"
        );
        return Ok(1);
    };
    // Корень пакета: если передан `package`, ищем его через `cargo metadata`,
    // иначе — текущий каталог. Именно этот корень передаётся в `watch_paths`.
    let package_root = resolve_package_root(args.package.as_deref(), &cwd);
    // ТЗ 0.14.2-B-fix: для workspace-членов артефакты cargo кладутся в
    // workspace_root/target, а не в package_root/target. Берём workspace_root
    // (если доступен) как корень для target/hot-build и target/hot-run;
    // fallback — package_root (одиночный крейт без workspace).
    let target_root = resolve_workspace_root(&cwd).unwrap_or_else(|| {
        eprintln!(
            "light dev: `cargo metadata` недоступен — target_dir из package_root {:?}",
            package_root
        );
        package_root.clone()
    });
    eprintln!("light dev: workspace_root = {:?}", target_root);
    // ТЗ 0.14.2-B-fix2: две папки — сборка в `target/hot-build` (cargo её
    // никогда не блокирует), запуск из копии в `target/hot-run/<bin>-<ts>.exe`
    // (Windows не даёт перезаписать запущенный .exe, но копия свободна).
    let build_target = target_root.join("target").join("hot-build");
    let run_dir = target_root.join("target").join("hot-run");
    let bin = bin_name(&pkg_name);
    eprintln!(
        "light dev: build_target = {:?}, run_dir = {:?}",
        build_target, run_dir
    );

    // === TZ 0.14.4: управление кэшем до Phase A (warm-up build) ===
    // Порядок: (a) `--clean` → полный сброс; (b) auto-invalidation по версии
    // light-fw-core (build-info.json); (c) age-привка hot-run; (d) warning
    // при рассинхроне CLI/core.
    if args.clean {
        let removed = clean_dev_cache(&target_root, Some(pkg_name.as_str()));
        eprintln!("light dev: cleaned cache (hot-build, hot-run, state, deps, build-info)");
        let _ = removed.len();
    }
    // Auto-invalidation: сравниваем версию light-fw-core (первоисточник —
    // [workspace.package].version из Cargo.toml, fallback — Cargo.lock) с тем,
    // что зафиксировано в build-info.json. При смене — удаляем target/hot-build
    // ОДИН РАЗ, чтобы fresh-артефакт пересобрался с новой версией.
    let lock_path = target_root.join("Cargo.lock");
    let toml_path = target_root.join("Cargo.toml");
    let info_path = build_info_path(&target_root);
    let current_version = read_fw_core_version_from_workspace_toml(&toml_path)
        .or_else(|| read_fw_core_version_from_lock(&lock_path));
    if let Some(current_version) = &current_version {
        let saved = std::fs::read_to_string(&info_path)
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            .and_then(|v| {
                v.get("light-fw-core-version")
                    .and_then(|x| x.as_str())
                    .map(String::from)
            });
        // Сравниваем major.minor: `"0.15.3"` vs `"0.15"` → same minor, skip;
        // `"0.14"` vs `"0.15"` → different minor, invalidate.
        if !same_minor(current_version, saved.as_deref().unwrap_or("")) {
            let hot_build = target_root.join("target").join("hot-build");
            if hot_build.exists() {
                if std::fs::remove_dir_all(&hot_build).is_ok() {
                    eprintln!(
                        "light dev: framework version changed ({} → {}) → hot-build cache invalidated",
                        saved.as_deref().unwrap_or("?"),
                        current_version
                    );
                } else {
                    eprintln!("light dev: не удалось удалить target/hot-build после смены версии");
                }
            } else {
                eprintln!(
                    "light dev: framework version changed ({} → {}) → hot-build cache invalidated",
                    saved.as_deref().unwrap_or("?"),
                    current_version
                );
            }
            // Записываем текущую версию, чтобы следующая сборка была «чистой».
            let lock_hash = std::fs::read_to_string(&lock_path)
                .ok()
                .map(|s| hex::encode(sha2::Sha256::digest(s.as_bytes())))
                .unwrap_or_default();
            save_build_info(&info_path, current_version, &lock_hash);
        }
    } else {
        eprintln!(
            "light dev: light-fw-core не найден в Cargo.toml/Cargo.lock — skip auto-invalidation"
        );
    }
    // Age-привка hot-run: на старте дочернего процесса ещё нет, но старые
    // артефакты (от прошлого запуска) уже лежат в hot-run. Удаляем их, пока
    // они не мешают. Возвращаем число удалённых.
    let pruned = prune_stale_run_exes(&run_dir, Path::new("/nonexistent"));
    if pruned > 0 {
        eprintln!("light dev: pruned {pruned} stale executables from hot-run");
    }
    // Предупреждение при рассинхроне между установленным CLI и версией
    // light-fw-core, зафиксированной в проекте.
    if let Some((installed, project)) =
        check_cli_version_mismatch(env!("CARGO_PKG_VERSION"), &toml_path, &lock_path)
    {
        eprintln!(
            "⚠ light dev: installed CLI version {installed} differs from project {project}. Run `cargo install --path crates/light`"
        );
    }

    // === ФАЗА A: warm-up build в target/hot-build ===
    // Блокирующий `cargo build` в отдельный target-dir. При ошибке — сразу
    // exit 1 (старого процесса нет, «оставлять живым» нечего).
    {
        // ТЗ 0.16.1, задача 4: статус на stdout до и после сборки, чтобы
        // агент не думал, что «зависло» (раньше выходил только stderr).
        println!("⏳ Сборка (cargo build) — target {:?}…", build_target);
        let mut cmd = Command::new("cargo");
        cmd.arg("build");
        cmd.args(["-p", &pkg_name, "--features", "light-fw/dev-mode"]);
        cmd.args(["--target-dir", &build_target.to_string_lossy()]);
        cmd.args(&args.extra_cargo_args);
        cmd.stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());

        // ТЗ 0.17.4: спиннер — каждые 2 с обновляем elapsed-время в stderr,
        // чтобы пользователь видел прогресс длительной сборки.
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let spinner = {
            use std::io::Write as _;
            use std::sync::atomic::Ordering;
            let stop2 = stop.clone();
            thread::spawn(move || {
                let start = Instant::now();
                loop {
                    thread::sleep(Duration::from_secs(2));
                    if stop2.load(Ordering::Relaxed) {
                        break;
                    }
                    let elapsed = start.elapsed().as_secs();
                    let mut err = std::io::stderr().lock();
                    let _ = write!(err, "\r⏳ Сборка (cargo build) ({}с)", elapsed);
                    let _ = err.flush();
                }
                let mut err = std::io::stderr().lock();
                let _ = write!(err, "\r\x1b[2K");
                let _ = err.flush();
            })
        };

        let status = match cmd.spawn() {
            Ok(mut child) => {
                let s = child.wait();
                stop.store(true, std::sync::atomic::Ordering::Relaxed);
                let _ = spinner.join();
                s
            }
            Err(e) => {
                stop.store(true, std::sync::atomic::Ordering::Relaxed);
                let _ = spinner.join();
                Err(e)
            }
        };
        match status {
            Ok(s) if s.success() => println!("✅ Сборка завершена, запускаю приложение…"),
            Ok(s) => {
                eprintln!(
                    "light dev: warm-up build FAILED (exit {:?}) — exit 1",
                    s.code()
                );
                return Ok(1);
            }
            Err(e) => {
                eprintln!("light dev: не удалось запустить `cargo build`: {e} — exit 1");
                return Ok(1);
            }
        }
        // ТЗ 0.14.2-B-fix 4: после успешного build проверим, что бинарник
        // действительно лежит там, где мы его ищем. Если нет — путь расхожился.
        let fresh_bin = fresh_bin_path(&build_target, &bin);
        if std::fs::metadata(&fresh_bin).is_err() {
            eprintln!(
                "light dev: BUG — build succeeded but binary not at {:?}. \
                 Search in {:?}...",
                fresh_bin,
                build_target.join("debug")
            );
            return Ok(1);
        }
    }

    // === ФАЗА B: копия в target/hot-run + spawn ===
    // Запускаем из копии, а не из hot-build: следующая сборка перезапишет
    // fresh-бинарник без os error 5 (запущен процесс с другой, свободной
    // копии).
    let first_run_exe = match copy_fresh_to_run(&build_target, &run_dir, &bin) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("light dev: не удалось скопировать бинарник: {e} — exit 1");
            return Ok(1);
        }
    };
    eprintln!(
        "light dev: spawning {:?} (build {:?})",
        first_run_exe,
        fresh_bin_path(&build_target, &bin)
    );
    // ТЗ 0.14.3-fix7: cold start не применяет ни state, ни geometry (см.
    // `resumed`/`create_window` в core). Чтобы они гарантированно не подтянули
    // устаревшие значения из прошлой сессии, удаляем оба файла до запуска
    // первого warm-up бинарника. (При user-close supervisor их тоже удаляет —
    // это двойная страховка на случай, если прошлый процесс завершился грубо.)
    for stale in [
        state_file_path(&package_root),
        window_file_path(&package_root),
    ] {
        if std::fs::remove_file(&stale).is_ok() {
            eprintln!("light dev: cold start — cleared stale {:?}", stale);
        }
    }
    let mut child_cmd = Command::new(&first_run_exe);
    child_cmd.stdin(Stdio::inherit());
    child_cmd.stdout(Stdio::inherit());
    child_cmd.stderr(Stdio::inherit());
    apply_state_env(&mut child_cmd, &package_root);
    apply_window_env(&mut child_cmd, &package_root);
    // ТЗ 0.14.3-fix5 (откат fix4): на cold start state.json НЕ применяется —
    // дефолты из нового кода имеют приоритет над устаревшим state. При swap
    // состояние сохраняется (см. блок ниже): LIGHT_RESUME_FROM_SWAP=1 +
    // LIGHT_PRESERVE_STATE=1, поэтому count переживает .rs change, а
    // вычисляемые значения (методы, например status) пересчитываются из нового
    // кода. --preserve-state оставлен как алиас (те же эффекты, что и default).
    let child = match child_cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            eprintln!(
                "light dev: не удалось запустить {:?}: {e} — exit 1",
                first_run_exe
            );
            return Ok(1);
        }
    };

    // ТЗ 0.14.2-B: общая обёртка для thread-safe доступа (main loop + rebuild
    // thread). `child_arc` — приложение, которое сейчас работает (или будет
    // после swap); `rebuild_arc` — текущая фоновая сборка (или None, если её
    // нет). Rebuild запускаем из main-потока и просто опрашиваем `try_wait()`
    // в главном цикле — отдельный тред не нужен (Cargo сам параллелит).
    let child_arc = Arc::new(Mutex::new(child));
    let rebuild_arc: Arc<Mutex<Option<Child>>> = Arc::new(Mutex::new(None));

    // State-machine: решение о реакции на результат фоновой сборки. Держим
    // в main-потоке (swap выполняется там); чистый, без side effects.
    let mut sm = RebuildStateMachine::default();
    // ТЗ 0.14.3-fix7: kill_child (kill + wait) синхронно потребляет exit
    // убитого swap-процесса, так что Ev::Exited в главном цикле получает
    // только события от «живых» детей (user-close или crash). Флаг pending_swap
    // больше не нужен — все exit 0 в Ev::Exited = user-close → удалить файлы.
    // Канал на перезапуск от наблюдателя. Несёт тип изменения: Html →
    // RELOAD_TEMPLATE (без убийства), Source → background rebuild + swap.
    let (restart_tx, restart_rx) = mpsc::channel::<(Instant, ChangeKind)>();
    // Флаг завершения: общий между главным потоком, наблюдателем и
    // rebuild-тредом. Атомарный (в отличие от mpsc::Receiver) — легко
    // шараится между несколькими тредами без clone() канала.
    let done_flag = Arc::new(AtomicBool::new(false));
    // Состояние наблюдателя (is_starting/pending_change) и канал на его
    // обновление. Главный поток решает, когда закрывать окно игнорирования:
    // как только порт 9229 открылся или прошёл STARTUP_IGNORE.
    let (state_tx, state_rx) = mpsc::channel::<WatchState>();
    let _ = state_tx.send(WatchState::starting());

    let watch_paths_copy = watch_paths(&package_root);
    eprintln!("light dev: watching {:?}", watch_paths_copy);
    let verbose = args.verbose;
    let done_flag_watch = done_flag.clone();
    let watcher = thread::spawn(move || {
        if watch_paths_copy.is_empty() {
            eprintln!("light dev: нет путей для наблюдения (нет src/, templates/, Cargo.toml)");
            return;
        }
        let (tx, rx) = mpsc::channel::<notify::Result<notify::Event>>();
        // PollWatcher: периодическое опросование файловой системы. В отличие от
        // нативного `recommended_watcher`, работает на UNC- и сетевых дисках,
        // где нативный механизм события просто не ловит.
        let config = Config::default().with_poll_interval(POLL_INTERVAL);
        let mut watcher = match PollWatcher::new(
            move |res| {
                let _ = tx.send(res);
            },
            config,
        ) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("light dev: не удалось создать файловый наблюдатель: {e}");
                return;
            }
        };
        for path in &watch_paths_copy {
            if let Err(e) = watcher.watch(path, RecursiveMode::NonRecursive) {
                eprintln!("light dev: не удалось следить за {}: {e}", path.display());
            }
        }
        // Начальное состояние: is_starting = true. Watcher сам решает, когда
        // закрывать окно: как только порт DevTools открылся либо прошёл
        // дедлайн STARTUP_IGNORE (защита от вечной блокировки при зависшей
        // сборке). Principal-поток лишь сбрасывает state в `starting()` при
        // перезапуске процесса.
        let mut state = match state_rx.recv() {
            Ok(s) => s,
            Err(_) => return,
        };
        // Момент запуска приложения: используется для дедлайна STARTUP_IGNORE.
        // Пересбрасывается при каждом новом запуске дочернего процесса (главный
        // поток шлёт `WatchState::starting()` через state_rx).
        let mut start_at = Instant::now();
        // Сбрасывает дебаунс; актуально только вне окна `is_starting`.
        // Отдельный таймер для каждого типа изменения (Html vs Source).
        let mut last_html: Option<Instant> = None;
        let mut last_rs: Option<Instant> = None;
        // Дедупликация по хэшу содержимого: один «логический» save генерирует
        // несколько событий notify (Modify + Modify(Data) + Modify(Metadata)).
        // Ключевой порядок (ТЗ 0.14.2-A-fix4): debounce СНАЧАЛА (файл успевает
        // дописаться), затем hash-фильтр (повторные события того же
        // содержимого отброшены), затем handle_watch_event.
        let mut last_hashes: std::collections::HashMap<PathBuf, u64> =
            std::collections::HashMap::new();
        // Накопитель путей, ожидающих debounce. На событие notify мы НЕ
        // хэшируем сразу (файл может быть в середине записи); только сохраняем
        // путь. Когда debounce срабатывает (100/500 мс тишины), проходим по
        // pending_debounce, хэшируем и дедуплицируем.
        let mut pending_debounce: std::collections::VecDeque<PathBuf> =
            std::collections::VecDeque::new();
        // Приоритет pending-флагов (Source > Html): если оба были накоплены,
        // применяем source (пересборка важнее reload).
        let apply_pending = |state: &mut WatchState,
                             tx: &mpsc::Sender<(Instant, ChangeKind)>|
         -> Option<ChangeKind> {
            let kind = resolve_startup_completion(state)?;
            let _ = tx.send((Instant::now(), kind));
            state.clear_pending();
            Some(kind)
        };
        loop {
            // 1) Завершение от основного потока.
            if done_flag_watch.load(Ordering::Relaxed) {
                break;
            }
            // 2) Обновление состояния от главного потока (при перезапуске
            //    приложение сбрасывает state в `starting()`).
            if let Ok(new_state) = state_rx.try_recv() {
                state = new_state;
                start_at = Instant::now();
                last_html = None;
                last_rs = None;
                // Сбрасываем накопленные пути: предыдущий запуск дочернего
                // процесса завершился, и все «недочитанные» изменения теряются.
                pending_debounce.clear();
                continue;
            }
            // 3) Окно игнорирования: порт 9229 открылся или дедлайн прошёл.
            //    Применяем накопленный pending (если есть) как ReloadHtml /
            //    Restart — без эвристики «обнаружены изменения».
            if state.is_starting {
                let port_open = is_port_open(DEVTOOLS_PORT);
                let deadline_reached = start_at.elapsed() >= STARTUP_IGNORE;
                if port_open || deadline_reached {
                    let reason = if port_open {
                        "порт DevTools открыт"
                    } else {
                        &format!("дедлайн {:?} истёк", STARTUP_IGNORE)
                    };
                    eprintln!(
                        "light dev: окно игнорирования снято ({reason}), \
                         очередь pending (HTML={}, Source={})",
                        state.pending_html, state.pending_source
                    );
                    if let Some(kind) = apply_pending(&mut state, &restart_tx) {
                        match kind {
                            ChangeKind::Html => last_html = Some(Instant::now()),
                            ChangeKind::Source => last_rs = Some(Instant::now()),
                        }
                    }
                    state.is_starting = false;
                }
                continue;
            }
            // 3) События наблюдателя.
            match rx.recv_timeout(Duration::from_millis(50)) {
                Ok(Ok(event)) => {
                    // На каждое notify-событие мы НЕ хэшируем сразу: файл может
                    // быть в середине записи (двухэтапный append + flush), и
                    // хэш поймает неполное содержимое. Вместо этого:
                    //  1) фильтруем по watch_paths;
                    //  2) классифицируем (Html / Source / None) для лога;
                    //  3) кладём путь в pending_debounce (deduplication по пути,
                    //     чтобы серия событий одного save не раздувала очередь);
                    //  4) поднимаем таймер debounce (Html / Source).
                    //
                    // Хэширование + дедупликация happen в timeout-ветке, ПОСЛЕ
                    // debounce — к тому моменту файл уже полностью записан.
                    for path in &event.paths {
                        if !watch_paths_copy.iter().any(|w| path.starts_with(w)) {
                            continue;
                        }
                        // Лог по ТЗ 3.1: сразу, до debounce.
                        let kind_log = classify_change(path);
                        if verbose {
                            eprintln!(
                                "light dev: notify event path={path:?} kind={kind_log:?} \
                                 (queued for debounce)"
                            );
                        }
                        // Deduplicate by path within the queue: если тот же путь
                        // уже в pending_debounce, не добавляем повторно (серия
                        // событий одного save генерирует 2-3 notify на один путь).
                        if !pending_debounce.contains(path) {
                            pending_debounce.push_back(path.clone());
                        }
                    }
                    // Поднимаем таймеры debounce: если любое из релевантных
                    // изменений пришло, debounce обнуляется и начинает отсчёт
                    // заново (стандартный trailing-edge debounce).
                    if event
                        .paths
                        .iter()
                        .any(|p| watch_paths_copy.iter().any(|w| p.starts_with(w)))
                    {
                        let has_html = event.paths.iter().any(|p| {
                            watch_paths_copy.iter().any(|w| p.starts_with(w))
                                && matches!(classify_change(p), Some(ChangeKind::Html))
                        });
                        let has_src = event.paths.iter().any(|p| {
                            watch_paths_copy.iter().any(|w| p.starts_with(w))
                                && matches!(classify_change(p), Some(ChangeKind::Source))
                        });
                        if has_html {
                            last_html = Some(Instant::now());
                        }
                        if has_src {
                            last_rs = Some(Instant::now());
                        }
                    }
                }
                Ok(Err(e)) => {
                    eprintln!("light dev: ошибка наблюдателя: {e}");
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    // Дебаунс + hash-дедупликация ПОСЛЕ debounce.
                    //
                    // Порядок (ТЗ 0.14.2-A-fix4):
                    //   1) debounce: дождались тишины (HTML_DEBOUNCE / RS_DEBOUNCE);
                    //   2) hash: хэшируем содержимое каждого пути из pending_debounce
                    //      (файл уже полностью записан, потому что прошло ≥ 100/500 мс);
                    //   3) dedup: отбрасываем пути, чей хэш == last_hashes[path]
                    //      (повторные события того же содержимого);
                    //   4) dispatch: берём ПЕРВЫЙ «чистый» путь, классифицируем и
                    //      передаём в handle_watch_event.
                    //
                    // Важно: если pending_debounce пусто — ничего не делаем, даже
                    // если таймеры debounce истекли (таймеры поднимаются только
                    // при добавлении путей, так что это ситуация невозможна, но
                    // защита на случай race с respawn).
                    if !state.is_starting && !state.has_pending() {
                        let html_fired =
                            matches!(last_html, Some(t) if t.elapsed() >= HTML_DEBOUNCE);
                        let rs_fired = matches!(last_rs, Some(t) if t.elapsed() >= RS_DEBOUNCE);
                        if html_fired || rs_fired {
                            if html_fired {
                                last_html = None;
                            }
                            if rs_fired {
                                last_rs = None;
                            }
                            if verbose {
                                eprintln!(
                                    "light dev: debounce fired, {} paths pending",
                                    pending_debounce.len()
                                );
                            }
                            // Проходим по pending_debounce: хэшируем и дедуплицируем.
                            // Файл уже полностью записан (debounce отработал).
                            let mut clean_events: std::collections::VecDeque<PathBuf> =
                                std::collections::VecDeque::new();
                            while let Some(path) = pending_debounce.pop_front() {
                                match file_hash(&path, verbose) {
                                    Some(hash) => {
                                        // Сравниваем с последним известным хэшем
                                        // этого пути:
                                        //  - совпадает → повторное событие того же
                                        //    содержимого → игнорируем (по ТЗ:
                                        //    "duplicate notify event, ignored");
                                        //  - отличается → настоящее изменение →
                                        //    обновляем last_hashes и кладём в
                                        //    clean_events.
                                        match last_hashes.get(&path) {
                                            Some(&last) if last == hash => {
                                                if verbose {
                                                    eprintln!(
                                                        "light dev: duplicate notify event for \
                                                         {path:?} (hash unchanged) — ignored"
                                                    );
                                                }
                                            }
                                            _ => {
                                                last_hashes.insert(path.clone(), hash);
                                                clean_events.push_back(path);
                                            }
                                        }
                                    }
                                    // Файл недоступен (удалён, ещё пишется после
                                    // 3 попыток чтения): НЕ дедуплицируем —
                                    // обрабатываем как изменение (консервативно).
                                    None => {
                                        if verbose {
                                            eprintln!(
                                                "light dev: file_hash returned None for \
                                                 {path:?} (treated as real change, no dedup)"
                                            );
                                        }
                                        clean_events.push_back(path);
                                    }
                                }
                            }
                            if verbose {
                                eprintln!(
                                    "light dev: {} clean events after hash-dedup",
                                    clean_events.len()
                                );
                            }
                            // Берём ПЕРВЫЙ чистый путь, классифицируем и
                            // передаём в handle_watch_event (по ТЗ 1.3).
                            if let Some(first) = clean_events.pop_front() {
                                let kind = classify_change(&first);
                                // Лог по ТЗ 3.4: при передаче в handle.
                                eprintln!("light dev: dispatching {kind:?} change for {first:?}");
                                match handle_watch_event(&mut state, kind) {
                                    WatchAction::None => {
                                        // Изменение во время сборки: просто
                                        // запомнили (pending_флаги подняты внутри
                                        // handle_watch_event).
                                    }
                                    WatchAction::ApplyPending => {
                                        if let Some(akind) = apply_pending(&mut state, &restart_tx)
                                        {
                                            eprintln!(
                                                "light dev: применяю pending (kind={akind:?})"
                                            );
                                            match akind {
                                                ChangeKind::Html => {
                                                    last_html = Some(Instant::now());
                                                }
                                                ChangeKind::Source => {
                                                    last_rs = Some(Instant::now());
                                                }
                                            }
                                        }
                                    }
                                    WatchAction::ReloadHtml => {
                                        eprintln!(
                                            "light dev: HTML change → RELOAD_TEMPLATE \
                                             (без пересборки)"
                                        );
                                        let _ = restart_tx.send((Instant::now(), ChangeKind::Html));
                                        last_html = Some(Instant::now());
                                    }
                                    WatchAction::Restart => {
                                        eprintln!(
                                            "light dev: Source change → RESTART \
                                             (пересборка)"
                                        );
                                        let _ =
                                            restart_tx.send((Instant::now(), ChangeKind::Source));
                                        last_rs = Some(Instant::now());
                                    }
                                }
                            } else {
                                if verbose {
                                    eprintln!(
                                        "light dev: все события отфильтрованы hash-дедупом — \
                                         action не требуется"
                                    );
                                }
                            }
                        }
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
    });

    // ТЗ 0.18.0: web UI DevTools — запуск делегируется бинарному слою, так
    // как `light-fw-cli-core` не может зависеть от `light-fw-devtools-web`
    // (цикл: devtools-web → cli-core). Бинарный слой (`light`) сам вызывает
    // `light_fw_devtools_web::server::serve()` перед `run_dev()`.

    loop {
        // Дедлайн защиты от вечной блокировки: если порт 9229 так и не
        // открылся за STARTUP_IGNORE (сборка зависла), watcher сам закроет
        // окно игнорирования (он владеет pending-флагами и знает, что
        // применить). Главный поток здесь ничего не решает.
        //
        // Внутренний цикл опроса: выходим в наружный цикл по первому событию
        // (restart от наблюдателя, завершение фоновой сборки либо выход
        // дочернего процесса).
        //
        // ТЗ 0.14.2-B: при .rs change мы НЕ убиваем старый процесс сразу —
        // запускаем `cargo build` в `target/hot-build` (рекурсивно, но не
        // блокируй watcher: он живёт в своём треде), а когда сборка закончится:
        //   - exit 0  → SAVE_STATE, copy fresh → hot-run, kill old, spawn new
        //   - exit != 0 → старый продолжает работать; stderr cargo уже на консоли.
        enum Ev {
            Restart(ChangeKind),
            RebuildDone(i32),
            Exited(std::process::ExitStatus),
        }
        let ev = loop {
            if let Ok((_ts, kind)) = restart_rx.try_recv() {
                break Ev::Restart(kind);
            }
            // Завершение фоновой сборки: poll rebuild-child в основном потоке.
            // `lock().ok()` — при poisoned mutex (краш в другом треде) просто
            // пропускаем опрос; main продолжит работать с app-процессом.
            let rebuild_done = rebuild_arc.lock().ok().and_then(|mut g| match g.as_mut() {
                Some(c) => c.try_wait().ok().flatten(),
                None => None,
            });
            if let Some(status) = rebuild_done {
                let code = status.code().unwrap_or(1);
                if let Ok(mut g) = rebuild_arc.lock() {
                    *g = None;
                }
                break Ev::RebuildDone(code);
            }
            // Опрос дочернего процесса: если mutex poisoned — пропускаем tick;
            // если `try_wait` вернул Err — тоже выходим с exit code 1 (не
            // продолжаем работать с процессом, который не удалось опросить).
            let child_exit = match child_arc.lock() {
                Ok(mut g) => match g.try_wait() {
                    Ok(Some(s)) => Some(Ok(s)),
                    Ok(None) => None, // ещё жив
                    Err(e) => Some(Err(e)),
                },
                Err(_) => None,
            };
            match child_exit {
                Some(Ok(s)) => break Ev::Exited(s),
                Some(Err(e)) => {
                    eprintln!("light dev: ошибка ожидания дочернего процесса: {e}");
                    break Ev::Exited(ExitStatus::from_raw(1));
                }
                None => {
                    // Mutex poisoned или process ещё жив.
                    if child_arc.lock().is_err() {
                        eprintln!("light dev: child_arc poisoned (пропуск опроса)");
                    }
                }
            }
            thread::sleep(Duration::from_millis(50));
        };
        match ev {
            Ev::Exited(status) => {
                done_flag.store(true, Ordering::Relaxed);
                let _ = watcher.join();
                // Web UI DevTools останавливается бинарным слоем (light),
                // который владеет WebServer-инстансом.
                // Если rebuild ещё идёт — тоже убиваем (Ctrl+C-сценарий).
                if let Ok(mut g) = rebuild_arc.lock()
                    && let Some(mut r) = g.take()
                {
                    let _ = r.kill();
                    let _ = r.wait();
                }
                let code = status.code().unwrap_or(1);
                // ТЗ 0.14.3-fix7: Ev::Exited получает только события от
                // «живых» детей (kill_child синхронно потребляет exit убитого
                // swap-процесса). Exit 0 → user-close: стираем state + geometry.
                // Exit != 0 → crash: файлы не трогаем.
                if code == 0 {
                    for stale in [
                        state_file_path(&package_root),
                        window_file_path(&package_root),
                    ] {
                        if std::fs::remove_file(&stale).is_ok() {
                            eprintln!("light dev: removed {:?}", stale);
                        }
                    }
                    eprintln!("light dev: user close — state + geometry removed");
                    eprintln!(
                        "light dev: app exited cleanly (window closed by user or session end)"
                    );
                } else {
                    eprintln!("light dev: app crashed (exit code {code})");
                }
                return Ok(code);
            }
            Ev::RebuildDone(exit_code) => {
                // Фоновая сборка закончилась. State-machine решает: swap или keep-old.
                let outcome = sm.poll_outcome(exit_code);
                if let RebuildOutcome::Success = outcome {
                    // === SWAP: SAVE_STATE → copy fresh → kill old → spawn new ===
                    // Порядок (ТЗ 0.14.2-B-fix2 3): copy работает и на живом
                    // exe (это отдельный файл), kill — после copy, но до spawn
                    // нового (чтобы не было двух видимых окон).
                    let took = sm
                        .started_at
                        .map(|t| Instant::now().saturating_duration_since(t))
                        .unwrap_or_default();
                    eprintln!("light dev: rebuild succeeded (took {took:?}), swapping…");
                    if let Err(e) = request_save_state(DEVTOOLS_PORT) {
                        eprintln!(
                            "light dev: не удалось отправить SAVE_STATE перед swap: {e} \
                             (состояние может быть не сохранено)"
                        );
                    } else {
                        eprintln!(
                            "light dev: state saved to {:?}",
                            state_file_path(&package_root)
                        );
                    }
                    // Короткая пауза: даём UI-потоку время обработать команду
                    // и записать файл, прежде чем уберём процесс.
                    thread::sleep(Duration::from_millis(200));
                    // a) Копируем свежий бинарник из hot-build в hot-run.
                    //    Никто не держит fresh-бинарник (запущена старая
                    //    копия в hot-run), run_exe новое имя — можно писать.
                    let new_run_exe = match copy_fresh_to_run(&build_target, &run_dir, &bin) {
                        Ok(p) => p,
                        Err(e) => {
                            eprintln!(
                                "light dev: не удалось скопировать свежий бинарник: {e} \
                                 (old остаётся)"
                            );
                            continue;
                        }
                    };
                    eprintln!("light dev: copied fresh build to {:?}", new_run_exe);
                    // ТЗ 0.15.1-fix3: graceful save перед kill — отправляем
                    // SAVE_ALL и ожидаем ok (до 500 мс). UI-поток синхронно
                    // пишет state.json + window.json. Если таймаут/ошибка —
                    // продолжаем как раньше (TerminateProcess без graceful).
                    request_save_all(DEVTOOLS_PORT);
                    // b) Убиваем старый app (через MutexGuard; `kill_child`
                    //    принимает `&mut Child`).
                    let old_pid = child_arc.lock().map(|g| g.id()).unwrap_or(0);
                    if let Ok(mut g) = child_arc.lock() {
                        kill_child(&mut g);
                    } else {
                        eprintln!("light dev: child_arc poisoned — не смогла убить old");
                    }
                    // c) Spawn нового из копии в hot-run.
                    let mut new_cmd = Command::new(&new_run_exe);
                    new_cmd.stdin(Stdio::inherit());
                    new_cmd.stdout(Stdio::inherit());
                    new_cmd.stderr(Stdio::inherit());
                    apply_state_env(&mut new_cmd, &package_root);
                    apply_window_env(&mut new_cmd, &package_root);
                    // Swap-процесс не должен удалять state.json при закрытии
                    // (state уже сохранён предыдущим процессом через SAVE_STATE
                    // или будет восстановлен из файла).
                    new_cmd.env("LIGHT_SWAP", "1");
                    // ТЗ 0.14.3-fix5 (откат fix4): по умолчанию состояние
                    // СОХРАНЯЕТСЯ при .rs change (как до fix4): child
                    // восстанавливает count из state.json, а вычисляемые
                    // значения (методы, например status) пересчитываются из
                    // нового кода. --preserve-state — алиас (те же эффекты),
                    // поэтому блок не зависит от флага.
                    new_cmd.env("LIGHT_RESUME_FROM_SWAP", "1");
                    new_cmd.env("LIGHT_PRESERVE_STATE", "1");
                    if args.preserve_state {
                        eprintln!("light dev: .rs changed → state preserved (--preserve-state)");
                    } else {
                        eprintln!("light dev: .rs changed → state preserved (default)");
                    }
                    eprintln!("light dev: clean exit during swap — state.json kept");
                    match new_cmd.spawn() {
                        Ok(c) => {
                            let new_pid = c.id();
                            if let Ok(mut g) = child_arc.lock() {
                                *g = c;
                            } else {
                                eprintln!(
                                    "light dev: child_arc poisoned — не записала новый process"
                                );
                            }
                            // ТЗ 0.14.3-fix7: старый child уже убив синхронно
                            // (kill_child → kill + wait), так что его exit НЕ
                            // попадёт в Ev::Exited. Новый child — обычный:
                            // если пользователь закроет X → user-close → удалить
                            // state + geometry. pending_swap остаётся false.
                            // Чистим старые копии (кроме текущей run_exe) +
                            // age-привка: убиваем всё, что старше STALE_RUN_AGE
                            // или чей суффикс-timestamp не совпадает с «сегодня».
                            cleanup_run_dir(&run_dir, &new_run_exe);
                            let pruned = prune_stale_run_exes(&run_dir, &new_run_exe);
                            if pruned > 0 {
                                eprintln!(
                                    "light dev: pruned {pruned} stale executables from hot-run"
                                );
                            }
                            // Сбрасываем watcher в is_starting: он сам решит,
                            // когда закрывать окно (порт 9229 открылся либо
                            // дедлайн STARTUP_IGNORE прошёл).
                            let _ = state_tx.send(WatchState::starting());
                            eprintln!(
                                "light dev: spawning {:?} (build {:?})",
                                new_run_exe,
                                fresh_bin_path(&build_target, &bin)
                            );
                            eprintln!("light dev: swap done (pid {old_pid} -> {new_pid})");
                        }
                        Err(e) => {
                            eprintln!("light dev: не удалось запустить новое приложение: {e}");
                            done_flag.store(true, Ordering::Relaxed);
                            let _ = watcher.join();
                            return Ok(1);
                        }
                    }
                }
                // RebuildOutcome::Failed(_): старый процесс остаётся; stderr
                // cargo уже на консоли. Watcher НЕ сбрасываем (старое app
                // работает, is_starting не нужен).
            }
            Ev::Restart(kind) => match kind {
                ChangeKind::Html => {
                    // HTML-чнейдж → runtime-reload (RELOAD_TEMPLATE) без
                    // убийства процесса. При неудаче (app умер, порт недоступен)
                    // НЕ триггерим полную пересборку: ждём, пока TCP снова
                    // станет доступен (poll каждые 200 мс, до 3 с), и повторяем
                    // RELOAD_TEMPLATE. Если так и не поднимется — чнейдж
                    // потеряется до следующего изменения (без лишних 4-5 сек
                    // на пустой restart).
                    eprintln!("light dev: HTML change → RELOAD_TEMPLATE (без пересборки)…");
                    if let Err(e) = request_reload_template_with_retry(DEVTOOLS_PORT) {
                        eprintln!(
                            "light dev: RELOAD_TEMPLATE так и не был доставлен: {e} \
                             (HTML-чнейдж потерян до следующего изменения)"
                        );
                    } else {
                        eprintln!("light dev: RELOAD_TEMPLATE доставлен — продолжаю watch");
                    }
                }
                ChangeKind::Source => {
                    // Source-чнейдж (.rs / Cargo.toml) → seamless rebuild.
                    //
                    // 1) Если уже идёт сборка — убиваем её (новый save важнее).
                    //    Лог «cancelled in-flight rebuild (new save)».
                    // 2) Поднимаем flag + таймер в state-machine.
                    // 3) Запускаем `cargo build --target-dir target/hot-build`
                    //    в фоне (non-blocking), сохраняем Child в rebuild_arc.
                    // 4) Главный цикл продолжит опрашивать — на Ev::RebuildDone
                    //    сделает swap (success) или промолчит (failure).
                    let had_in_flight = {
                        let old = rebuild_arc.lock().ok().and_then(|mut g| g.take());
                        old.map(|mut c| {
                            let _ = c.kill();
                            let _ = c.wait();
                            true
                        })
                        .unwrap_or(false)
                    };
                    if had_in_flight {
                        eprintln!("light dev: cancelled in-flight rebuild (new save)");
                    }
                    eprintln!("light dev: rebuild started (old app still running)");
                    sm.start();
                    // Собираем команду для background build. `build_target` —
                    // тот же путь, что и в warm-up (workspace_root/hot-build).
                    let mut cmd = Command::new("cargo");
                    cmd.arg("build");
                    cmd.args(["-p", &pkg_name, "--features", "light-fw/dev-mode"]);
                    cmd.args(["--target-dir", &build_target.to_string_lossy()]);
                    cmd.args(&args.extra_cargo_args);
                    cmd.stdin(Stdio::inherit())
                        .stdout(Stdio::inherit())
                        .stderr(Stdio::inherit());
                    // Запускаем; если spawn упал — считаем ошибкой (old живёт).
                    match cmd.spawn() {
                        Ok(c) => {
                            if let Ok(mut g) = rebuild_arc.lock() {
                                *g = Some(c);
                            } else {
                                eprintln!(
                                    "light dev: rebuild_arc poisoned — не записала новый build"
                                );
                            }
                        }
                        Err(e) => {
                            eprintln!("light dev: не удалось запустить `cargo build`: {e}");
                            // Сбрасываем флаг: сборка не стартовала, old остаётся.
                            sm.poll_outcome(1);
                        }
                    }
                }
            },
        }
    }
}
