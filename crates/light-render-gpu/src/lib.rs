//! # Light Render (GPU)
//!
//! Аппаратный показ программно растеризованного RGBA-кадра через `pixels`
//! (wgpu/naga). Бэкенд [`WgpuBackend`] реализует трейт
//! [`light_gui::render::RendererBackend`], вынесенный из `light-gui` в 0.20.0
//! (Этап 5), чтобы GUI-слой не зависел от `pixels`.
//!
//! Зависимость `light-gui` — только ради интерфейса
//! [`light_gui::render::RendererBackend`]: цикла нет, т.к. `light-gui` НЕ
//! зависит от `light-render-gpu`.
//!
//! Модуль существует только при фиче `window` (нужно окно `winit` для
//! поверхности `pixels`).

#[cfg(feature = "window")]
pub mod wgpu;
#[cfg(feature = "window")]
pub use wgpu::WgpuBackend;

#[cfg(test)]
mod tests {
    #[test]
    fn crate_is_empty_until_window_feature() {
        // 0.20.0 (Этап 5): публичный API крейта — только `WgpuBackend`
        // под фичей `window`. Без фичи крейт не экспортирует ничего; тест
        // фиксирует, что пустой крейт компилируется и линтуется.
        assert!(true);
    }
}
