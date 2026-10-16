use std::collections::BTreeSet;

use crate::registry::ComponentRegistry;
use crate::scan::ScanResult;

/// Компонент, необходимый проекту (используется в шаблонах).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequiredComponent {
    /// Имя крейта-компонента.
    pub name: String,
}

/// Определяет компоненты, необходимые по результатам сканирования шаблонов.
///
/// Для каждого использованного тега, атрибута и директивы ищет компонент в
/// реестре; результат отсортирован и без дубликатов.
pub fn needed_components(
    scan: &ScanResult,
    registry: &ComponentRegistry,
) -> Vec<RequiredComponent> {
    let mut names: BTreeSet<String> = BTreeSet::new();
    for tag in scan.tags.keys() {
        if let Some(component) = registry.component_for_element(tag) {
            names.insert(component.name.clone());
        }
    }
    for attr in scan.attributes.keys() {
        if let Some(component) = registry.component_for_attribute(attr) {
            names.insert(component.name.clone());
            continue;
        }
        if is_directive(attr)
            && let Some(component) = registry.component_for_directive(attr)
        {
            names.insert(component.name.clone());
        }
    }
    names
        .into_iter()
        .map(|name| RequiredComponent { name })
        .collect()
}

/// Атрибут похож на директиву (`@click`, `v-model`, `:disabled`).
fn is_directive(attr: &str) -> bool {
    attr.starts_with('@') || attr.starts_with(':') || attr.starts_with("v-")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::ScanResult;

    fn scan_with(tags: &[&str], attrs: &[&str]) -> ScanResult {
        let mut scan = ScanResult::default();
        for t in tags {
            scan.tags.insert((*t).to_string(), 1);
        }
        for a in attrs {
            scan.attributes.insert((*a).to_string(), 1);
        }
        scan
    }

    #[test]
    fn resolves_needed_components_from_tags() {
        let registry = ComponentRegistry::builtin();
        let scan = scan_with(&["text", "checkbox", "div"], &[]);
        let needed = needed_components(&scan, &registry);
        let names: Vec<_> = needed.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["relay-element-form", "relay-element-text"],
            "div не сопоставляется ни с одним компонентом"
        );
    }

    #[test]
    fn resolves_directives_and_attributes() {
        let registry = ComponentRegistry::builtin();
        let scan = scan_with(&[], &["required", "@validate"]);
        let needed = needed_components(&scan, &registry);
        assert_eq!(needed.len(), 1);
        assert_eq!(needed[0].name, "relay-behavior-validation");
    }

    #[test]
    fn no_duplicates_for_shared_component() {
        let registry = ComponentRegistry::builtin();
        let scan = scan_with(&["checkbox", "radio"], &["required"]);
        let needed = needed_components(&scan, &registry);
        assert_eq!(needed.len(), 2, "form + validation, без дублей");
    }
}
