//! Типы ошибок библиотеки.

/// Результат операций библиотеки.
pub type Result<T> = std::result::Result<T, Error>;

/// Ошибка библиотеки Light.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Не удалось загрузить шрифт для отрисовки текста.
    #[error("не удалось загрузить шрифт: {0}")]
    Font(String),

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
}
