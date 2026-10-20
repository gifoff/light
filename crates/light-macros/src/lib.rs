//! Процедурные макросы библиотеки Light.
//!
//! Макросы:
//! * `#[proxy]` — делает поля структуры реактивными свойствами;
//! * `#[template("...")]` — компилирует HTML-шаблон в функцию отрисовки компонента.

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
