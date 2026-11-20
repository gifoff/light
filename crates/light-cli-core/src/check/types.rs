use crate::manifest::Provides;

/// Компонент-элемент, найденный в проекте (из `cargo metadata`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentInfo {
    /// Имя крейта (например, `light-fw-element-form`).
    pub crate_name: String,
    /// Версия, извлечённая из `Cargo.toml` пакета.
    pub version: String,
    /// Что компонент предоставляет (`provides.elements/attributes/directives`).
    pub provides: Provides,
}

/// Компонент, который можно установить (есть во встроенном реестре).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvailableComponent {
    /// Тег, которого не хватает в проекте.
    pub tag: String,
    /// Имя крейта, который его предоставляет.
    pub crate_name: String,
    /// Рекомендация к версии (`name ^ver`).
    pub requirement: String,
}

/// Конфликт: один тег предоставляют несколько установленных компонентов.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    /// Конфликтующий тег.
    pub tag: String,
    /// Имена компонентов-обладателей (отсортированы).
    pub owners: Vec<String>,
}

/// Результат умной проверки проекта.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CheckReport {
    /// Установленные компоненты (есть в `Cargo.toml` как path/git/published).
    pub installed: Vec<ComponentInfo>,
    /// Компоненты, которые можно установить из встроенного реестра.
    pub available: Vec<AvailableComponent>,
    /// Нераспознанные теги (не стандартные и не из реестра).
    pub unknown: Vec<String>,
    /// Установленные, но не используемые компоненты.
    pub unused: Vec<ComponentInfo>,
    /// Конфликты: один тег предоставляют два и более установленных компонента.
    pub conflicts: Vec<Conflict>,
}

impl CheckReport {
    /// Есть ли проблемы (для `--strict`): доступно, неизвестно или конфликт.
    pub fn has_problems(&self) -> bool {
        !self.available.is_empty() || !self.unknown.is_empty() || !self.conflicts.is_empty()
    }

    /// Человекочитаемый отчёт по спецификации ТЗ (русская терминология,
    /// эмодзи-маркеры категорий).
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        if self.installed.is_empty() {
            out.push_str("🟢 Установлено: нет\n");
        } else {
            out.push_str("🟢 Установлено:\n");
            for c in &self.installed {
                out.push_str(&format!("  - {} {}\n", c.crate_name, c.version));
            }
        }
        if !self.available.is_empty() {
            out.push_str("\n🟡 Доступно (установить):\n");
            for a in &self.available {
                out.push_str(&format!("  - <{}> → {}\n", a.tag, a.requirement));
            }
        }
        if !self.unknown.is_empty() {
            out.push_str("\n🔴 Неизвестно (нужен плагин или кастомный элемент):\n");
            for u in &self.unknown {
                out.push_str(&format!("  - <{}>\n", u));
            }
        }
        if !self.unused.is_empty() {
            out.push_str("\n🗑 Не используется (можно убрать из Cargo.toml):\n");
            for u in &self.unused {
                out.push_str(&format!("  - {}\n", u.crate_name));
            }
        }
        if !self.conflicts.is_empty() {
            out.push_str("\n⚠️ Конфликты:\n");
            for c in &self.conflicts {
                out.push_str(&format!("  - <{}>: {}\n", c.tag, c.owners.join(", ")));
            }
        }
        out
    }

    /// Машиночитаемый отчёт (JSON) для CI.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "installed": self
                .installed
                .iter()
                .map(|c| serde_json::json!({
                    "crate_name": c.crate_name,
                    "version": c.version,
                    "provides_elements": c.provides.elements,
                    "provides_attributes": c.provides.attributes,
                    "provides_directives": c.provides.directives,
                }))
                .collect::<Vec<_>>(),
            "available": self
                .available
                .iter()
                .map(|a| serde_json::json!({
                    "tag": a.tag,
                    "crate_name": a.crate_name,
                    "requirement": a.requirement,
                }))
                .collect::<Vec<_>>(),
            "unknown": &self.unknown,
            "unused": self
                .unused
                .iter()
                .map(|u| serde_json::json!({ "crate_name": u.crate_name }))
                .collect::<Vec<_>>(),
            "conflicts": self
                .conflicts
                .iter()
                .map(|c| serde_json::json!({
                    "tag": c.tag,
                    "owners": c.owners,
                }))
                .collect::<Vec<_>>(),
        })
    }
}
