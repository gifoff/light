//! Абстракция бэкенда рендеринга.
//!
//! Интерфейс уже рисуется программно: модуль `window::paint` растеризует
//! прямоугольники и текст (через `ab_glyph`) прямо в RGBA-буфер. Бэкенд
//! отвечает только за владение этим буфером и его показ в окне.
//!
//! Конкретные реализации вынесены из `light-gui` в 0.20.0 (Этап 5):
//! * `softbuffer` — [`light-render-cpu`](https://crates.io/crates/light-render-cpu);
//! * `pixels`/wgpu — [`light-render-gpu`](https://crates.io/crates/light-render-gpu).
//!
//! Этот модуль содержит только **интерфейс** ([`RendererBackend`]), тип
//! результата ([`Result`]) и заглушку [`NullBackend`] для headless-режима.
//! Выбор конкретной реализации делает фасад `light-fw` (функция `run`),
//! который передаёт готовый бэкенд в [`crate::run_with_backend`].

pub mod null;

use crate::window::paint::Canvas;
use light_vdom::layout::{Point, Rect};

/// Результат операций бэкенда рендеринга (обёртка над `crate::error::Result`),
/// переиспользуемая во всех реализациях [`RendererBackend`] и во внешних
/// крейтах (`light-render-cpu`/`light-render-gpu`), которые не зависят
/// от ошибок GUI-слоя напрямую.
pub type Result<T = ()> = crate::error::Result<T>;

/// Цвет в формате `0xRRGGBBAA`.
pub type Color = u32;

/// Низкоуровневый бэкенд окна.
///
/// Реализация владеет RGBA-буфером кадра и поверхностью показа. Методы
/// отрисовки по умолчанию работают через программный `Canvas`, поэтому
/// любой бэкенд получает одинаковую растеризацию без шейдеров и атласов.
pub trait RendererBackend {
    /// Меняет размер буфера и поверхности (в физических пикселях).
    fn resize(&mut self, width: u32, height: u32) -> Result<()>;

    /// Текущий размер буфера в пикселях.
    fn size(&self) -> (u32, u32);

    /// RGBA8-буфер кадра (4 байта на пиксель).
    fn frame_mut(&mut self) -> &mut [u8];

    /// Показывает готовый кадр.
    fn present(&mut self) -> Result<()>;

    /// Заливает весь кадр цветом `color` (замена пикселей).
    fn clear(&mut self, color: Color) {
        let (width, height) = self.size();
        let mut canvas = Canvas::new(
            self.frame_mut(),
            width,
            height,
            Rect::new(0.0, 0.0, width as f32, height as f32),
        );
        canvas.clear(color);
    }

    /// Заливает прямоугольник `rect` цветом `color`.
    fn draw_rect(&mut self, rect: Rect, color: Color) {
        let (width, height) = self.size();
        let mut canvas = Canvas::new(self.frame_mut(), width, height, rect);
        canvas.fill_rect(color, rect);
    }

    /// Рисует строку `text`; `pos` — левый верх типографской строки.
    ///
    /// Размер шрифта и сам шрифт передаются явно: текст растеризуется
    /// программно, без texture atlas и шейдеров.
    fn draw_text(
        &mut self,
        font: &crate::window::font::FontMetrics,
        text: &str,
        pos: Point,
        color: Color,
        size: f32,
    ) {
        let (width, height) = self.size();
        let mut canvas = Canvas::new(
            self.frame_mut(),
            width,
            height,
            Rect::new(0.0, 0.0, width as f32, height as f32),
        );
        canvas.draw_text(font, text, pos.x, pos.y, color, size);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MemoryBackend {
        width: u32,
        height: u32,
        frame: Vec<u8>,
    }

    impl MemoryBackend {
        fn new(width: u32, height: u32) -> Self {
            Self {
                width,
                height,
                frame: vec![0; (width * height * 4) as usize],
            }
        }

        fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
            let index = (y * self.width + x) as usize * 4;
            self.frame[index..index + 4].try_into().expect("pixel")
        }
    }

    impl RendererBackend for MemoryBackend {
        fn resize(&mut self, width: u32, height: u32) -> Result<()> {
            self.width = width;
            self.height = height;
            self.frame = vec![0; (width * height * 4) as usize];
            Ok(())
        }

        fn size(&self) -> (u32, u32) {
            (self.width, self.height)
        }

        fn frame_mut(&mut self) -> &mut [u8] {
            &mut self.frame
        }

        fn present(&mut self) -> Result<()> {
            Ok(())
        }
    }

    #[test]
    fn default_clear_fills_the_frame() {
        let mut backend = MemoryBackend::new(3, 2);
        backend.clear(0x11_22_33_FF);
        for y in 0..2 {
            for x in 0..3 {
                assert_eq!(backend.pixel(x, y), [0x11, 0x22, 0x33, 0xFF]);
            }
        }
    }

    #[test]
    fn default_draw_rect_only_touches_the_rect() {
        let mut backend = MemoryBackend::new(4, 4);
        backend.clear(0x11_22_33_FF);
        backend.draw_rect(Rect::new(1.0, 1.0, 2.0, 2.0), 0xFF_00_00_FF);
        assert_eq!(backend.pixel(1, 1), [0xFF, 0x00, 0x00, 0xFF]);
        assert_eq!(backend.pixel(2, 2), [0xFF, 0x00, 0x00, 0xFF]);
        assert_eq!(backend.pixel(0, 0), [0x11, 0x22, 0x33, 0xFF]);
        assert_eq!(backend.pixel(3, 3), [0x11, 0x22, 0x33, 0xFF]);
    }
}
