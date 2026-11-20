//! Ошибки GUI-слоя Light (0.20.0, Этап 4).
//!
//! Сюда переехали оконные/бэкенд-варианты `light_fw_core::error::Error`
//! (`Window`, `CreateWindow`, …) и `Image`. В 0.20.0 (Этап 5) бэкенды
//! (`softbuffer`, `pixels`) вынесены из `light-gui` в `light-render-cpu` /
//! `light-render-gpu`, поэтому их типизированные payload'ы (`Surface`,
//! `Texture`, `Softbuffer`) больше НЕ нужны здесь: конкретные реализации сами
//! маппят свои ошибки на [`Error::Backend`] (общий текстовый вариант), а
//! публичный тип ошибок приложения (`light-fw-core::Error`) сохраняет
//! типизированные варианты для обратной совместимости 0.19.x.

/// Результат операций GUI-слоя.
pub type Result<T> = std::result::Result<T, Error>;

/// Ошибка GUI-слоя Light (окно, отрисовка, изображения, диалоги).
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

    /// Ошибка рендер-бэкенда (поверхность / буфер). Общий текстовый вариант:
    /// 0.20.0 (Этап 5) конкретные payload'ы (`softbuffer::SoftBufferError`,
    /// `pixels::Error`) живут в `light-render-cpu` / `light-render-gpu`,
    /// которые мапят их сюда.
    #[error("{0}")]
    Backend(String),

    /// Не удалось декодировать PNG-изображение.
    #[cfg(feature = "image-support")]
    #[error("не удалось декодировать изображение: {0}")]
    Image(String),

    /// Не удалось загрузить системный шрифт или файл повреждён.
    #[cfg(feature = "window")]
    #[error("{0}")]
    Font(String),
}
