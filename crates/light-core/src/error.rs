//! Типы ошибок библиотеки.
//!
//! Здесь живут **все** ошибки Light: VDM-слой (шрифт, кастомные элементы,
//! изображения) плюс оконный бэкенд — через `From<light_gui::Error>`.
//!
//! **Этап 3 (Strangler Fig):** VDM-слой ошибок дублируется из
//! `light_vdom::error` для обратной совместимости.
//! **Этап 4:** оконные/бэкенд-варианты переехали в
//! [`light_gui::error::Error`]; ядро сохраняет полный тип `Error`
//! и маппит GUI-ошибки через `From`, чтобы публичные
//! сигнатуры `light_fw_core::run()` / `light_fw_core::Result<T>` не
//! изменились для пользователей 0.19.x.
//!
//! **Этап 6 (0.20.0): Headless по умолчанию.** Оконные/бэкенд-варианты
//! (`Window`, `CreateWindow`, `Surface`, `Texture`, `Softbuffer`) теперь
//! хранят текст ошибки (`String`), а не типизированные payload'ы
//! (`winit::error::*`, `pixels::*`, `softbuffer::SoftBufferError`). Это
//! позволяет ядру оставаться без зависимостей от `winit`/`pixels`/
//! `softbuffer` в headless-сборке; обратная совместимость 0.19.x на уровне
//! текста (`Display`) сохраняется.

/// Результат операций библиотеки.
pub type Result<T> = std::result::Result<T, Error>;

/// Ошибка библиотеки Light.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Не удалось загрузить шрифт для отрисовки текста.
    #[error("не удалось загрузить шрифт: {0}")]
    Font(String),

    /// В шаблоне найден тег кастомного элемента, который не зарегистрирован
    /// ни одним подключённым плагином (Plugin API, Этап 3).
    #[error("не зарегистрирован кастомный элемент: {0}")]
    UnregisteredCustomElement(String),

    /// Не удалось декодировать PNG-изображение.
    #[cfg(feature = "image-support")]
    #[error("не удалось декодировать изображение: {0}")]
    Image(String),

    /// Системная ошибка цикла событий или окна (переехало из `light-gui`,
    /// этап 4: здесь маппится из `light_gui::error::Error::Window`).
    /// 0.20.0 (Этап 6): payload заменён на `String` — ядро не зависит от
    /// `winit` (Headless by default); текст ошибки сохранён.
    #[cfg(feature = "window")]
    #[error("ошибка окна: {0}")]
    Window(String),

    /// Не удалось создать системное окно.
    /// 0.20.0 (Этап 6): payload заменён на `String` (см. [`Self::Window`]).
    #[cfg(feature = "window")]
    #[error("не удалось создать окно: {0}")]
    CreateWindow(String),

    /// Ошибка поверхности отрисовки (wgpu/pixels).
    /// 0.20.0 (Этап 6): payload заменён на `String` — ядро не зависит от
    /// `pixels` (Headless by default); текст ошибки сохранён.
    #[cfg(feature = "wgpu-backend")]
    #[error("ошибка поверхности отрисовки: {0}")]
    Surface(String),

    /// Ошибка буфера отрисовки (например, при изменении размера).
    /// 0.20.0 (Этап 6): payload заменён на `String` (см. [`Self::Window`]).
    #[cfg(feature = "wgpu-backend")]
    #[error("ошибка буфера отрисовки: {0}")]
    Texture(String),

    /// Ошибка поверхности `softbuffer`.
    /// 0.20.0 (Этап 6): payload заменён на `String` — ядро не зависит от
    /// `softbuffer` (Headless by default); текст ошибки сохранён.
    #[cfg(feature = "softbuffer-backend")]
    #[error("ошибка поверхности softbuffer: {0}")]
    Softbuffer(String),
}

impl From<light_vdom::error::Error> for Error {
    fn from(err: light_vdom::error::Error) -> Self {
        match err {
            light_vdom::error::Error::Font(msg) => Error::Font(msg),
            light_vdom::error::Error::UnregisteredCustomElement(msg) => {
                Error::UnregisteredCustomElement(msg)
            }
            #[cfg(feature = "image-support")]
            light_vdom::error::Error::Image(msg) => Error::Image(msg),
        }
    }
}

// Этап 4 (Strangler Fig): GUI-ошибки из `light-gui` маппятся в ядро.
// Без фичи `window` у `light_gui::error::Error` нет оконных вариантов,
// а `Image` — только при `image-support` — поэтому маппинг покрывает
// все варианты, которые реально встречаются.
#[cfg(any(feature = "window", feature = "image-support"))]
impl From<light_gui::error::Error> for Error {
    fn from(err: light_gui::error::Error) -> Self {
        match err {
            // 0.20.0 (Этап 6): payload'ы `winit::error::*` заменены на
            // `String` — ядро не зависит от `winit` (Headless by default).
            // Обратная совместимость 0.19.x: вариант и текст (`Display`)
            // сохранены.
            #[cfg(feature = "window")]
            light_gui::error::Error::Window(e) => Error::Window(e.to_string()),
            #[cfg(feature = "window")]
            light_gui::error::Error::CreateWindow(e) => Error::CreateWindow(e.to_string()),
            // 0.20.0 (Этап 5): рендер-крейты (`light-render-cpu` / `-gpu`)
            // мапят свои типизированные ошибки на общий текстовый `Backend`.
            // Здесь он раскладывается обратно по фиче сборки, чтобы публичный
            // `Error` сохранил свои варианты (обратная совместимость 0.19.x).
            //
            // Приоритет: softbuffer > wgpu > (нет бэкендов). Совпадает с
            // приоритетом выбора бэкенда в `light_fw::run` / `light_core::run`
            // (см. `#[cfg(all(feature = "wgpu-backend", not(feature = "softbuffer-backend")))]`).
            // 0.20.0 (Этап 6): payload'ы `softbuffer`/`pixels` заменены на
            // `String` — ядро не зависит от этих крейтов (Headless by default).
            // Обратная совместимость 0.19.x: варианты и текст (`Display`)
            // сохранены.
            #[cfg(feature = "softbuffer-backend")]
            light_gui::error::Error::Backend(msg) => Error::Softbuffer(msg),
            #[cfg(all(feature = "wgpu-backend", not(feature = "softbuffer-backend")))]
            light_gui::error::Error::Backend(msg) => Error::Surface(msg),
            // 0.20.0 (Этап 5): ни одного бэкенда — `Backend` на практике не
            // создаётся, но закрываем матч (вариант существует в `light_gui::Error`).
            // Фолбэк — `Font(String)`: единственный вариант `Error`, доступный
            // без фич (`image-support`/`window`/`*-backend`); текст ошибки
            // сохраняется в поле.
            #[cfg(all(not(feature = "softbuffer-backend"), not(feature = "wgpu-backend")))]
            light_gui::error::Error::Backend(msg) => Error::Font(msg),
            #[cfg(feature = "image-support")]
            light_gui::error::Error::Image(msg) => Error::Image(msg),
            #[cfg(feature = "window")]
            light_gui::error::Error::Font(msg) => Error::Font(msg),
        }
    }
}
