#[derive(Clone)]
pub(super) struct WindowGeometry {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub maximized: bool,
    /// ТЗ 0.14.3-fix7: флаг «окно было свёрнуто» при записи. Применяется только
    /// как диагностика — restore никогда НЕ ставит окно в minimized (это ломает
    /// thumbnail), поэтому при чтении поле читается, но на позицию не влияет.
    /// `Deserialize` кастомный (без derive) и полагается на отсутствие поля →
    /// по умолчанию `false`; явный `default`-атрибут здесь не нужен.
    pub minimized: bool,
}

impl WindowGeometry {
    /// ТЗ 0.14.3-fix7: мусорные координаты свёрнутого окна (Windows кладёт
    /// minimized-окно за левую границу экрана, например `(-32000, -32000)`).
    /// Такую геометрию нельзя применять — окно будет невидимым, а thumbnail
    /// пустым.
    pub(super) fn is_garbage(&self) -> bool {
        self.x < -10000 || self.y < -10000
    }

    /// Путь файла геометрии (`LIGHT_WINDOW_FILE` или `.light/window.json`).
    pub(super) fn file_path() -> std::path::PathBuf {
        std::env::var_os("LIGHT_WINDOW_FILE")
            .map(std::path::PathBuf::from)
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| std::path::PathBuf::from(".light/window.json"))
    }

    /// Читает геометрию из файла или переменной окружения `LIGHT_WINDOW_INIT`.
    /// Возвращает `None`, если ни один источник не дал данных.
    pub(super) fn load() -> Option<Self> {
        // Переменная окружения имеет приоритет над файлом.
        if let Ok(json) = std::env::var("LIGHT_WINDOW_INIT")
            && let Ok(geom) = serde_json::from_str::<Self>(&json)
        {
            return Some(geom);
        }
        let path = Self::file_path();
        let raw = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str(&raw).ok()
    }

    /// Записывает геометрию в файл (создаёт каталог, если нужно).
    pub(super) fn save(&self) -> std::io::Result<()> {
        let path = Self::file_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(
            &path,
            serde_json::to_string_pretty(self).map_err(std::io::Error::other)?,
        )
    }
}

impl<'de> serde::Deserialize<'de> for WindowGeometry {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error as _;
        let obj = serde_json::Value::deserialize(deserializer)?;
        let get = |key: &str| obj.get(key);
        let x = get("x").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
        let y = get("y").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
        let width = get("width").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        let height = get("height").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        let maximized = get("maximized").and_then(|v| v.as_bool()).unwrap_or(false);
        let minimized = get("minimized").and_then(|v| v.as_bool()).unwrap_or(false);
        if width == 0 || height == 0 {
            return Err(D::Error::custom("width/height must be non-zero"));
        }
        Ok(Self {
            x,
            y,
            width,
            height,
            maximized,
            minimized,
        })
    }
}

impl serde::Serialize for WindowGeometry {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("WindowGeometry", 6)?;
        state.serialize_field("x", &self.x)?;
        state.serialize_field("y", &self.y)?;
        state.serialize_field("width", &self.width)?;
        state.serialize_field("height", &self.height)?;
        state.serialize_field("maximized", &self.maximized)?;
        state.serialize_field("minimized", &self.minimized)?;
        state.end()
    }
}



