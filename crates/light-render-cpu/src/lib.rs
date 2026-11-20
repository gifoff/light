//! # Light Render (CPU)
//!
//! Программная отрисовка и показ кадра через `softbuffer` (без wgpu/naga).
//! Бэкенд [`SoftbufferBackend`] реализует трейт [`light_gui::render::RendererBackend`],
//! вынесенный из `light-gui` в 0.20.0 (Этап 5), чтобы GUI-слой не зависел
//! от `softbuffer`.
//!
//! Зависимость `light-gui` — только ради интерфейса [`light_gui::render::RendererBackend`]:
//! цикла нет, т.к. `light-gui` НЕ зависит от `light-render-cpu`.
//!
//! Модуль существует только при фиче `window` (нужно окно `winit` для
//! поверхности `softbuffer`).

#[cfg(feature = "window")]
pub mod softbuffer;
#[cfg(feature = "window")]
pub use softbuffer::SoftbufferBackend;

#[cfg(test)]
mod tests {
    #[test]
    fn crate_is_empty_until_window_feature() {
        // 0.20.0 (Этап 5): публичный API крейта — только `SoftbufferBackend`
        // под фичей `window`. Без фичи крейт не экспортирует ничего; тест
        // фиксирует, что пустой крейт компилируется и линтуется.
        assert!(true);
    }
}
