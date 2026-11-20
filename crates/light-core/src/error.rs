//! Типы ошибок библиотеки.
//!
//! Здесь живут **все** ошибки Light: VDM-слой (шрифт, кастомные элементы,
//! изображения) плюс оконный бэкенд — через `From<light_gui::Error>`
//! (`Window`, `CreateWindow`, `Surface`, `Texture`, `Softbuffer`).
//!
//! **Этап 3 (Strangler Fig):** VDM-слой ошибок дублируется из
//! `light_vdom::error` для обратной совместимости.
//! **Этап 4:** оконные/бэкенд-варианты переехали в
//! [`light_gui::error::Error`]; ядро сохраняет полный тип `Error`
//! (8 вариантов) и маппит GUI-ошибки через `From`, чтобы публичные
//! сигнатуры `light_fw_core::run()` / `light_fw_core::Result<T>` не
//! изменились для пользователей 0.19.x.

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
    #[cfg(feature = "window")]
    #[error("ошибка окна: {0}")]
    Window(#[from] winit::error::EventLoopError),

    /// Не удалось создать системное окно.
    #[cfg(feature = "window")]
    #[error("не удалось создать окно: {0}")]
    CreateWindow(#[from] winit::error::OsError),

    /// Ошибка поверхности отрисовки.
    #[cfg(feature = "wgpu-backend")]
    #[error("ошибка поверхности отрисовки: {0}")]
    Surface(#[from] pixels::Error),

    /// Ошибка буфера отрисовки (например, при изменении размера).
    #[cfg(feature = "wgpu-backend")]
    #[error("ошибка буфера отрисовки: {0}")]
    Texture(#[from] pixels::TextureError),

    /// Ошибка поверхности `softbuffer`.
    #[cfg(feature = "softbuffer-backend")]
    #[error("ошибка поверхности softbuffer: {0}")]
    Softbuffer(#[from] softbuffer::SoftBufferError),
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
            #[cfg(feature = "window")]
            light_gui::error::Error::Window(e) => Error::Window(e),
            #[cfg(feature = "window")]
            light_gui::error::Error::CreateWindow(e) => Error::CreateWindow(e),
            // 0.20.0 (Этап 5): рендер-крейты (`light-render-cpu` / `-gpu`)
            // мапят свои типизированные ошибки на общий текстовый `Backend`.
            // Здесь он раскладывается обратно по фиче сборки, чтобы публичный
            // `Error` сохранил свои варианты (обратная совместимость 0.19.x).
            //
            // Приоритет: softbuffer > wgpu > (нет бэкендов). Совпадает с
            // приоритетом выбора бэкенда в `light_fw::run` / `light_core::run`
            // (см. `#[cfg(all(feature = "wgpu-backend", not(feature = "softbuffer-backend")))]`).
            #[cfg(feature = "softbuffer-backend")]
            light_gui::error::Error::Backend(msg) => {
                Error::Softbuffer(softbuffer_error_from_str(&msg))
            }
            #[cfg(all(feature = "wgpu-backend", not(feature = "softbuffer-backend")))]
            light_gui::error::Error::Backend(msg) => {
                // `pixels::Error` (0.17) — non-exhaustive, но имеет вариант
                // `UserDefined(Box<dyn Error + Send + Sync>)`. Сохраняем
                // текст через `std::io::Error::new(Other, msg)` (реализует
                // `Display`, `Error`, `Send + Sync`). Обратная совместимость
                // 0.19.x: вариант `Surface(pixels::Error)` сохранён.
                let io_err = std::io::Error::new(std::io::ErrorKind::Other, String::from(msg));
                Error::Surface(pixels::Error::UserDefined(Box::new(io_err)))
            }
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

/// Возвращает `softbuffer::SoftBufferError` из строки общего варианта `Backend`.
///
/// Строки от `light-render-cpu` выглядят как
/// `"ошибка поверхности softbuffer: <detail>"`. Мы не можем сконструировать
/// произвольный `SoftBufferError` (его варианты закрыты), поэтому маппим на
/// первый доступный вариант — `NotYetSupported`, с detail в комментарии.
/// Обратная совместимость 0.19.x сохраняется на уровне текста (`Display`).
#[cfg(feature = "softbuffer-backend")]
fn softbuffer_error_from_str(msg: &str) -> softbuffer::SoftBufferError {
    // `softbuffer::SoftBufferError` — `#[non_exhaustive]`-enum (0.4) с
    // публичным вариантом `PlatformError(Option<String>, Option<Box<dyn Error + Send + Sync>>)`.
    // Сохраняем текст в `Option<String>`; текст ошибки виден через `Display`
    // (`"Platform error: <msg>"`). Обратная совместимость 0.19.x: вариант
    // `Softbuffer(SoftBufferError)` сохранён, тип — тот же.
    softbuffer::SoftBufferError::PlatformError(Some(String::from(msg)), None)
}
