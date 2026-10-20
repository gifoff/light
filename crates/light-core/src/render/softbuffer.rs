//! Бэкенд показа кадра через `softbuffer` (без wgpu/naga).

use std::num::NonZeroU32;
use std::sync::Arc;

use softbuffer::{Context, Surface};
use winit::window::Window;

use super::RendererBackend;
use crate::Result;

/// Бэкенд, который отдаёт RGBA-кадр в `softbuffer`.
///
/// `softbuffer` принимает пиксели в формате `0x00RRGGBB`, поэтому альфа при
/// показе отбрасывается: окно непрозрачное. Внутренний буфер остаётся RGBA8,
/// чтобы программная растеризация не зависела от бэкенда.
pub struct SoftbufferBackend {
    // Поля уничтожаются сверху вниз: поверхность должна уйти раньше контекста.
    surface: Surface<Arc<Window>, Arc<Window>>,
    _context: Context<Arc<Window>>,
    frame: Vec<u8>,
    width: u32,
    height: u32,
}

impl SoftbufferBackend {
    /// Создаёт поверхность для окна `window` размером `width`×`height`.
    pub fn new(width: u32, height: u32, window: Arc<Window>) -> Result<Self> {
        let context = Context::new(Arc::clone(&window))?;
        let mut surface = Surface::new(&context, window)?;
        resize_surface(&mut surface, width, height)?;
        Ok(Self {
            surface,
            _context: context,
            frame: vec![0; frame_len(width, height)],
            width,
            height,
        })
    }
}

impl RendererBackend for SoftbufferBackend {
    fn resize(&mut self, width: u32, height: u32) -> Result<()> {
        if (width, height) == (self.width, self.height) {
            return Ok(());
        }
        resize_surface(&mut self.surface, width, height)?;
        self.width = width;
        self.height = height;
        self.frame = vec![0; frame_len(width, height)];
        Ok(())
    }

    fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    fn frame_mut(&mut self) -> &mut [u8] {
        &mut self.frame
    }

    fn present(&mut self) -> Result<()> {
        if self.width == 0 || self.height == 0 {
            return Ok(());
        }
        let mut buffer = self.surface.buffer_mut()?;
        pack_rgb(&self.frame, &mut buffer);
        buffer.present()?;
        Ok(())
    }
}

fn frame_len(width: u32, height: u32) -> usize {
    (width as usize)
        .saturating_mul(height as usize)
        .saturating_mul(4)
}

fn resize_surface(
    surface: &mut Surface<Arc<Window>, Arc<Window>>,
    width: u32,
    height: u32,
) -> Result<()> {
    if let (Some(width), Some(height)) = (NonZeroU32::new(width), NonZeroU32::new(height)) {
        surface.resize(width, height)?;
    }
    Ok(())
}

/// Перекладывает RGBA8 в `0x00RRGGBB`, который ожидает `softbuffer`.
fn pack_rgb(frame: &[u8], buffer: &mut [u32]) {
    for (pixel, rgba) in buffer.iter_mut().zip(frame.as_chunks::<4>().0) {
        *pixel = (u32::from(rgba[0]) << 16) | (u32::from(rgba[1]) << 8) | u32::from(rgba[2]);
    }
}

#[cfg(test)]
mod tests {
    use super::pack_rgb;

    #[test]
    fn pack_rgb_matches_softbuffer_layout() {
        let frame = [0x12, 0x34, 0x56, 0xFF, 0xAB, 0xCD, 0xEF, 0x80];
        let mut buffer = [0u32; 2];
        pack_rgb(&frame, &mut buffer);
        assert_eq!(buffer, [0x00_12_34_56, 0x00_AB_CD_EF]);
    }
}
