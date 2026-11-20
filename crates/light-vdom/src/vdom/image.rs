#[cfg(feature = "image-support")]
use std::sync::Arc;
#[cfg(feature = "image-support")]
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(feature = "image-support")]
static NEXT_IMAGE_ID: AtomicU64 = AtomicU64::new(1);

/// Изображение, загруженное во время выполнения (например, из файла).
///
/// Хранит закодированный PNG и его размеры. Внутренние данные используют
/// [`Arc`], поэтому клонирование значения дешёвое — это важно для состояния
/// компонентов и повторных отрисовок.
#[cfg(feature = "image-support")]
#[derive(Clone, PartialEq)]
pub struct ImageData {
    /// Уникальный ключ изображения (путь или идентификатор).
    pub key: Arc<str>,
    /// Сырые байты PNG-файла.
    pub bytes: Arc<[u8]>,
    /// Ширина изображения в пикселях.
    pub width: u32,
    /// Высота изображения в пикселях.
    pub height: u32,
}

#[cfg(feature = "image-support")]
impl serde::Serialize for ImageData {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // Сериализуем только метаданные (размеры), не сырые байты PNG —
        // в снимок DevTools нецелесообразно включать многомегабайтные данные.
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("ImageData", 3)?;
        state.serialize_field("width", &self.width)?;
        state.serialize_field("height", &self.height)?;
        state.serialize_field("key", &*self.key)?;
        state.end()
    }
}

#[cfg(feature = "image-support")]
impl ImageData {
    /// Загружает PNG с разумными ограничениями: не более 8 МиБ и 4096 px
    /// по каждой стороне.
    pub fn from_png(bytes: Vec<u8>) -> crate::Result<Self> {
        Self::from_png_limited(bytes, 8 * 1024 * 1024, 4096)
    }

    /// Загружает PNG с явными ограничениями по размеру файла и стороне.
    ///
    /// Файл проверяется по сигнатуре и IHDR до выделения буфера декодера,
    /// затем полностью декодируется для проверки целостности.
    pub fn from_png_limited(
        bytes: Vec<u8>,
        max_bytes: usize,
        max_dimension: u32,
    ) -> crate::Result<Self> {
        if bytes.len() > max_bytes {
            return Err(crate::Error::Image(format!(
                "файл слишком большой: {} байт (максимум {max_bytes})",
                bytes.len()
            )));
        }
        const SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
        if bytes.len() < 24 || &bytes[..8] != SIGNATURE || &bytes[12..16] != b"IHDR" {
            return Err(crate::Error::Image(
                "файл не является PNG с корректным заголовком IHDR".to_owned(),
            ));
        }
        let width = u32::from_be_bytes(bytes[16..20].try_into().unwrap_or_default());
        let height = u32::from_be_bytes(bytes[20..24].try_into().unwrap_or_default());
        if width == 0 || height == 0 || width > max_dimension || height > max_dimension {
            return Err(crate::Error::Image(format!(
                "недопустимое разрешение {width}x{height} (максимум {max_dimension} px)"
            )));
        }

        let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes.as_slice()));
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = decoder
            .read_info()
            .map_err(|error| crate::Error::Image(error.to_string()))?;
        let mut buffer = vec![0; reader.output_buffer_size()];
        reader
            .next_frame(&mut buffer)
            .map_err(|error| crate::Error::Image(error.to_string()))?;

        let id = NEXT_IMAGE_ID.fetch_add(1, Ordering::Relaxed);
        Ok(Self {
            key: Arc::from(format!("runtime-image-{id}")),
            bytes: Arc::from(bytes),
            width,
            height,
        })
    }

    /// Ширина изображения в пикселях.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Высота изображения в пикселях.
    pub fn height(&self) -> u32 {
        self.height
    }
}

#[cfg(feature = "image-support")]
impl std::fmt::Debug for ImageData {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ImageData")
            .field("key", &self.key)
            .field("bytes", &self.bytes.len())
            .field("width", &self.width)
            .field("height", &self.height)
            .finish()
    }
}
