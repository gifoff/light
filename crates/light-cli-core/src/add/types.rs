use crate::fetch::{DependencyEntry, ResolvedComponent};

use super::helpers::dependency_line;

/// Опции команды `light add`.
#[derive(Debug, Clone)]
pub struct AddOptions {
    /// Явная версия (`--version`).
    pub version: Option<String>,
    /// Не задавать вопросов (`--yes`).
    pub yes: bool,
    /// Только показать план (`--dry-run`).
    pub dry_run: bool,
    /// Без сетевого доступа (`--offline`).
    pub offline: bool,
    /// Фича, к которой привязать зависимость (`--feature`).
    pub feature: Option<String>,
    /// Фичи самого добавляемого крейта (`--features`).
    pub features: Vec<String>,
    /// Не запускать `cargo check` (`--no-verify`).
    pub no_verify: bool,
    /// Принудительно разрешить конфликт по тегу (`--force`): новый компонент
    /// «вытесняет» уже установленного владельца общего элемента, приоритет
    /// фиксируется в секции `[plugins]` файла `light.lock`.
    pub force: bool,
    /// Уважать дефолтные фичи мета-крейта `light-fw` при добавлении компонентов.
    ///
    /// Всегда `true`: автоподгрузка (`light build`/`light dev`) и ручной `light add`
    /// никогда не отключают `default-features` у `light-fw`. Поле существует, чтобы
    /// опция передавалась явно (и её нельзя было случайно выключить через
    /// `Default::default()`), — см. ТЗ 0.17.2. Значение `false` допустимо, но
    /// ничего не меняет: `edit.rs` никогда не выносит `default-features = false`.
    pub default_features: bool,
}

// Ручная реализация `Default`: все флаги — `false`, кроме `default_features`,
// который обязан быть `true`, чтобы ни один вызов через `Default::default()`
// не мог случайно отключить дефолтные фичи `light-fw` (ТЗ 0.17.2).
impl Default for AddOptions {
    fn default() -> Self {
        Self {
            version: None,
            yes: false,
            dry_run: false,
            offline: false,
            feature: None,
            features: Vec::new(),
            no_verify: false,
            force: false,
            default_features: true,
        }
    }
}

/// Компонент, который нужно установить дополнительно (из `requires`).
#[derive(Debug, Clone)]
pub struct RequiredComponent {
    /// Имя крейта.
    pub name: String,
    /// Как записать зависимость.
    pub dependency: DependencyEntry,
}

/// Подготовленный план добавления компонента.
#[derive(Debug, Clone)]
pub struct AddPlan {
    /// Разрешённый компонент.
    pub component: ResolvedComponent,
    /// Как записать зависимость.
    pub dependency: DependencyEntry,
    /// Фича, к которой привязывается зависимость.
    pub feature: String,
    /// Фичи самого крейта (`--features`).
    pub crate_features: Vec<String>,
    /// Фичи мета-крейта `light-fw`, которые нужно включить.
    pub light_features: Vec<String>,
    /// Будет ли фича создана (иначе — уже существует).
    pub creates_feature: bool,
    /// Компонент уже установлен (повторный `add` — не ошибка).
    pub already_installed: bool,
    /// Дополнительные компоненты из `requires`.
    pub requires: Vec<RequiredComponent>,
    /// Приоритеты по тегам (`--force`): элемент → добавляемый крейт-победитель.
    ///
    /// Пусто, если конфликта не было или он разрешён повторным `add` без
    /// `--force`. Применяется в [`do_add`]: при записи `light.lock` для каждого
    /// тега вызывается `Lockfile::set_plugin_priority`.
    pub plugin_priorities: Vec<(String, String)>,
}

impl AddPlan {
    /// Человекочитаемое описание плана.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("План добавления {}\n", self.component.name));
        let location = match &self.component.path {
            Some(p) => format!(" ({})", p.display()),
            None => String::new(),
        };
        out.push_str(&format!(
            "  источник:    {}{}\n",
            self.component.source.as_str(),
            location,
        ));
        out.push_str(&format!("  версия:      {}\n", self.component.version));
        if self.already_installed {
            out.push_str("  статус:      уже установлен\n");
            return out;
        }
        out.push_str(&format!(
            "  фича:        {}{}\n",
            self.feature,
            if self.creates_feature {
                " (будет создана)"
            } else {
                ""
            }
        ));
        out.push_str(&format!(
            "  зависимость: {}\n",
            dependency_line(&self.component.name, &self.dependency)
        ));
        if !self.crate_features.is_empty() {
            out.push_str(&format!(
                "  фичи крейта: {}\n",
                self.crate_features.join(", ")
            ));
        }
        if !self.light_features.is_empty() {
            out.push_str(&format!(
                "  фичи light:  {}\n",
                self.light_features.join(", ")
            ));
        }
        for req in &self.requires {
            out.push_str(&format!(
                "  также нужно: {} — {}\n",
                req.name,
                dependency_line(&req.name, &req.dependency)
            ));
        }
        out
    }
}

/// Результат применения плана.
#[derive(Debug, Clone)]
pub struct AddOutcome {
    /// Установленный компонент.
    pub component: ResolvedComponent,
    /// Фича, к которой привязана зависимость.
    pub feature: String,
    /// Дополнительно установленные компоненты.
    pub added_components: Vec<String>,
    /// Запускалась ли проверка сборки.
    pub verified: bool,
}
