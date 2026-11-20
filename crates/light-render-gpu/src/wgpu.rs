//! Бэкенд на `pixels` (wgpu/naga): аппаратный показ программно
//! растеризованного RGBA-буфера.

use std::sync::Arc;

use pixels::{Pixels, SurfaceTexture};
use winit::window::Window;

use light_gui::Result;
use light_gui::render::RendererBackend;

/// Бэкенд, который показывает кадр через `pixels` и `wgpu`.
pub struct WgpuBackend {
    surface: Pixels<'static>,
    width: u32,
    height: u32,
}

/// Маппит ошибку `pixels` на `light_gui::Error::Backend` (0.20.0, Этап 5:
/// типизированные payload'ы вынесены из `light-gui`).
fn map_pixels(e: pixels::Error) -> light_gui::Error {
    light_gui::Error::Backend(format!("ошибка поверхности wgpu (pixels): {e}"))
}

impl WgpuBackend {
    /// Создаёт поверхность `pixels` для окна `window`.
    pub fn new(width: u32, height: u32, window: Arc<Window>) -> Result<Self> {
        let surface_texture = SurfaceTexture::new(width, height, window);
        let surface = Pixels::new(width, height, surface_texture).map_err(map_pixels)?;
        Ok(Self {
            surface,
            width,
            height,
        })
    }
}

impl RendererBackend for WgpuBackend {
    fn resize(&mut self, width: u32, height: u32) -> Result<()> {
        if width == self.width && height == self.height {
            return Ok(());
        }
        self.width = width;
        self.height = height;
        if width > 0 && height > 0 {
            // `resize_surface` / `resize_buffer` возвращают `Result<_, wgpu::Error>`
            // (pixels 0.17). Берём текст через `Display` и мапим на общий
            // текстовый вариант `Backend` (0.20.0, Этап 5).
            let map = |e: &dyn std::fmt::Display| {
                light_gui::Error::Backend(format!("ошибка wgpu (resize): {e}"))
            };
            self.surface
                .resize_surface(width, height)
                .map_err(|e| map(&e))?;
            self.surface
                .resize_buffer(width, height)
                .map_err(|e| map(&e))?;
        }
        Ok(())
    }

    fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    fn frame_mut(&mut self) -> &mut [u8] {
        self.surface.frame_mut()
    }

    fn present(&mut self) -> Result<()> {
        self.surface.render().map_err(map_pixels)?;
        Ok(())
    }
}
