use std::path::{Path, PathBuf};

/// Имя внешнего компонента Light: `light-fw-*`, кроме ядра.
pub fn is_component_crate_name(name: &str) -> bool {
    name.starts_with("light-fw-") && name != "light-fw-core"
}

/// Вычисляет путь `target` относительно каталога `base`.
///
/// Оба пути должны быть абсолютными. Результат — относительный путь с
/// ведущими `..` при необходимости (например, `../comp`).
pub(crate) fn relative_to(base: &Path, target: &Path) -> PathBuf {
    let base: Vec<_> = base.components().collect();
    let target: Vec<_> = target.components().collect();
    let mut common = 0;
    while common < base.len() && common < target.len() && base[common] == target[common] {
        common += 1;
    }
    let mut result = PathBuf::new();
    for _ in common..base.len() {
        result.push("..");
    }
    for component in &target[common..] {
        result.push(component.as_os_str());
    }
    result
}

/// Как записать зависимость компонента в `Cargo.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DependencyEntry {
    /// `name = { workspace = true, optional = true }`
    Workspace,
    /// `name = { path = "...", optional = true }`
    Path(PathBuf),
    /// `name = { version = "...", optional = true }`
    Version(String),
}
