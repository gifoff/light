use crate::edit;
use crate::error::Result;
use crate::fetch::DependencyEntry;

pub(crate) fn dependency_line(name: &str, dependency: &DependencyEntry) -> String {
    match dependency {
        DependencyEntry::Workspace => format!("{name} = {{ workspace = true, optional = true }}"),
        DependencyEntry::Path(path) => {
            format!(
                "{name} = {{ path = \"{}\", optional = true }}",
                path.display()
            )
        }
        DependencyEntry::Version(version) => {
            format!("{name} = {{ version = \"{version}\", optional = true }}")
        }
    }
}

/// Имя фичи по умолчанию: `light-fw-element-form` → `element-form`.
pub fn default_feature_name(name: &str) -> String {
    name.strip_prefix("light-fw-").unwrap_or(name).to_string()
}

/// Выбирает фичу, к которой привязать компонент.
///
/// Порядок: фича `<component>` из манифеста → существующая агрегирующая фича
/// (`dep:light-*`, предпочитая `full`) → новая фича [`default_feature_name`].
pub(crate) fn choose_feature(text: &str, name: &str) -> Result<String> {
    let default = default_feature_name(name);
    if edit::has_feature(text, &default)? {
        return Ok(default);
    }
    let aggregators = edit::features_with_light_component_dep(text)?;
    if let Some(full) = aggregators.iter().find(|f| f.as_str() == "full") {
        return Ok(full.clone());
    }
    if let Some(first) = aggregators.first() {
        return Ok(first.clone());
    }
    Ok(default)
}

/// Разбирает `"light-fw-element-form ^0.3"` → `("light-fw-element-form", Some("^0.3"))`.
pub(crate) fn split_requirement(requirement: &str) -> (&str, Option<&str>) {
    match requirement.split_once(char::is_whitespace) {
        Some((name, rest)) => (name, Some(rest.trim())),
        None => (requirement, None),
    }
}

pub(crate) fn is_core_name(name: &str) -> bool {
    matches!(name, "light" | "light-fw" | "light-fw-core")
}
