//! Иконка окна: пользовательский PNG или встроенная иконка Light по умолчанию.

use winit::window::Icon;

use crate::component::WindowConfig;

/// Встроенная иконка Light (128×128, сырые RGBA-байты: синий скруглённый
/// квадрат с белым шевроном). Формат совпадает с тем, что принимает
/// [`Icon::from_rgba`] — `R,G,B,A` по 4 байта на пиксель, строки подряд.
static DEFAULT_ICON: &[u8] = include_bytes!("../../assets/default_icon.rgba");

/// Размер встроенной иконки по умолчанию в пикселях.
const DEFAULT_ICON_SIZE: u32 = 128;

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
    default_icon()
}

/// Встроенная иконка Light: превращает сырые RGBA-байты в [`Icon`].
fn default_icon() -> Option<Icon> {
    Icon::from_rgba(DEFAULT_ICON.to_vec(), DEFAULT_ICON_SIZE, DEFAULT_ICON_SIZE).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_icon_bytes_are_128x128_rgba() {
        // Сырые байты должны точно соответствовать 128*128*4 байта (RGBA).
        assert_eq!(
            DEFAULT_ICON.len(),
            (DEFAULT_ICON_SIZE * DEFAULT_ICON_SIZE * 4) as usize
        );
    }

    #[test]
    fn default_icon_decodes_into_winit_icon() {
        // Байты корректно интерпретируются как 128x128 RGBA → winit Icon.
        let icon = default_icon().expect("embedded icon must decode into a winit Icon");
        let _ = icon; // Icon не даёт доступа к байтам, но from_rgba вернул Some.
    }

    #[test]
    fn default_icon_corner_is_transparent() {
        // Пиксель в самом углу (0,0) — прозрачный (скруглённый угол).
        assert_eq!(DEFAULT_ICON[3], 0);
    }

    #[test]
    fn default_icon_center_is_opaque() {
        // Центральный пиксель — непрозрачный (белая часть шеврона).
        let center = ((64 * DEFAULT_ICON_SIZE + 64) * 4) as usize;
        assert_eq!(DEFAULT_ICON[center + 3], 255);
    }

    #[test]
    fn default_icon_has_visible_and_invisible_pixels() {
        // Краткая проверка: среди 128*128 пикселей есть и прозрачные (углы),
        // и непрозрачные (центр) — это значит, маска сработала корректно.
        let mut has_opaque = false;
        let mut has_transparent = false;
        for i in (0..DEFAULT_ICON.len()).step_by(4) {
            if DEFAULT_ICON[i + 3] > 0 {
                has_opaque = true;
            } else {
                has_transparent = true;
            }
        }
        assert!(
            has_opaque && has_transparent,
            "icon must have both opaque and transparent pixels"
        );
    }
}
