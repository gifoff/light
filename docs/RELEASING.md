# Публикация релиза

Короткая инструкция для мейнтейнеров Relay. Публикация выполняется вручную:
токен crates.io не хранится в репозитории.

## 1. Подготовка

1. Убедитесь, что рабочее дерево чистое, а версия обновлена во всех
   манифестах и внутренних зависимостях.
2. Обновите `CHANGELOG.md` и `docs/ROADMAP.md`.
3. Прогоните проверки:

   ```bash
   cargo fmt --all -- --check
   cargo clippy --workspace --all-targets --all-features -- -D warnings
   cargo test --workspace --all-features
   cargo build --workspace
   ```

4. Проверьте демо:

   ```bash
   cargo run -p relay-framework-cli -- check --strict crates/relay-demo
   ```

## 2. Вход в crates.io

Создайте токен на <https://crates.io/settings/tokens> и выполните:

```bash
cargo login
```

`cargo` попросит вставить токен и сохранит его в
`~/.cargo/credentials.toml`. В CI передавайте токен через переменную
окружения `CARGO_REGISTRY_TOKEN`, а не через файл.

## 3. Проверка пакетов

Сначала выполните dry-run — он упакует все крейты, но ничего не опубликует:

```bash
cargo run -p relay-framework-cli -- publish --dry-run
```

Если `relay` уже установлен:

```bash
relay publish --dry-run
```

`--dry-run` полезен для проверки состава пакетов и порядка публикации.
Он не требует токена.

## 4. Публикация

Рекомендуемый способ — `relay publish`: команда сама строит топологический
порядок из `cargo metadata`, пропускает `publish = false` (например,
`relay-demo`), проверяет версии и внутренние зависимости, запускает
pre-flight-гейты и повторяет попытки при устаревании индекса crates.io.

```bash
cargo run -p relay-framework-cli -- publish
```

Или, если CLI установлен:

```bash
relay publish
```

Для неинтерактивного запуска:

```bash
relay publish --yes
```

Токен можно передать явно:

```bash
relay publish --token "$CARGO_REGISTRY_TOKEN"
```

Полезные флаги: `--allow-dirty`, `--skip-gates`, `--no-verify`,
`--only <крейт>` (повторить публикацию одного крейта).

### Ручной вариант через `cargo publish`

Если нужно публиковать вручную, соблюдайте порядок зависимостей:

```bash
cargo publish -p relay-html
cargo publish -p relay-cli-core
cargo publish -p relay-framework-cli
cargo publish -p relay-macros
cargo publish -p relay-core
cargo publish -p relay-behavior-validation
cargo publish -p relay-element-form
cargo publish -p relay
```

После каждой публикации может потребоваться пауза, пока индекс crates.io
увидит новую версию. `relay publish` делает такие паузы и повторы
автоматически.

## 5. Тег и проверка

После успешной публикации всех крейтов создайте тег:

```bash
git tag -a vX.Y.Z -m "Release X.Y.Z"
git push origin master
git push origin vX.Y.Z
```

Проверьте результат:

```bash
cargo search relay
cargo install relay-framework-cli --version X.Y.Z
```

Также откройте страницы крейтов на crates.io и убедитесь, что docs.rs
собрал документацию.

## Важно

- Опубликованную версию нельзя перезаписать. Если публикация оборвалась
  после части крейтов, исправьте проблему и выпустите следующую версию;
  `relay publish` пропустит уже опубликованные крейты.
- Не публикуйте с грязным рабочим деревом без необходимости.
- Тег ставьте только на проверенный коммит, из которого собраны пакеты.
