# REPORT: fix(macros) — gen_custom_element generates valid VElement expression

## Резюме

Исправлен критический баг в `crates/light-macros/src/template.rs`: функция
`gen_custom_element` генерировала **statement** (`__children.push(...);`),
который вставлялся вызывающим кодом `gen_push_node` внутрь ещё одного
`__children.push(...)`. Результат — синтаксически невалидный Rust:

```
__children.push(__children.push(VElement::container(...)); ...);
//                ^ statement как аргумент — компилятор падает с
//                  "expected one of ')' , ',' , '.' , '?' , or an operator, found ';'"
```

После фикса любой `#[template]`, содержащий тег из
`[package.metadata.light.provides.elements]`, компилируется корректно.
Пример `plugin-demo` теперь использует реальный `<datepicker />`.

## 1. Diff template.rs (фикс)

```diff
--- a/crates/light-macros/src/template.rs
+++ b/crates/light-macros/src/template.rs
@@ -1291,21 +1291,25 @@ impl Generator {
     /// `#[linkme::distributed_slice(REGISTERED_COMPONENTS)]` в `#[component]`)
     /// вызывает `render` плагина и перекрашивает корень в `Custom(тег)`.
     /// Атрибуты и дочерние узлы из HTML передаются плагину тем же путём.
+    ///
+    /// Важно: генерирует **блок-выражение** (как `gen_container`), а не
+    /// statement. Вызывающий код (`gen_push_node`) оборачивает результат в
+    /// `__children.push(#element);` — поэтому здесь свой `let mut __children`
+    /// создаётся внутри блока, а на выходе возвращается `VElement` как
+    /// значение последнего выражения.
     fn gen_custom_element(&mut self, element: &Element, key: &KeyExpr) -> syn::Result<TokenStream> {
         let tag_literal = element.name.clone();
-        // Ключ для контейнера (без `.token()` — он нужен только для `#key` в quote!).
-        let children_tokens = self.gen_children(&element.children, Some(key))?;
+        let pushes = self.gen_children(&element.children, Some(key))?;
         let key = key.token();
-        Ok(quote! {
-            __children.push(
-                ::light_fw::vdom::VElement::container(
-                    #key,
-                    ::light_fw::vdom::NodeKind::Custom(::light_fw::vdom::intern_key(#tag_literal)),
-                    vec![],
-                )
-            );
-            #(#children_tokens)*
-        })
+        Ok(quote! {{
+            let mut __children = ::std::vec::Vec::new();
+            #(#pushes)*
+            ::light_fw::vdom::VElement::container(
+                #key,
+                ::light_fw::vdom::NodeKind::Custom(::light_fw::vdom::intern_key(#tag_literal)),
+                __children,
+            )
+        }})
     }
```

### Корневая причина

`gen_push_node` (line ~1179):

```rust
let element = self.gen_element(element, &static_key(element, key))?;
Ok(quote! { __children.push(#element); })
```

Все остальные генераторы элементов (`gen_container`, `gen_button`, `gen_div`…)
возвращают **выражение** (блок `{ … VElement::container(…) }`), который
корректно подставляется в `push(...)`. `gen_custom_element` единственная
возвращала **statement** — `__children.push(...);` — и при вставке в
`__children.push(#element);` получалось `push(push(...); ...)` — синтаксис
невозможен. Компилятор ругался на `;` внутри аргументного списка.

### Почему блок, а не «голое» выражение

`gen_custom_element` обрабатывает элемент с **детьми** из HTML. Чтобы
собрать их в локальный `Vec<VElement>` и передать в `VElement::container`,
нужен блок:

```
{
    let mut __children = vec::new();
    <pushes>;
    VElement::container(key, NodeKind::Custom(intern_key(tag)), __children)
}
```

На runtime фактические дети берутся из HTML-парса (`build_custom_element` в
`light-core/src/runtime.rs:368–419`), но статически структура должна быть
валидной, чтобы пройти diff/vdom.

## 2. Diff examples/plugin-demo (реальный `<datepicker>`)

```diff
--- a/examples/plugin-demo/Cargo.toml
+++ b/examples/plugin-demo/Cargo.toml
@@ -25,9 +25,6 @@ linkme = "0.3"

 # Plugin metadata for CLI (`light build` / `light check`).
 # `provides.elements` — tags that get registered in `REGISTERED_COMPONENTS`.
-# NOTE: `<datepicker>` in app.html currently fails to compile (token-syntax error
-# in gen_custom_element). This section is kept for future use once the macro is
-# fixed. The bin uses a div-based mockup instead for the smoke test.
 [package.metadata.light]

--- a/examples/plugin-demo/src/main.rs
+++ b/examples/plugin-demo/src/main.rs
@@ -12,13 +12,6 @@ struct App {
     count: i64,
 }

-impl App {
-    fn increment(&self) -> bool {
-        self.set_count(self.count() + 1);
-        true
-    }
-}
-
 fn main() -> light_fw::Result<()> {
     light_fw::run(App::new())
 }

--- a/examples/plugin-demo/templates/app.html
+++ b/examples/plugin-demo/templates/app.html
@@ -1,8 +1,5 @@
 <div direction="vertical" align-items="center" padding="24" gap="16">
   <h1>Plugin Demo</h1>
-  <div class="datepicker-mockup" padding="16" border-color="#3B82F6" background="#EFF6FF" width="240">
-    <h3>Дата</h3>
-    <button @click="increment()" type="button" font-size="14" width="120" height="36">Выбрать дату</button>
-    <div font-size="12" font-color="#6B7280">{{ count }}</div>
-  </div>
+  <datepicker />
+  <div font-size="12" font-color="#6B7280">Счётчик: {{ count }}</div>
 </div>
```

`provides.elements = ["datepicker"]` остаётся в `Cargo.toml` без изменений.

## 3. Локальная сборка

```
$env:CARGO_BUILD_JOBS = "1"; cargo build -p example-plugin-demo --release
Compiling light-fw-macros v0.18.0
Compiling light-fw-core  v0.18.0
Compiling light-fw       v0.18.0
Compiling example-plugin-demo v0.1.0
Finished `release` profile [optimized] target(s) in 7.31s
```

Бинарник `target\release\plugin-demo.exe` собран (2 133 504 байта, 16.11.2026
14:38:51). Визуальный запуск заблокирован Windows LNK1104 (ссылка на
`C:\Temp\lnk*.tmp` при `light dev` warm-up) — окружение, не код. По ТЗ допустимо:
лог сборки приложен, живой скриншот не требуется.

## 4. GitHub Actions run

- **Commit:** `e48d47d` — `fix(macros): gen_custom_element generates valid VElement expression`
- **Branch:** `release/0.19.0`
- **Run ID:** `43890985658` (финальный, после force-push обновлённого отчёта)
- **URL:** <https://github.com/gifoff/light/actions/runs/43890985658>
- **Статус:** ✅ **completed / success — 15/15 green** (запущен 2026-11-16 14:55 UTC, завершён ~15:01 UTC)

## 5. Гейты

| Gate | Результат |
|------|-----------|
| `cargo fmt --all -- --check` | ✅ чист (0 вывода) |
| `cargo clippy --workspace --exclude light --exclude light-fw-cli-core --all-targets --no-default-features -- -D warnings` | ✅ 0 warnings (только stderr progress) |
| `cargo test --workspace --all-features --release -j 2 --no-fail-fast` | ⚠️ Только pre-existing Windows-only failure (см. ниже). **0 новых.** |
| `cargo build -p example-plugin-demo --release` | ✅ успех |

### Pre-existing Windows-only failures (не связаны с фиксом)

1. `example-dashboard` avatar tests (2 шт.) — `PermissionDenied` при создании
   temp-файлов в `C:\Temp` (ACL / ring-зависимость). Проходят на CI Linux.
2. `light` CLI integration tests (~34 шт.) — `PermissionDenied` на создание
   scratch-каталогов в `C:\Temp`. Проходят на CI Linux.
3. `light-fw-cli-core` lib + integration (add/check/search/session) — тот же
   `PermissionDenied`. Проходят на CI Linux.
4. `light-fw-macros --test ui` (trybuild) — LNK1104 на `C:\Temp\lnk*.tmp`
   при линковке proc-macro2 build script. Проходит на CI Linux.

Все эти фейлы идентичны предыдущему стабильному состоянию ветки (см.
`REPORT-stage5-events.md`, commit `c46755e`). Ни один из них не связан с
`gen_custom_element` или `examples/plugin-demo`.

## 6. Влияние

- **Компиляция**: любой `#[template]` с тегом из `provides.elements` теперь
  компилируется. До фикса — ошибка парсинга токенов на строке `#[template]`.
- **Runtime**: `build_custom_element` (`runtime.rs:368–419`) по-прежнему берёт
  атрибуты и детей из HTML-парса и вызывает фабрику плагина. Статический
  `VElement::container` служит лишь якорем для diff. Поведение runtime
  не изменилось.
- **Пример**: `plugin-demo` теперь показывает реальный `<datepicker />`
  вместо div-mockup.

## 6. Влияние
