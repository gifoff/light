use std::collections::HashMap;

use serde::Deserialize;

use crate::manifest::{ComponentKind, Provides};

/// Встроенная таблица известных компонентов.
///
/// Файл `registry.toml` компилируется в бинарник через `include_str!`, поэтому
/// CLI работает офлайн и не требует сетевого доступа.
const REGISTRY_TOML: &str = include_str!("../registry.toml");

/// Описание одного компонента из реестра.
#[derive(Debug, Clone)]
pub struct ComponentInfo {
    /// Имя крейта-компонента (например, `relay-element-form`).
    pub name: String,
    /// Тип компонента.
    pub kind: ComponentKind,
    /// Человекочитаемое имя.
    pub display_name: String,
    /// Последняя известная версия.
    pub latest_version: semver::Version,
    /// Требование к версии ядра.
    pub core_requirement: semver::VersionReq,
    /// Что компонент предоставляет (теги, атрибуты, директивы).
    pub provides: Provides,
}

impl ComponentInfo {
    /// Строка для рекомендации CLI, например `relay-element-form ^0.3`.
    pub fn requirement(&self) -> String {
        format!("{} {}", self.name, self.core_requirement)
    }
}

/// Реестр компонентов с индексами по элементам, атрибутам и директивам.
pub struct ComponentRegistry {
    components: Vec<ComponentInfo>,
    by_name: HashMap<String, usize>,
    by_element: HashMap<String, usize>,
    by_attribute: HashMap<String, usize>,
    by_directive: HashMap<String, usize>,
}

#[derive(Debug, Deserialize)]
struct RegistryFile {
    #[serde(default)]
    components: Vec<RegistryEntry>,
}

#[derive(Debug, Deserialize)]
struct RegistryEntry {
    name: String,
    kind: ComponentKind,
    #[serde(rename = "display-name")]
    display_name: String,
    version: String,
    core: String,
    #[serde(default)]
    elements: Vec<String>,
    #[serde(default)]
    attributes: Vec<String>,
    #[serde(default)]
    directives: Vec<String>,
}

impl ComponentRegistry {
    /// Загружает встроенный реестр.
    ///
    /// Встроенный `registry.toml` корректен по построению; если бы он не
    /// разбирался, это ошибка сборки, поэтому здесь допустимо ожидать
    /// успешного разбора.
    pub fn builtin() -> Self {
        Self::from_toml(REGISTRY_TOML).expect("встроенный registry.toml должен разбираться")
    }

    /// Создаёт реестр из TOML-описания.
    pub fn from_toml(text: &str) -> crate::Result<Self> {
        let file: RegistryFile =
            toml_edit::de::from_str(text).map_err(|e| crate::Error::Manifest(e.to_string()))?;
        let mut registry = ComponentRegistry {
            components: Vec::new(),
            by_name: HashMap::new(),
            by_element: HashMap::new(),
            by_attribute: HashMap::new(),
            by_directive: HashMap::new(),
        };
        for entry in file.components {
            let latest_version = semver::Version::parse(&entry.version)
                .map_err(|e| crate::Error::Manifest(format!("invalid version: {e}")))?;
            let core_requirement = semver::VersionReq::parse(&entry.core)
                .map_err(|e| crate::Error::Manifest(format!("invalid core range: {e}")))?;
            let provides = Provides {
                elements: entry.elements,
                attributes: entry.attributes,
                directives: entry.directives,
            };
            registry.insert(ComponentInfo {
                name: entry.name,
                kind: entry.kind,
                display_name: entry.display_name,
                latest_version,
                core_requirement,
                provides,
            });
        }
        Ok(registry)
    }

    fn insert(&mut self, info: ComponentInfo) {
        let idx = self.components.len();
        for tag in &info.provides.elements {
            self.by_element.insert(tag.clone(), idx);
        }
        for attr in &info.provides.attributes {
            self.by_attribute.insert(attr.clone(), idx);
        }
        for dir in &info.provides.directives {
            self.by_directive.insert(dir.clone(), idx);
        }
        self.by_name.insert(info.name.clone(), idx);
        self.components.push(info);
    }

    /// Все компоненты, известные реестру.
    pub fn all(&self) -> &[ComponentInfo] {
        &self.components
    }

    /// Компонент по имени крейта.
    pub fn by_name(&self, name: &str) -> Option<&ComponentInfo> {
        self.by_name.get(name).map(|&i| &self.components[i])
    }

    /// Компонент, предоставляющий тег.
    pub fn component_for_element(&self, tag: &str) -> Option<&ComponentInfo> {
        self.by_element.get(tag).map(|&i| &self.components[i])
    }

    /// Компонент, предоставляющий атрибут.
    pub fn component_for_attribute(&self, attr: &str) -> Option<&ComponentInfo> {
        self.by_attribute.get(attr).map(|&i| &self.components[i])
    }

    /// Компонент, предоставляющий директиву.
    pub fn component_for_directive(&self, dir: &str) -> Option<&ComponentInfo> {
        self.by_directive.get(dir).map(|&i| &self.components[i])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_registry_is_valid() {
        let r = ComponentRegistry::builtin();
        assert!(r.all().len() >= 2);
        assert!(r.by_name("relay-element-form").is_some());
    }

    #[test]
    fn resolves_elements() {
        let r = ComponentRegistry::builtin();
        let form = r.component_for_element("checkbox").expect("checkbox");
        assert_eq!(form.name, "relay-element-form");
        assert!(
            r.component_for_element("text").is_none(),
            "текстовые поля входят в ядро и компонента не требуют"
        );
    }

    #[test]
    fn resolves_attributes_and_directives() {
        let r = ComponentRegistry::builtin();
        assert_eq!(
            r.component_for_attribute("required")
                .map(|c| c.name.as_str()),
            Some("relay-behavior-validation")
        );
        assert_eq!(
            r.component_for_directive("@validate")
                .map(|c| c.name.as_str()),
            Some("relay-behavior-validation")
        );
    }

    #[test]
    fn unknown_component_is_none() {
        let r = ComponentRegistry::builtin();
        assert!(r.component_for_element("definitely-not-real").is_none());
    }
}
