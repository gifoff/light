//! Процедурные макросы библиотеки Light.
//!
//! Макросы:
//! * `#[proxy]` — делает поля структуры реактивными свойствами;
//! * `#[template("...")]` — компилирует HTML-шаблон в функцию отрисовки компонента;
//! * `#[component(tag = "...")]` — регистрирует компонент как плагинный элемент
//!   (`CustomElement` + distributed-slice фабрика) на основе `#[template]` +
//!   `#[proxy]` (Plugin API, ТЗ 0.19.0 Этап 4).

#![deny(missing_docs)]

mod component;
mod provides;
mod proxy;
mod template;

/// Делает поля структуры реактивными свойствами (по аналогии с `reactive()` из Vue 3).
///
/// Для каждого поля генерируется:
/// * геттер `имя_поля()` — чтение с отслеживанием зависимостей обозревателей;
/// * сеттер `set_имя_поля(value)` — запись с уведомлением обозревателей;
/// * доступ к ячейке `имя_поля_property()` для программных привязок.
///
/// Структура автоматически реализует трейт `Proxy`, а также `Clone` и `Default`.
///
/// Значение по умолчанию берётся из `Default`, если указано поле-атрибут `#[default(выражение)]`.
///
/// ```ignore
/// #[proxy]
/// struct Counter {
///     #[default(42)]
///     count: i64,
/// }
///
/// let c = Counter::new();
/// assert_eq!(c.count(), 42);
/// c.set_count(43);
/// ```
///
/// Поля структуры — *состояние* (сохраняется/восстанавливается через
/// `inspect_state`/`inject_state`). Вычисляемые значения (computed) — методы
/// в `impl App` (`fn status(&self) -> String`): они никогда не сохраняются и
/// пересчитываются при каждом рендере; в шаблоне их указывают как `{{ status }}`
/// (ТЗ 0.14.3-fix6):
///
/// ```ignore
/// #[proxy]
/// struct App {
///     count: i64,
/// }
/// impl App {
///     fn status(&self) -> String {
///         if self.count() == 0 {
///             "Кликните «Инкремент»".to_owned()
///         } else {
///             format!("Счётчик: {}", self.count())
///         }
///     }
/// }
/// ```
///
/// Атрибут `#[proxy]` нужно располагать **выше** `#[derive(...)]` и ниже других
/// атрибутов-макросов (например, `#[template]`).
#[proc_macro_attribute]
pub fn proxy(
    attr: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    proxy::expand(attr.into(), item.into())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Компилирует HTML-шаблон в реализацию трейта `Component` для структуры.
///
/// Путь к шаблону указан относительно корня пакета (каталог `Cargo.toml`).
/// Шаблон разбирается на этапе компиляции; изменение файла шаблона
/// автоматически запускает пересборку.
///
/// ```ignore
/// #[template("templates/app.html")]
/// #[proxy]
/// struct App {
///     title: String,
/// }
/// ```
#[proc_macro_attribute]
pub fn template(
    attr: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    template::expand(attr.into(), item.into())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Регистрирует компонент как плагинный элемент (Plugin API, ТЗ 0.19.0 Этап 4).
///
/// Композиция `#[template("...")]` + `#[proxy]`: структура **обязана** иметь
/// оба атрибута — иначе `compile_error!`. Генерируется:
/// * `impl ::light_fw::plugins::CustomElement for <Тип>`: `tag()` — литерал из
///   аргумента макроса, `render` делегирует статической `Component::render`
///   (атрибуты и дочерние узлы плагина в Этапе 4 не используются);
/// * фабрика `CustomElementFactory` через
///   `#[linkme::distributed_slice(REGISTERED_COMPONENTS)]`: ядро находит её по
///   тегу при runtime-рендеринге `<тег>` из HTML.
///
/// ```ignore
/// #[component(tag = "datepicker")]
/// #[template("templates/datepicker.html")]
/// #[proxy]
/// struct Datepicker {
///     selected: Option<String>,
/// }
/// ```
///
/// Чтобы использовать `#[component]`, крейт-потребитель должен иметь
/// прямую зависимость `linkme = "0.3"` (макрос разворачивается в его коде):
///
/// ```toml
/// [dependencies]
/// light-fw = { path = "..." }
/// linkme = "0.3"
/// ```
///
/// Имя тега — латинские буквы, цифры, дефис; первая — буква. Валидируется
/// на этапе компиляции.
#[proc_macro_attribute]
pub fn component(
    attr: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    component::expand(attr.into(), item.into())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
