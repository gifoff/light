# Публикация релиза

Короткая инструкция для мейнтейнеров Light. Публикация выполняется вручную:
токен crates.io не хранится в репозитории.

## 1. Подготовка

1. **Убедитесь, что рабочее дерево абсолютно чистое.** Выполните `git status --short`. Если есть модифицированные файлы, закоммитьте их. **Публикация с грязным деревом приведет к сбою.**
2. Проверьте, что версия обновлена во всех манифестах и внутренних зависимостях.
3. Обновите `CHANGELOG.md` и `docs/ROADMAP.md`.
4. Прогоните проверки:

   ```bash
   cargo fmt --all -- --check
   cargo clippy --workspace --all-targets --all-features -- -D warnings
   cargo test --workspace --all-features
   cargo build --workspace
   ```

   На Windows (MSVC) ограничьте параллелизм:
   ```powershell
   $env:CARGO_BUILD_JOBS = "1"
   cargo clippy --workspace --all-targets --all-features -- -D warnings
   cargo test --workspace --all-features -j 2
   ```

## 2. Вход в crates.io

Создайте токен на <https://crates.io/settings/tokens> и выполните:

```bash
cargo login
```

## 3. Проверка пакетов

Сначала выполните dry-run:

```bash
light publish --dry-run
```

Перед dry-run очистите локальные артефакты:
`cargo clean -p example-dashboard`.

## 4. Публикация

Рекомендуемый способ — `light publish`:

```bash
light publish
```

Для неинтерактивного запуска (CI):

```bash
light publish --yes --skip-gates --no-verify
```

Полезные флаги: `--allow-dirty`, `--skip-gates`, `--no-verify`,
`--only <крейт>`.

## 5. Тег и проверка

После успешной публикации всех 8 крейтов создайте тег:

```bash
git tag -a vX.Y.Z -m "Release X.Y.Z"
git push origin master
git push origin vX.Y.Z
```

Проверьте результат:

```bash
cargo search light
cargo install light --version X.Y.Z
```

## Важно

- Опубликованную версию нельзя перезаписать. Если публикация оборвалась
  после части крейтов, исправьте проблему, сделайте bump до следующей версии
  и запустите `light publish` снова (он пропустит уже опубликованные крейты).
- Не публикуйте с грязным рабочим деревом.
- Тег ставьте только на проверенный коммит, из которого собраны пакеты.