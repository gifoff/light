//! Декодирование PNG-изображений в RGBA8 и кэш по пути источника.
//!
//! Изображения встраиваются в бинарник макросом `#[template]`, поэтому ключом
//! кэша служит статическая строка `src` из атрибута `<image src="...">`.

use std::collections::HashMap;
use std::io::Cursor;
use std::sync::{Arc, Mutex, OnceLock};

/// Декодированное изображение в формате RGBA8.
pub(crate) struct DecodedImage {
    /// Ширина в пикселях.
    pub width: u32,
    /// Высота в пикселях.
    pub height: u32,
    /// Пиксели: четыре байта (R, G, B, A) на точку, построчно.
    pub pixels: Vec<u8>,
}

static CACHE: OnceLock<Mutex<HashMap<String, Arc<DecodedImage>>>> = OnceLock::new();

/// Возвращает изображение из кэша, декодируя его при первом обращении.
pub(crate) fn load(src: &str, data: &[u8]) -> Option<Arc<DecodedImage>> {
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(cache) = cache.lock()
        && let Some(image) = cache.get(src)
    {
        return Some(Arc::clone(image));
    }
    let image = Arc::new(decode_png(data).ok()?);
    if let Ok(mut cache) = cache.lock() {
        cache.insert(src.to_owned(), Arc::clone(&image));
    }
    Some(image)
}

/// Возвращает runtime-изображение из кэша по его уникальному ключу.
pub(crate) fn load_runtime(image: &crate::vdom::ImageData) -> Option<Arc<DecodedImage>> {
    load(&image.key, &image.bytes)
}

/// Декодирует PNG в RGBA8.
pub(crate) fn decode_png(data: &[u8]) -> crate::Result<DecodedImage> {
    let mut decoder = png::Decoder::new(Cursor::new(data));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder
        .read_info()
        .map_err(|error| crate::Error::Image(error.to_string()))?;
    let mut buffer = vec![0; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut buffer)
        .map_err(|error| crate::Error::Image(error.to_string()))?;
    let width = info.width;
    let height = info.height;
    let pixels = match info.color_type {
        png::ColorType::Rgba => buffer[..info.buffer_size()].to_vec(),
        png::ColorType::Rgb => {
            let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
            let (pixels, _) = buffer[..info.buffer_size()].as_chunks::<3>();
            for pixel in pixels {
                rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 0xFF]);
            }
            rgba
        }
        png::ColorType::Grayscale => {
            let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
            for &value in &buffer[..info.buffer_size()] {
                rgba.extend_from_slice(&[value, value, value, 0xFF]);
            }
            rgba
        }
        png::ColorType::GrayscaleAlpha => {
            let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
            let (pixels, _) = buffer[..info.buffer_size()].as_chunks::<2>();
            for pixel in pixels {
                rgba.extend_from_slice(&[pixel[0], pixel[0], pixel[0], pixel[1]]);
            }
            rgba
        }
        png::ColorType::Indexed => {
            return Err(crate::Error::Image(String::from(
                "индексированный PNG не был преобразован в RGBA",
            )));
        }
    };
    Ok(DecodedImage {
        width,
        height,
        pixels,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode_png(width: u32, height: u32, pixels: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, width, height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().expect("png header");
            writer.write_image_data(pixels).expect("png data");
        }
        out
    }

    #[test]
    fn decodes_rgba_png() {
        let pixels = [255, 0, 0, 255, 0, 255, 0, 128];
        let data = encode_png(2, 1, &pixels);
        let image = decode_png(&data).expect("png decoded");
        assert_eq!((image.width, image.height), (2, 1));
        assert_eq!(image.pixels, pixels);
    }

    #[test]
    fn decodes_rgb_png_to_rgba() {
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, 1, 1);
            encoder.set_color(png::ColorType::Rgb);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().expect("png header");
            writer.write_image_data(&[10, 20, 30]).expect("png data");
        }
        let image = decode_png(&out).expect("png decoded");
        assert_eq!(image.pixels, [10, 20, 30, 255]);
    }
}
