//! Иконка окна: пользовательский PNG или встроенная иконка Light по умолчанию.

use winit::window::Icon;

use crate::component::WindowConfig;

/// Размер встроенной иконки по умолчанию.
const DEFAULT_ICON_SIZE: u32 = 32;

/// Возвращает иконку из `WindowConfig` или встроенную иконку по умолчанию.
pub(crate) fn window_icon(config: &WindowConfig) -> Option<Icon> {
    #[cfg(feature = "image-support")]
    if let Some(data) = config.icon
        && let Ok(image) = super::image::decode_png(data)
        && let Ok(icon) = Icon::from_rgba(image.pixels, image.width, image.height)
    {
        return Some(icon);
    }
    let _ = config;
    Icon::from_rgba(default_icon_rgba(), DEFAULT_ICON_SIZE, DEFAULT_ICON_SIZE).ok()
}

/// Рисует простую иконку: скруглённый синий квадрат с белой буквой «L».
fn default_icon_rgba() -> Vec<u8> {
    let size = DEFAULT_ICON_SIZE;
    let mut pixels = vec![0_u8; (size * size * 4) as usize];
    let radius = 7.0_f32;
    for y in 0..size {
        for x in 0..size {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            let edge = rounded_edge(px, py, size as f32, radius);
            if edge <= 0.0 {
                continue;
            }
            let alpha = (edge.min(1.0) * 255.0) as u8;
            let index = ((y * size + x) * 4) as usize;
            let color = if is_letter_l(px, py, size as f32) {
                [0xFF, 0xFF, 0xFF, alpha]
            } else {
                [0x3B, 0x82, 0xF6, alpha]
            };
            pixels[index..index + 4].copy_from_slice(&color);
        }
    }
    pixels
}

fn rounded_edge(x: f32, y: f32, size: f32, radius: f32) -> f32 {
    let nearest_x = x.clamp(radius, size - radius);
    let nearest_y = y.clamp(radius, size - radius);
    let dx = x - nearest_x;
    let dy = y - nearest_y;
    let distance = (dx * dx + dy * dy).sqrt();
    (radius - distance).max(0.0)
}

fn is_letter_l(x: f32, y: f32, size: f32) -> bool {
    let stem_x = size * 0.36..size * 0.45;
    let stem_y = size * 0.28..size * 0.72;
    let foot_x = size * 0.36..size * 0.68;
    let foot_y = size * 0.63..size * 0.72;
    (stem_x.contains(&x) && stem_y.contains(&y)) || (foot_x.contains(&x) && foot_y.contains(&y))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_icon_has_visible_pixels() {
        let pixels = default_icon_rgba();
        assert_eq!(
            pixels.len(),
            (DEFAULT_ICON_SIZE * DEFAULT_ICON_SIZE * 4) as usize
        );
        assert!(pixels.as_chunks::<4>().0.iter().any(|pixel| pixel[3] > 0));
    }
}
