//! Типы ошибок библиотеки (VDOM-слой).
//!
//! Здесь живут ошибки, не привязанные к оконному бэкенду: шрифт,
//! кастомные элементы и декодирование изображений. Ошибки бэкендов
//! (`Window`, `CreateWindow`, `Surface`, `Texture`, `Softbuffer`) остались
//! в `light-fw-core::error` (этап 3 — Strangler Fig): они требуют
//! `winit`/`pixels`/`softbuffer`, которых нет в этом крейте.

/// Результат операций VDOM-слоя.
pub type Result<T> = std::result::Result<T, Error>;

/// Ошибка VDOM-слоя Light.
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
}
