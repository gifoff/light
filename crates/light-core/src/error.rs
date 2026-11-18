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
            #[cfg(feature = "wgpu-backend")]
            light_gui::error::Error::Surface(e) => Error::Surface(e),
            #[cfg(feature = "wgpu-backend")]
            light_gui::error::Error::Texture(e) => Error::Texture(e),
            #[cfg(feature = "softbuffer-backend")]
            light_gui::error::Error::Softbuffer(e) => Error::Softbuffer(e),
            #[cfg(feature = "image-support")]
            light_gui::error::Error::Image(msg) => Error::Image(msg),
            #[cfg(feature = "window")]
            light_gui::error::Error::Font(msg) => Error::Font(msg),
        }
    }
}
