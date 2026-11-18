//! Ошибки GUI-слоя Light (0.20.0, Этап 4).
//!
//! Сюда переехали оконные/бэкенд-варианты `light_fw_core::error::Error`
//! (`Window`, `CreateWindow`, `Surface`, `Texture`, `Softbuffer`) и `Image`,
//! которые раньше жили в `light-fw-core`. Остальные варианты
//! (`Font`, `UnregisteredCustomElement`) остаются в `light_vdom::error::Error`
//! и `light_fw_core::error::Error` — световое ядро их реэкспортирует для
//! обратной совместимости, а `light-fw-core` реализует `From<light_gui::Error>`
//! для своих пользователей.

/// Результат операций GUI-слоя.
pub type Result<T> = std::result::Result<T, Error>;

/// Ошибка GUI-слоя Light (окно, бэкенды отрисовки, изображения, диалоги).
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Системная ошибка цикла событий или окна.
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

    /// Не удалось декодировать PNG-изображение.
    #[cfg(feature = "image-support")]
    #[error("не удалось декодировать изображение: {0}")]
    Image(String),

    /// Не удалось загрузить системный шрифт или файл повреждён.
    #[cfg(feature = "window")]
    #[error("{0}")]
    Font(String),
}
