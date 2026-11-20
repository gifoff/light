//! Бэкенд-заглушка: хранит кадр в памяти и ничего не показывает.
//!
//! Используется, когда окно собрано без графического бэкенда (headless):
//! ввод и раскладка работают, а кадр остаётся недоступным для просмотра.

use crate::Result;

use super::RendererBackend;

/// Бэкенд-заглушка: хранит кадр в памяти и ничего не показывает.
pub struct NullBackend {
    width: u32,
    height: u32,
    frame: Vec<u8>,
}

impl NullBackend {
    /// Создаёт пустой буфер кадра размером `width`×`height`.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            frame: vec![0; (width * height * 4) as usize],
        }
    }
}

impl RendererBackend for NullBackend {
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
