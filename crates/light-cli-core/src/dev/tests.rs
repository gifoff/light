#[cfg(test)]
mod tests {
    use crate::dev::args::parse_dev_command;
    use crate::dev::build::{
        ChangeKind, RebuildOutcome, RebuildStateMachine, WatchAction, WatchState, bin_name,
        classify_change, cleanup_run_dir, copy_fresh_to_run, file_hash, fresh_bin_path,
        handle_watch_event, resolve_startup_completion, run_exe_path, wait_port_available,
    };
    use crate::dev::version::{
        build_info_path, check_cli_version_mismatch, clean_dev_cache, minor_key,
        parse_workspace_package_version, prune_stale_run_exes, read_fw_core_version_from_lock,
        read_fw_core_version_from_workspace_toml, same_minor, save_build_info,
    };
    use crate::dev::watch::resolve_package_root;
    use std::io::Write as _;
    use std::net::TcpListener;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};

    // ===================== handle_watch_event / resolve =====================

    /// Нерелевантное событие (kind=None): ни действий, ни изменения флагов.
    #[test]
    fn watch_event_irrelevant_does_nothing() {
        let mut s = WatchState::starting();
        assert_eq!(handle_watch_event(&mut s, None), WatchAction::None);
        assert!(s.is_starting);
        assert!(!s.pending_html);
        assert!(!s.pending_source);
    }

    /// **ТЗ 4.1**: HTML-чнейдж во время `is_starting` → `pending_html=true`,
    /// `pending_source=false`. При закрытии окна → ReloadHtml (НЕ restart).
    #[test]
    fn html_during_start_pending_as_html() {
        let mut s = WatchState::starting();
        assert_eq!(
            handle_watch_event(&mut s, Some(ChangeKind::Html)),
            WatchAction::None
        );
        assert!(s.is_starting);
        assert!(s.pending_html);
        assert!(!s.pending_source);
        // Окно закрывается (порт 9229 открылся либо дедлайн прошёл):
        // применение → Html (runtime-reload), не Source (пересборка).
        assert_eq!(resolve_startup_completion(&s), Some(ChangeKind::Html));
    }

    /// **ТЗ 4.2**: Source-чнейдж во время `is_starting` → `pending_source=true`,
    /// `pending_html=false`. При закрытии окна → Source (пересборка).
    #[test]
    fn source_during_start_pending_as_source() {
        let mut s = WatchState::starting();
        assert_eq!(
            handle_watch_event(&mut s, Some(ChangeKind::Source)),
            WatchAction::None
        );
        assert!(s.is_starting);
        assert!(!s.pending_html);
        assert!(s.pending_source);
        assert_eq!(resolve_startup_completion(&s), Some(ChangeKind::Source));
    }

    /// **ТЗ 4.3**: HTML-чнейдж ПОСЛЕ закрытия `is_starting` → немедленный
    /// `ReloadHtml` (не restart). Это ключевой фикс race: HTML-чнейдж не
    /// должен приводить к полной пересборке.
    #[test]
    fn html_after_start_emits_reload() {
        let mut s = WatchState {
            is_starting: false,
            pending_html: false,
            pending_source: false,
        };
        assert_eq!(
            handle_watch_event(&mut s, Some(ChangeKind::Html)),
            WatchAction::ReloadHtml
        );
        assert!(!s.is_starting);
        assert!(!s.pending_html);
        assert!(!s.pending_source);
    }

    /// Source-чнейдж после закрытия окна → немедленный Restart.
    #[test]
    fn source_after_start_emits_restart() {
        let mut s = WatchState {
            is_starting: false,
            pending_html: false,
            pending_source: false,
        };
        assert_eq!(
            handle_watch_event(&mut s, Some(ChangeKind::Source)),
            WatchAction::Restart
        );
    }

    /// Приоритет при закрытии окна: Source > Html (пересборка важнее reload).
    #[test]
    fn startup_priority_source_over_html() {
        let mut s = WatchState::starting();
        let _ = handle_watch_event(&mut s, Some(ChangeKind::Html));
        let _ = handle_watch_event(&mut s, Some(ChangeKind::Source));
        assert!(s.pending_html && s.pending_source);
        assert_eq!(resolve_startup_completion(&s), Some(ChangeKind::Source));
    }

    /// Без pending-флагов при закрытии окна — `None` (действий нет).
    #[test]
    fn startup_no_pending_returns_none() {
        let s = WatchState::starting();
        assert_eq!(resolve_startup_completion(&s), None);
    }

    /// Если перед событием был pending и окно уже закрыто — `ApplyPending`
    /// (один перезапуск за накопленную серию).
    #[test]
    fn event_with_stale_pending_returns_apply_pending() {
        let mut s = WatchState {
            is_starting: false,
            pending_html: true,
            pending_source: false,
        };
        assert_eq!(
            handle_watch_event(&mut s, Some(ChangeKind::Source)),
            WatchAction::ApplyPending
        );
        // Флаги сброшены.
        assert!(!s.pending_html && !s.pending_source);
    }

    // ===================== wait_port_available / reload retry ==============

    /// **ТЗ 4.4** (логика мока TCP): порт стал доступен до дедлайна →
    /// `wait_port_available` возвращает `true` раньше, чем истёк `max`.
    /// Не запускает пересборку: просто подтверждает, что повторная попытка
    /// `RELOAD_TEMPLATE` возможна.
    #[test]
    fn wait_port_available_becomes_open_within_max() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().unwrap().port();
        // Порт уже открыт — должно вернуть true мгновенно (без ожидания max).
        let start = Instant::now();
        assert!(wait_port_available(
            port,
            Duration::from_millis(500),
            Duration::from_millis(20)
        ));
        // Задержка < max: подтверждается, что мы не ждали всё время.
        assert!(
            start.elapsed() < Duration::from_millis(500),
            "wait_port_available вернул слишком поздно"
        );
    }

    /// **ТЗ 4.4** (часть 2): порт так и не открылся → `false` через ~`max`.
    /// В production это означает: `request_reload_template_with_retry` вернёт
    /// `Err` — без триггера полной пересборки (HTML-чнейдж просто потеряется
    /// до следующего изменения).
    ///
    /// Проверяем поведение (не wall-clock время): если функция вернула `false`,
    /// значит цикл закончился по дедлайну; если бы она «зависла» или «сработала
    /// раньше», мы бы получили `true` или panic по timeout. Дополнительная
    /// проверка на wall-clock сознательно опущена — на Windows под нагрузкой
    /// `thread::sleep` не даёт точного верхнего bound.
    #[test]
    fn wait_port_available_never_opens_times_out() {
        // Берём свободный порт: listen'им и сразу drop'аем слушателя, чтобы
        // порт гарантированно не принимал соединения.
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        // Короткий дедлайн (80 мс), быстрый poll (10 мс) — тест завершится
        // быстро и надёжно. Функция должна вернуть false.
        let result =
            wait_port_available(port, Duration::from_millis(80), Duration::from_millis(10));
        assert!(
            !result,
            "wait_port_available должен вернуть false для закрытого порта"
        );
    }

    // ===================== parse / classify / package_root ==================

    /// Без имени пакета корнем становится текущий каталог (быстрый путь без
    /// запуска `cargo metadata`).
    #[test]
    fn package_root_without_package_is_cwd() {
        let root = resolve_package_root(None, std::path::Path::new("/tmp/workspace"));
        assert_eq!(root, std::path::PathBuf::from("/tmp/workspace"));
    }

    /// Без аргументов: ни пакета, ни доп. флагов.
    #[test]
    fn parse_empty() {
        let args = parse_dev_command(&[]);
        assert!(args.package.is_none());
        assert!(args.extra_cargo_args.is_empty());
    }

    /// Один аргумент — имя пакета, без доп. флагов.
    #[test]
    fn parse_package_only() {
        let args = parse_dev_command(&["my-app".into()]);
        assert_eq!(args.package.as_deref(), Some("my-app"));
        assert!(args.extra_cargo_args.is_empty());
    }

    /// Пакет + доп. флаги: первый свободный — пакет, остальное — в cargo.
    #[test]
    fn parse_package_and_extra() {
        let args = parse_dev_command(&[
            "my-app".into(),
            "--release".into(),
            "--".into(),
            "arg0".into(),
        ]);
        assert_eq!(args.package.as_deref(), Some("my-app"));
        assert_eq!(
            args.extra_cargo_args,
            vec!["--release".to_owned(), "--".to_owned(), "arg0".to_owned()]
        );
    }

    /// Несколько позиционных: первый — пакет, остальные — доп. флаги.
    #[test]
    fn parse_multiple_positionals_first_is_package() {
        let args = parse_dev_command(&["crate-a".into(), "crate-b".into(), "flag".into()]);
        assert_eq!(args.package.as_deref(), Some("crate-a"));
        assert_eq!(
            args.extra_cargo_args,
            vec!["crate-b".to_owned(), "flag".to_owned()]
        );
    }

    /// Только доп. флаги (начинаются с `-`): пакета нет, всё уходит в cargo.
    #[test]
    fn parse_flags_only_no_package() {
        let args = parse_dev_command(&["--release".into(), "--quiet".into()]);
        assert!(args.package.is_none());
        assert_eq!(
            args.extra_cargo_args,
            vec!["--release".to_owned(), "--quiet".to_owned()]
        );
    }

    /// `--verbose` вынимается из доп. аргументов и не попадает в cargo.
    #[test]
    fn parse_verbose_flag() {
        let args = parse_dev_command(&["my-app".into(), "--verbose".into(), "--release".into()]);
        assert!(args.verbose);
        assert_eq!(args.package.as_deref(), Some("my-app"));
        assert_eq!(args.extra_cargo_args, vec!["--release".to_owned()]);
    }

    /// `--verbose` без пакета.
    #[test]
    fn parse_verbose_only() {
        let args = parse_dev_command(&["--verbose".into()]);
        assert!(args.verbose);
        assert!(args.package.is_none());
        assert!(args.extra_cargo_args.is_empty());
    }

    /// `--preserve-state` вынимается из аргументов (не попадает в cargo).
    #[test]
    fn parse_preserve_state_flag() {
        let args = parse_dev_command(&[
            "my-app".into(),
            "--preserve-state".into(),
            "--release".into(),
        ]);
        assert!(args.preserve_state);
        assert_eq!(args.package.as_deref(), Some("my-app"));
        assert_eq!(args.extra_cargo_args, vec!["--release".to_owned()]);
    }

    /// По умолчанию preserve_state = false.
    #[test]
    fn parse_preserve_state_default_false() {
        let args = parse_dev_command(&[]);
        assert!(!args.preserve_state);
    }

    /// `.html` → `ChangeKind::Html`.
    #[test]
    fn classify_html_file() {
        assert_eq!(
            classify_change(Path::new("templates/app.html")),
            Some(ChangeKind::Html)
        );
    }

    /// `.rs` → `ChangeKind::Source`.
    #[test]
    fn classify_rs_file() {
        assert_eq!(
            classify_change(Path::new("src/main.rs")),
            Some(ChangeKind::Source)
        );
    }

    /// `Cargo.toml` → `ChangeKind::Source`.
    #[test]
    fn classify_cargo_toml() {
        assert_eq!(
            classify_change(Path::new("Cargo.toml")),
            Some(ChangeKind::Source)
        );
    }

    /// Неизвестное расширение → `None` (игнорируется).
    #[test]
    fn classify_unknown_extension_returns_none() {
        assert_eq!(classify_change(Path::new("assets/logo.png")), None);
        assert_eq!(classify_change(Path::new("data.json")), None);
    }

    /// Файл без расширения → `None`.
    #[test]
    fn classify_no_extension_returns_none() {
        assert_eq!(classify_change(Path::new("README")), None);
    }

    // ===================== file_hash (дедупликация) ========================

    /// **ТЗ 2.1**: одно и то же содержимое даёт одинаковый хэш (повторное
    /// событие игнорируется), новое содержимое — другой хэш (обрабатывается).
    ///
    /// Сценарий из ТЗ:
    ///   - записать "a" → hash1
    ///   - записать "a" ещё раз → hash2 == hash1 (ИГНОРИРУЕТСЯ)
    ///   - записать "b" → hash3 != hash1 (ОБРАБАТЫВАЕТСЯ)
    #[test]
    fn file_hash_same_content_same_hash() {
        let dir = std::env::temp_dir().join("light_dev_test_file_hash");
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join("sample.html");
        // Убедимся, что файл не существует с предыдущих тестов.
        let _ = std::fs::remove_file(&path);

        // Записываем "a".
        let mut f = std::fs::File::create(&path).expect("create file");
        f.write_all(b"a").expect("write 'a'");
        f.sync_all().expect("sync");
        drop(f);
        let hash1 = file_hash(&path, false).expect("hash1 must be Some");

        // Записываем "a" ещё раз (симулируем повторное событие notify).
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .expect("reopen");
        f.set_len(0).expect("truncate");
        f.write_all(b"a").expect("rewrite 'a'");
        f.sync_all().expect("sync");
        drop(f);
        let hash2 = file_hash(&path, false).expect("hash2 must be Some");

        // Одно и то же содержимое → одинаковый хэш.
        assert_eq!(
            hash1, hash2,
            "одинаковое содержимое должно давать одинаковый хэш"
        );

        // Записываем "b" (новое содержимое).
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .expect("reopen");
        f.set_len(0).expect("truncate");
        f.write_all(b"b").expect("write 'b'");
        f.sync_all().expect("sync");
        drop(f);
        let hash3 = file_hash(&path, false).expect("hash3 must be Some");

        // Разное содержимое → разные хэши (крайне маловероятно совпадение
        // у DefaultHasher для двух байтов, но формально проверяем).
        assert_ne!(hash1, hash3, "разное содержимое должно дать разный хэш");

        // Cleanup.
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    /// Файл, которого нет → `None` (не дедуплицируется, обрабатывается как
    /// изменение — консервативно).
    #[test]
    fn file_hash_missing_file_returns_none() {
        let dir = std::env::temp_dir().join("light_dev_test_file_hash_missing");
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join("does_not_exist.html");
        let _ = std::fs::remove_file(&path);
        assert_eq!(file_hash(&path, false), None);
        let _ = std::fs::remove_dir(&dir);
    }

    // ===================== ТЗ 0.14.2-A-fix4: dedup after debounce =========

    /// **ТЗ 4.1**: имитируем двухэтапный save (append + flush) — первое
    /// событие видит неполное содержимое, второе — полное. После debounce
    /// содержимое уже финальное, и hash-фильтр корректно отбрасывает повторы.
    ///
    /// Сценарий:
    ///   1. записать "a" → событие 1 (неполное, append)
    ///   2. дописать "b" → событие 2 (полное, flush)
    ///   3. debounce срабатывает → хэшируем: hash("ab")
    ///   4. last_hashes["file"] = hash("ab")
    ///   5. следующее notify-событие с тем же содержимым → hash == last →
    ///      ИГНОРИРУЕТСЯ (1 reload на 2 notify-события, не 2).
    ///
    /// Тест проверяет именно то, что происходит ПОСЛЕ debounce: если бы мы
    /// хэшировали ДО debounce, hash1 ("a") != hash2 ("ab"), и оба события
    /// прошли бы → двойной reload.
    #[test]
    fn dedup_after_debounce() {
        let dir = std::env::temp_dir().join("light_dev_test_dedup_after_debounce");
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join("sample.html");
        let _ = std::fs::remove_file(&path);

        // Этап 1 (append): пишем "a" — это первый notify-событие, файл
        // содержит только "a".
        let mut f = std::fs::File::create(&path).expect("create");
        f.write_all(b"a").expect("write 'a'");
        f.sync_all().expect("sync");
        drop(f);

        // Этап 2 (flush): дописываем "b" — второй notify-событие, файл
        // содержит "ab".
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .expect("reopen-append");
        f.write_all(b"b").expect("append 'b'");
        f.sync_all().expect("sync");
        drop(f);

        // К этому моменту debounce (100/500 мс) уже истёк. Хэшируем финальное
        // содержимое "ab":
        let final_hash = file_hash(&path, false).expect("hash of 'ab'");

        // Симулируем «повторное» notify-событие (Modify(Data) / Modify(Metadata)):
        // содержимое НЕ изменилось, хэш должен совпадать.
        let dup_hash = file_hash(&path, false).expect("hash on duplicate event");
        assert_eq!(
            final_hash, dup_hash,
            "повторное событие того же содержимого должно дать одинаковый хэш \
             (и быть отброшено hash-фильтром)"
        );

        // Теперь симулируем НОВЫЙ save: содержимое изменилось.
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .expect("rewrite");
        f.set_len(0).expect("truncate");
        f.write_all(b"xyz").expect("write 'xyz'");
        f.sync_all().expect("sync");
        drop(f);
        let new_hash = file_hash(&path, false).expect("hash of 'xyz'");
        assert_ne!(
            final_hash, new_hash,
            "новое содержимое даёт новый хэш → пройдёт hash-фильтр"
        );

        // Cleanup.
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    /// **ТЗ 4.2**: имитируем временную ошибку чтения (например, PermissionDenied
    /// при частичной записи, NotFound в момент атомарного удаления). `file_hash`
    /// должна ретраить 3 раза с интервалом 50 мс, и после всех попыток вернуть
    /// `None` (если файл реально недоступен) или `Some(hash)` (если ошибка
    /// ушла в процессе ретраев).
    ///
    /// Мы не можем реально вызвать `PermissionDenied` под sandbox-ограничениями,
    /// поэтому тестируем наблюдаемое поведение:
    /// - файл существует и читается → `Some(hash)`;
    /// - файла нет вообще → 3 неудачные попытки (лог с "read attempt N failed")
    ///   и возврат `None`.
    ///
    /// Поведение «ретраи до 3 раз» проверяем косвенно: время выполнения
    /// `file_hash` для несуществующего файла >= 2 × 50 мс (два sleep между
    /// тремя попытками), но < 1 с (не блокирует поток watcher надолго).
    #[test]
    fn file_hash_retries_on_permission_denied() {
        let dir = std::env::temp_dir().join("light_dev_test_file_hash_retry");
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join("missing.html");
        let _ = std::fs::remove_file(&path);

        let start = Instant::now();
        let result = file_hash(&path, false);
        let elapsed = start.elapsed();

        // Файл не существует: все 3 попытки падают, возвращаем None.
        assert_eq!(
            result, None,
            "несуществующий файл → None после всех ретраев"
        );

        // Два sleep по 50 мс (между 3 попытками): минимум ~100 мс,
        // максимум ~200 мс с запасом на задержки ОС. НЕ проверяем верхнюю
        // границу жёстко: Windows `thread::sleep` может overshoot-нуть.
        assert!(
            elapsed >= Duration::from_millis(100),
            "ожидаю минимум 2 × 50 мс ретраев, получил {elapsed:?}"
        );
        assert!(
            elapsed < Duration::from_secs(1),
            "ретраи должны завершиться быстро (< 1 с), получили {elapsed:?}"
        );

        // Cleanup.
        let _ = std::fs::remove_dir(&dir);
    }

    // ===================== ТЗ 0.14.2-B: seamless rebuild state machine ======

    /// **ТЗ 6.1**: `start()` поднимает `in_progress` и задаёт `started_at`.
    #[test]
    fn rebuild_started_flag() {
        let mut sm = RebuildStateMachine::default();
        assert!(!sm.in_progress, "начальное состояние: in_progress=false");
        assert!(
            sm.started_at.is_none(),
            "начальное состояние: started_at=None"
        );
        sm.start();
        assert!(sm.in_progress, "после start(): in_progress=true");
        assert!(
            sm.started_at.is_some(),
            "после start(): started_at=Some(Instant)"
        );
    }

    /// **ТЗ 6.2**: `poll_outcome(0)` → Success (swap будет выполнен).
    /// `started_at` остаётся (для лога «took N s»); `in_progress` сбрасывается.
    #[test]
    fn rebuild_success_triggers_swap() {
        let mut sm = RebuildStateMachine::default();
        sm.start();
        let Some(started) = sm.started_at else {
            panic!("expected started_at to be set after start()");
        };
        let outcome = sm.poll_outcome(0);
        assert_eq!(outcome, RebuildOutcome::Success);
        assert!(!sm.in_progress, "после poll_outcome: in_progress=false");
        assert_eq!(
            sm.started_at,
            Some(started),
            "started_at сохраняется для лога"
        );
    }

    /// **ТЗ 6.3**: `poll_outcome(N)` при N != 0 → Failed(N) (old живёт).
    /// `started_at` остаётся (для возможного последующего пересбора).
    #[test]
    fn rebuild_failure_keeps_old() {
        let mut sm = RebuildStateMachine::default();
        sm.start();
        let Some(started) = sm.started_at else {
            panic!("expected started_at to be set after start()");
        };
        let outcome = sm.poll_outcome(2);
        assert_eq!(outcome, RebuildOutcome::Failed(2));
        assert!(!sm.in_progress, "после poll_outcome: in_progress=false");
        assert_eq!(sm.started_at, Some(started), "started_at сохраняется");
    }

    /// **ТЗ 6.4**: новый save во время in-flight сборки → убить старую,
    /// `start()` заново (flag + fresh timer).
    ///
    /// Юнит на уровне SM: проверяем, что после `reset()`+`start()` флаг
    /// возвращается к `true`, а `started_at` обновляется. Убийство процесса
    /// выполняется в main-loop (через `rebuild_arc`) — это integration test.
    #[test]
    fn new_save_cancels_in_flight() {
        let mut sm = RebuildStateMachine::default();
        sm.start();
        assert!(sm.in_progress);
        let first_start = sm.started_at.unwrap();
        // Имитация: новый save пришёл, старую сборку убили, новая запускается.
        // Короткий sleep гарантирует, что второй вызов Instant::now() внутри
        // start() вернёт значение, строго отличное от первого (на Linux
        // разрешение системных часов может быть грубым — несколько мс).
        std::thread::sleep(std::time::Duration::from_millis(5));
        sm.start();
        assert!(sm.in_progress, "после нового start(): in_progress=true");
        assert_ne!(
            sm.started_at,
            Some(first_start),
            "started_at должен быть обновлён (новый запуск)"
        );
    }

    // ===================== ТЗ 0.14.2-B-fix2: path helpers ================

    /// Бинарник: имя пакета без изменений (cargo не заменяет `-`). Windows: `.exe`.
    #[test]
    fn bin_name_preserves_dashes() {
        assert_eq!(bin_name("my-app"), "my-app");
        assert_eq!(bin_name("my-app-x"), "my-app-x");
        assert_eq!(bin_name("simple"), "simple");
    }

    /// Свежий бинарник: `<build_target>/debug/<package-name>[.exe]`.
    #[test]
    fn fresh_bin_path_layout() {
        let base = std::env::temp_dir();
        let build_target = base.join("hot-build");
        let p = fresh_bin_path(&build_target, "my-app");
        let s = p.to_string_lossy().to_string();
        assert!(s.contains("hot-build"), "path contains hot-build: {s}");
        assert!(
            s.ends_with("my-app.exe") || s.ends_with("my-app"),
            "correct bin name: {s}"
        );
        if cfg!(windows) {
            assert!(s.ends_with(".exe"), "windows .exe, got: {s}");
        }
    }

    /// Копия для запуска: `<run_dir>/<bin>-<ts>[.exe]` — имя содержит bin
    /// и timestamp, расширение `.exe` на Windows.
    #[test]
    fn run_exe_path_has_timestamp_and_extension() {
        let run_dir = std::env::temp_dir().join("hot-run");
        let p = run_exe_path(&run_dir, "my-app");
        let file = p
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        assert!(
            file.starts_with("my-app-"),
            "filename should start with bin + dash, got: {file}"
        );
        if cfg!(windows) {
            assert!(file.ends_with(".exe"), "windows extension, got: {file}");
        } else {
            assert!(!file.ends_with(".exe"), "no .exe on unix, got: {file}");
        }
    }

    /// `copy_fresh_to_run`: создаёт run_dir, копирует, возвращает путь к копии.
    /// `cleanup_run_dir`: удаляет старые .exe, кроме активного.
    #[test]
    fn copy_and_cleanup_run_dir() {
        // Используем CWD-относительный каталог, потому что temp_dir() в этом
        // окружении имеет ACL-ограничения (см. предсуществующие ACL-файлы
        // dev::tests::file_hash_*). Каталог уникален по pid + nanos и чистится
        // в конце теста.
        let tmp = std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(format!(
                ".light-dev-test-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ));
        let build_root = tmp.join("hot-build");
        let build_target = build_root.join("debug");
        let run_dir = tmp.join("hot-run");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&build_target).expect("create build dir");

        // Создаём фейковый fresh-бинарник в <build_target>/<bin>[.exe].
        let bin = "test-app";
        #[cfg_attr(not(windows), allow(unused_mut))]
        let mut fresh = build_target.join(bin);
        #[cfg(windows)]
        {
            fresh.set_extension("exe");
        }
        std::fs::write(&fresh, b"fake").expect("write fake binary");

        // Копируем: build_root = <tmp>/hot-build.
        let copied = copy_fresh_to_run(&build_root, &run_dir, bin).expect("copy should succeed");
        assert!(copied.exists(), "copy should exist: {:?}", copied);
        let name = copied
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        assert!(name.starts_with("test-app-"), "name: {name}");

        // Чистим: добавляем «старый» .exe и проверяем, что он удалён, а
        // текущий остался.
        #[cfg_attr(not(windows), allow(unused_mut))]
        let mut stale = run_dir.join("test-app-oldstale");
        #[cfg(windows)]
        {
            stale.set_extension("exe");
        }
        std::fs::write(&stale, b"old").expect("write stale");
        cleanup_run_dir(&run_dir, &copied);
        assert!(!stale.exists(), "stale should be removed: {:?}", stale);
        assert!(copied.exists(), "current should remain: {:?}", copied);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    // ===================== TZ 0.14.4: cache invalidation tests =====================

    /// ТЗ 6.1: round-trip — `save_build_info` пишет JSON с двумя полями,
    /// `read_fw_core_version_from_lock` читает версию из `Cargo.lock`.
    #[test]
    fn build_info_round_trip() {
        let tmp = test_tmp_dir("build_info");
        std::fs::create_dir_all(&tmp).expect("create tmp");

        let info = build_info_path(&tmp);
        save_build_info(&info, "0.14.0", "abc123hash");

        // Проверяем, что файл существует и содержит обе версии.
        let content = std::fs::read_to_string(&info).expect("read build-info.json");
        assert!(content.contains("0.14.0"), "version in file: {content}");
        assert!(content.contains("abc123hash"), "hash in file: {content}");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// ТЗ 6.1 (часть 2): `read_fw_core_version_from_lock` находит версию
    /// light-fw-core в тексте Cargo.lock.
    #[test]
    fn read_fw_core_version_from_lock_text() {
        let tmp = test_tmp_dir("lock_parse");
        std::fs::create_dir_all(&tmp).expect("create tmp");
        let lock = tmp.join("Cargo.lock");

        // Фиктивный Cargo.lock с блоком light-fw-core.
        let lock_content = r#"
[[package]]
name = "light-fw-core"
version = "0.14.0"
dependencies = [
 "serde",
]

[[package]]
name = "other-crate"
version = "1.0.0"
"#;
        std::fs::write(&lock, lock_content).expect("write fake lock");

        let version = read_fw_core_version_from_lock(&lock).expect("should find version");
        assert_eq!(version, "0.14.0");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// ТЗ 6.2: `--clean` чистит правильные пути: hot-build, hot-run, state, deps.
    #[test]
    fn clean_dev_cache_removes_expected_paths() {
        let tmp = test_tmp_dir("clean_cache");
        // Создаём структуру: target/hot-build, target/hot-run, .light/state.json,
        // target/debug/deps/<package>*.
        let target = tmp.join("target");
        let hot_build = target.join("hot-build");
        let hot_run = target.join("hot-run");
        let state_file = tmp.join(".light").join("state.json");
        let deps = target.join("debug").join("deps");
        let pkg_dep = deps.join("example-devtools-demo-abcdef.exe");

        std::fs::create_dir_all(&hot_build).expect("create hot-build");
        std::fs::create_dir_all(&hot_run).expect("create hot-run");
        std::fs::create_dir_all(tmp.join(".light")).expect("create .light");
        std::fs::create_dir_all(&deps).expect("create deps");
        std::fs::write(&state_file, "{}").expect("write state");
        std::fs::write(&pkg_dep, b"fake").expect("write dep");

        // Убеждаемся, что файлы существуют до чистки.
        assert!(hot_build.exists());
        assert!(hot_run.exists());
        assert!(state_file.exists());
        assert!(pkg_dep.exists());

        // Чистим.
        let removed = clean_dev_cache(&tmp, Some("example-devtools-demo"));

        // Проверяем, что все четыре пути удалены.
        assert!(!hot_build.exists(), "hot-build should be removed");
        assert!(!hot_run.exists(), "hot-run should be removed");
        assert!(!state_file.exists(), "state.json should be removed");
        assert!(!pkg_dep.exists(), "deps artifact should be removed");

        // Проверяем, что в списке removed есть наши пути.
        let removed_strs: Vec<String> = removed
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        assert!(
            removed_strs.iter().any(|s| s.contains("hot-build")),
            "removed should contain hot-build: {removed_strs:?}"
        );
        assert!(
            removed_strs.iter().any(|s| s.contains("hot-run")),
            "removed should contain hot-run: {removed_strs:?}"
        );
        assert!(
            removed_strs.iter().any(|s| s.contains("state.json")),
            "removed should contain state.json: {removed_strs:?}"
        );
        assert!(
            removed_strs
                .iter()
                .any(|s| s.contains("example-devtools-demo")),
            "removed should contain dep artifact: {removed_strs:?}"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// ТЗ 6.3: age-prune удаляет устаревшие .exe, но не текущий.
    /// Активный exe имеет свежий timestamp; старый — «вчерашний» (date ≠ today).
    #[test]
    fn prune_stale_run_exes_respects_keep_and_age() {
        let tmp = test_tmp_dir("prune_stale");
        let run_dir = tmp.join("hot-run");
        std::fs::create_dir_all(&run_dir).expect("create run dir");

        // Активный exe: свежий timestamp (сейчас) — НЕ должен быть удалён.
        let active = run_exe_path(&run_dir, "test-app");
        std::fs::write(&active, b"active").expect("write active");

        // Старый exe: «вчерашний» timestamp → date ≠ today → stale_by_date=true.
        // Берём сейчас минус 24 часа в миллисекундах.
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let yesterday_ms = now_ms.saturating_sub(86_400_000); // 24 hours ago
        let old_name = format!("test-app-{yesterday_ms}.exe");
        let old_exe = run_dir.join(&old_name);
        std::fs::write(&old_exe, b"old").expect("write old");

        // Проверяем, что активный НЕ удалён, а старый — удалён.
        let pruned = prune_stale_run_exes(&run_dir, &active);
        assert!(
            pruned >= 1,
            "should prune at least 1 stale: pruned={pruned}"
        );
        assert!(active.exists(), "active should remain: {:?}", active);
        assert!(!old_exe.exists(), "old should be removed: {:?}", old_exe);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// ТЗ fix1 4.1: `read_fw_core_version_from_workspace_toml` парсит
    /// `[workspace.package] version = "0.15.0"` из текста Cargo.toml.
    #[test]
    fn parse_workspace_package_version_basic() {
        let toml_text = r#"
[workspace]
resolver = "2"
members = ["crates/core"]

[workspace.package]
version = "0.15.0"
edition = "2024"
license = "MIT"

[workspace.dependencies]
serde = "1.0"
"#;
        let ver = parse_workspace_package_version(toml_text).expect("should parse");
        assert_eq!(ver, "0.15.0");
    }

    /// ТЗ fix1 4.1 (часть 2): отсутствие [workspace.package] → None.
    #[test]
    fn parse_workspace_package_version_missing_section() {
        let toml_text = r#"
[package]
name = "solo-crate"
version = "1.0.0"
"#;
        assert!(parse_workspace_package_version(toml_text).is_none());
    }

    /// ТЗ fix1 4.2: `read_fw_core_version_from_workspace_toml` читает файл,
    /// а при его отсутствии возвращает None (fallback на lock вызывается
    /// снаружи через .or_else).
    #[test]
    fn workspace_toml_read_and_fallback() {
        let tmp = test_tmp_dir("toml_fallback");
        let _ = std::fs::create_dir_all(&tmp);

        // С томом:
        let toml_path = tmp.join("Cargo.toml");
        let _ = std::fs::write(
            &toml_path,
            "[workspace.package]\nversion = \"0.16.0\"\nedition = \"2024\"\n",
        );
        let ver = read_fw_core_version_from_workspace_toml(&toml_path).expect("should read toml");
        assert_eq!(ver, "0.16.0");

        // Fallback: нет Cargo.toml → None (вызывающий код делает .or_else(lock)).
        let missing = tmp.join("nonexistent/Cargo.toml");
        assert!(read_fw_core_version_from_workspace_toml(&missing).is_none());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// ТЗ fix1 4.3: `clean_dev_cache` удаляет build-info.json.
    #[test]
    fn clean_dev_cache_removes_build_info() {
        let tmp = test_tmp_dir("clean_build_info");
        let _ = std::fs::create_dir_all(tmp.join("target/hot-build"));
        let _ = std::fs::create_dir_all(tmp.join(".light"));
        let bi = build_info_path(&tmp);
        let _ = std::fs::write(&bi, "{}");

        let removed = clean_dev_cache(&tmp, Some("myapp"));
        assert!(!bi.exists(), "build-info.json should be removed");
        let removed_strs: Vec<_> = removed
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        assert!(
            removed_strs.iter().any(|s| s.contains("build-info")),
            "removed should contain build-info: {removed_strs:?}"
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    // ===================== same_minor / minor_key / check_cli_version_mismatch =====

    /// `"0.15.3"` → `"0.15"`, `"0.15"` → `"0.15"`, `"0.14"` → `"0.14"`.
    #[test]
    fn minor_key_extracts_major_minor() {
        assert_eq!(minor_key("0.15.3"), "0.15");
        assert_eq!(minor_key("0.15"), "0.15");
        assert_eq!(minor_key("0.14"), "0.14");
        assert_eq!(minor_key("1.2.3"), "1.2");
        assert_eq!(minor_key("1.2"), "1.2");
    }

    /// Same minor: `"0.15.3"` vs `"0.15"` → true (no invalidation).
    #[test]
    fn same_minor_exact_vs_range() {
        assert!(same_minor("0.15.3", "0.15"));
        assert!(same_minor("0.15", "0.15.3"));
        assert!(same_minor("0.15.3", "0.15.2"));
        assert!(!same_minor("0.14", "0.15"));
        assert!(!same_minor("0.15", "0.16"));
        assert!(!same_minor("1.0", "0.99"));
    }

    /// `check_cli_version_mismatch`: project version from TOML is a range
    /// (`light = "0.15"`), installed is exact (`0.15.4`) → no mismatch.
    #[test]
    fn cli_version_mismatch_same_minor() {
        let tmp = test_tmp_dir("mismatch_same");
        let _ = std::fs::create_dir_all(&tmp);
        let toml_file = tmp.join("Cargo.toml");
        std::fs::write(&toml_file, "[dependencies]\nlight = \"0.15\"\n").unwrap();
        let lock = tmp.join("Cargo.lock");
        // No mismatch: 0.15.4 vs 0.15 → same minor.
        assert_eq!(
            check_cli_version_mismatch("0.15.4", &toml_file, &lock),
            None
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// `check_cli_version_mismatch`: different minor → Some warning.
    #[test]
    fn cli_version_mismatch_different_minor() {
        let tmp = test_tmp_dir("mismatch_diff");
        let _ = std::fs::create_dir_all(&tmp);
        let toml_file = tmp.join("Cargo.toml");
        std::fs::write(&toml_file, "[dependencies]\nlight = \"0.14\"\n").unwrap();
        let lock = tmp.join("Cargo.lock");
        let result = check_cli_version_mismatch("0.15.4", &toml_file, &lock);
        assert_eq!(result, Some(("0.15.4".to_string(), "0.14".to_string())));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Вспомогательная: создаёт уникальный CWD-относительный каталог для теста.
    fn test_tmp_dir(label: &str) -> PathBuf {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(format!(
                ".light-dev-test-{}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos(),
                label
            ))
    }
}
