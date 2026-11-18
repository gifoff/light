//! Бэкенд на `pixels` (wgpu/naga): аппаратный показ программно
//! растеризованного RGBA-буфера.

use std::sync::Arc;

use pixels::{Pixels, SurfaceTexture};
use winit::window::Window;

use super::RendererBackend;
use crate::Result;

/// Бэкенд, который показывает кадр через `pixels` и `wgpu`.
pub struct WgpuBackend {
    surface: Pixels<'static>,
    width: u32,
    height: u32,
}

impl WgpuBackend {
    /// Создаёт поверхность `pixels` для окна `window`.
    pub fn new(width: u32, height: u32, window: Arc<Window>) -> Result<Self> {
        let surface_texture = SurfaceTexture::new(width, height, window);
        let surface = Pixels::new(width, height, surface_texture)?;
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
            self.surface.resize_surface(width, height)?;
            self.surface.resize_buffer(width, height)?;
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
        self.surface.render()?;
        Ok(())
    }
}
