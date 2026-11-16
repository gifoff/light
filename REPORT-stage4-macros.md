# ОТЧЁТ: Этап 4 — `#[component]` макрос и compile-time валидация тегов

## Резюме

Этап 4 реализован и прошёл CI 15/15. Макрос `#[component(tag = "...")]` генерирует
`impl CustomElement` + `linkme::distributed_slice` фабрика, требует наличия
`#[template]` и `#[proxy]` на структуре (иначе `compile_error!`). Макрос `#[template]`
теперь проверяет неизвестные теги по списку встроенных элементов + `provides` из
Cargo.toml текущего крейта, с graceful fallback при отсутствующем/непарсеабельном
манифесте. Trybuild-тест `component_unknown_tag.rs` фиксирует compile_error «неизвестный элемент».

## Коммиты

| SHA | Сообщение |
|-----|-----------|
| `ac67c73` | feat(macros): stage 4 — #[component] macro and compile-time tag validation |
| `50288cb` | fix(macros): align trybuild fixture — light_fw import and verified stderr |
| `2bbcf3e` | fix(macros): correct caret count in trybuild stderr fixture (76 chars) |
| `12df641` | fix(macros): remove UTF-8 BOM and ensure trailing newline in stderr fixture |
| `2a74639` | fix(macros): simplify trybuild test — remove unused proxy import to eliminate warning from stderr |
| `c267adf` | fix(macros): correct trybuild path — must include tests/ prefix (relative to CARGO_MANIFEST_DIR) |
| `03f6b40` | fix(macros): use relative path in trybuild stderr (tests/ui/ not $DIR/) |
| `5cd8059` | fix(macros): add extra space indent to trybuild stderr (proc-macro expansion adds 1-space indent) |

## Diff: light-macros (component.rs, template.rs)

### crates/light-macros/src/component.rs (НОВЫЙ, ~300 строк)

```rust
//! `#[component(tag = "name")]` — компиляторная валидация и генерация CustomElement.
//!
//! Требует:
//! - `#[template("...")]` на той же структуре
//! - `#[proxy]` на той же структуре
//! Иначе: `compile_error!`
//!
//! Генерирует:
//! - `impl CustomElement for X { fn tag() -> &'static str; fn render(...) -> VElement }`
//! - `#[linkme::distributed_slice(REGISTERED_COMPONENTS)] static X_FACTORY: &dyn ComponentFactory = &...;`

use proc_macro::TokenStream;
// ... (Parse dual-path: Paren form `(tag = "x")` + bare form `tag = "x"`)

pub fn component(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as CompInput);
    let tag = input.tag;
    // 1. Валидация: структура должна иметь #[template]
    // 2. Валидация: структура должна иметь #[proxy]
    // 3. Проверка ItemStruct
    // 4. Генерация impl CustomElement + distributed_slice factory
}
```

Ключевые функции:
- `parse_component_attr()` — dual-path парсинг (bare `tag = "x"` и paren `(tag = "x")`)
- `validate_has_template(attrs)` — проверка наличия `#[template]`
- `validate_has_proxy(attrs)` — проверка наличия `#[proxy]`
- `gen_custom_element(name, tag)` — генерация `impl CustomElement`
- `gen_factory(name, tag)` — генерация `linkme::distributed_slice`

### crates/light-macros/src/template.rs (изменения)

Добавлено:
- Строки 201–204: `CARGO_MANIFEST_DIR` → `full_path = manifest_dir.join(&relative)` → `load_nodes`
- Строка 212: `let custom_tags = crate::provides::read_provides_elements(&manifest_dir);`
- Match arm для custom tags → `gen_custom_element`
- Строки 1272–1281: unknown-tag error:
  ```rust
  _ => {
      return Err(format!(
          "неизвестный элемент <{other}>; поддерживаются: <div>, <span>, \
           <h1>…<h5>, <text>, <password>, <checkbox>, <radio>, <select>, \
           <option>, <button>, <titlebar>, <image>, <include>, <window>. \
           Для собственного элемента объявите тег в [package.metadata.light.provides] \
           elements в Cargo.toml и реализуйте его через #[component] (Plugin API)"
      ));
  }
  ```

Graceful fallback: если `CARGO_MANIFEST_DIR` не найден или Cargo.toml не парсится —
`read_provides_elements` возвращает пустой `HashSet`, и валидация работает только
по встроенным тегам.

### crates/light-macros/src/provides.rs (НОВЫЙ, ~140 строк)

```rust
//! Чтение `[package.metadata.light.provides] elements` из Cargo.toml.

pub fn read_provides_elements(manifest_dir: &Path) -> HashSet<String> {
    let toml_path = manifest_dir.join("Cargo.toml");
    let text = match std::fs::read_to_string(&toml_path) {
        Ok(t) => t,
        Err(_) => return HashSet::new(), // graceful fallback
    };
    parse_provides_elements(&text)
}

pub fn parse_provides_elements(text: &str) -> HashSet<String> {
    let doc: toml::Value = match text.parse() {
        Ok(v) => v,
        Err(_) => return HashSet::new(), // graceful fallback
    };
    // doc["package"]["metadata"]["light"]["provides"]["elements"]
}
```

5 unit tests: valid parse, missing section, empty array, unparseable TOML, null values.

### crates/light-macros/Cargo.toml

Добавлено:
- `toml = "0.8"` в `[dependencies]`
- `trybuild = "1"` в `[dev-dependencies]`
- `light-fw = { workspace = true }` в `[dev-dependencies]`

### crates/light-macros/src/lib.rs

- `#![deny(missing_docs)]`
- `mod component;` + `#[proc_macro_attribute] pub fn component(...)`
- Mod doc описывает все три макроса
- 4 ignored doctests

## Diff: trybuild тесты

### crates/light-macros/tests/ui.rs (НОВЫЙ)

```rust
#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/component_unknown_tag.rs");
}
```

### crates/light-macros/tests/ui/component_unknown_tag.rs (НОВЫЙ)

```rust
use light_fw::template;

/// Trybuild fixture: неизвестный тег <datepicker> без provides в Cargo.toml
/// приводит к compile_error.

#[derive(Debug, Clone)]
#[template("../../../../crates/light-macros/templates/ui_unknown_tag.html")]
pub struct UiApp {
    pub title: String,
}

fn main() {}
```

### crates/light-macros/tests/ui/component_unknown_tag.stderr (НОВЫЙ)

```
error: неизвестный элемент <datepicker>; поддерживаются: <div>, <span>, <h1>…<h5>, <text>, <password>, <checkbox>, <radio>, <select>, <option>, <button>, <titlebar>, <image>, <include>, <window>. Для собственного элемента объявите тег в [package.metadata.light.provides] elements в Cargo.toml и реализуйте его через #[component] (Plugin API)
  --> tests/ui/component_unknown_tag.rs:14:1
   |
14 | #[template("../../../../crates/light-macros/templates/ui_unknown_tag.html")]
   | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
   |
   = note: this error originates in the attribute macro `template` (in Nightly builds, run with -Z macro-backtrace for more info)
```

### crates/light-macros/templates/ui_unknown_tag.html (НОВЫЙ)

```html
<div class="app"><datepicker></datepicker></div>
```

## Локальные результаты

```
cargo test -p light-fw-macros --lib
test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

cargo clippy -p light-fw-macros --all-targets -- -D warnings
(чисто, 0 warnings)

cargo fmt --all -- --check
(чисто)
```

## CI

**Run #43848200168 — 15/15 ✅**

URL: https://github.com/gifoff/light/actions/runs/43848200168

| Job | Статус |
|-----|--------|
| Build examples (default features) | ✓ |
| Tests (ubuntu-latest) | ✓ |
| Tests (macos-latest) | ✓ |
| Tests (windows-latest) | ✓ |
| Documentation | ✓ |
| Clippy (ubuntu/macos/windows) | ✓ |
| Formatting | ✓ |
| Features (no-default/basic/full/a11y/form-window) | ✓ |

`examples/plugin-demo` собирается в CI (`cargo build -p example-plugin-demo`) —
реальный `[package.metadata.light.provides] elements = ["datepicker"]` +
`#[component(tag = "datepicker")]` + `#[template]` + `#[proxy]`.

## Путь к отчёту

`REPORT-stage4-macros.md` (этот файл, в корне репозитория).
