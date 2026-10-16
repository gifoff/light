use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ignore::WalkBuilder;
use relay_html::Node;

use crate::error::{Error, Result};

/// Результат сканирования шаблонов.
#[derive(Debug, Default)]
pub struct ScanResult {
    /// Использования тегов: имя → количество.
    pub tags: BTreeMap<String, usize>,
    /// Использования атрибутов: имя → количество.
    pub attributes: BTreeMap<String, usize>,
    /// Просканированные файлы шаблонов.
    pub files: Vec<PathBuf>,
    /// Ошибки разбора шаблонов.
    pub errors: Vec<ScanError>,
}

/// Ошибка разбора отдельного шаблона.
#[derive(Debug)]
pub struct ScanError {
    /// Путь к файлу.
    pub file: PathBuf,
    /// Сообщение парсера.
    pub message: String,
}

/// Сканирует `*.html`-шаблоны под `root`.
///
/// Обход учитывает `.gitignore`/`.ignore`, пропускает скрытые каталоги
/// и каталог `target`. Ничего не пишет на диск.
pub fn scan_templates(root: &Path) -> Result<ScanResult> {
    let mut result = ScanResult::default();
    let walk = WalkBuilder::new(root)
        .hidden(true)
        .filter_entry(|entry| entry.file_name() != "target")
        .build();
    for entry in walk {
        let entry = entry.map_err(Error::Ignore)?;
        let path = entry.path();
        if !path.is_file() || path.extension().is_none_or(|ext| ext != "html") {
            continue;
        }
        let content = std::fs::read_to_string(path)?;
        match relay_html::parse(&content) {
            Ok(nodes) => {
                collect(&nodes, &mut result.tags, &mut result.attributes);
                result.files.push(path.to_path_buf());
            }
            Err(message) => result.errors.push(ScanError {
                file: path.to_path_buf(),
                message,
            }),
        }
    }
    result.files.sort();
    Ok(result)
}

fn collect(
    nodes: &[Node],
    tags: &mut BTreeMap<String, usize>,
    attributes: &mut BTreeMap<String, usize>,
) {
    for node in nodes {
        if let Node::Element(element) = node {
            *tags.entry(element.name.clone()).or_insert(0) += 1;
            for attr in &element.attrs {
                *attributes.entry(attr.name.clone()).or_insert(0) += 1;
            }
            collect(&element.children, tags, attributes);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("relay-cli-scan-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn counts_tags_and_attributes() {
        let dir = temp_dir("counts");
        fs::write(
            dir.join("a.html"),
            "<div><text v-model=\"x\"></text><checkbox required></checkbox></div>",
        )
        .expect("write template");
        let result = scan_templates(&dir).expect("scan");
        assert_eq!(result.files.len(), 1);
        assert_eq!(result.tags.get("text"), Some(&1));
        assert_eq!(result.tags.get("checkbox"), Some(&1));
        assert_eq!(result.attributes.get("v-model"), Some(&1));
        assert_eq!(result.attributes.get("required"), Some(&1));
        assert!(result.errors.is_empty());
    }

    #[test]
    fn reports_parse_errors() {
        let dir = temp_dir("errors");
        fs::write(dir.join("broken.html"), "<div><text>не закрыт</div>").expect("write template");
        let result = scan_templates(&dir).expect("scan");
        assert!(result.files.is_empty());
        assert_eq!(result.errors.len(), 1);
    }

    #[test]
    fn ignores_non_html_files() {
        let dir = temp_dir("non-html");
        fs::write(dir.join("readme.txt"), "<div></div>").expect("write file");
        let result = scan_templates(&dir).expect("scan");
        assert!(result.files.is_empty());
    }
}
