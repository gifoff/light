/// Текущая версия CLI-крэйта `light` (ядро Light). Используется для проверки
/// совместимости `core`-требования компонента.
pub const LIGHT_CORE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Версия, извлечённая из скачанного `.crate`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentMetadata {
    /// Имя крейта-компонента.
    pub crate_name: String,
    /// Версия скачанного артефакта.
    pub version: String,
    /// Тип компонента (строка из манифеста: `widget`, `element`, …).
    pub kind: Option<String>,
    /// Требование к версии ядра (semver-range из `core`).
    pub core_version_req: Option<String>,
    /// Элементы, которые компонент предоставляет.
    pub provides_elements: Vec<String>,
    /// Атрибуты, которые компонент предоставляет.
    pub provides_attributes: Vec<String>,
    /// Директивы, которые компонент предоставляет.
    pub provides_directives: Vec<String>,
    /// Обязательные компоненты (`"name ^ver"`).
    pub requires_components: Vec<String>,
}

/// Результат валидации скачанного `.crate`.
#[derive(Debug, PartialEq, Eq)]
pub enum ValidationResult {
    /// Компонент валиден и совместим.
    Ok(ComponentMetadata),
    /// Фатальная ошибка (нет манифеста / несовместимое ядро) — установка
    /// невозможна без действий пользователя.
    Fatal(ValidationError),
    /// Предупреждение (пустые `provides`, конфликт): пользователь может
    /// подтвердить установку через [y/N].
    Warning {
        /// Метаданные валидны, предупреждение отдельно.
        metadata: ComponentMetadata,
        /// Само предупреждение (`EmptyProvides` или `Conflict`).
        warning: ValidationError,
    },
}

/// Ошибка валидации скачанного компонента.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    /// В `.crate` нет секции `[package.metadata.light]`.
    NoMetadata,
    /// `core`-требование несовместимо с текущей версией ядра.
    IncompatibleCore {
        /// Требование из манифеста (например `^0.20`).
        required: String,
        /// Текущая версия ядра (например `0.19.0-dev`).
        current: String,
    },
    /// `provides` полностью пуст (элементов/атрибутов/директив нет).
    EmptyProvides,
    /// Конфликт с уже установленным компонентом по тегу.
    Conflict {
        /// Тег, по которому возник конфликт.
        tag: String,
        /// Имя уже установленного владельца тега.
        current_owner: String,
    },
}

impl ValidationError {
    /// Человекочитаемое сообщение на русском (для интерактивного вывода).
    pub fn message(&self) -> String {
        match self {
            ValidationError::NoMetadata => {
                "в крейте нет секции `[package.metadata.light]` — это не компонент Light"
                    .to_string()
            }
            ValidationError::IncompatibleCore { required, current } => {
                format!("компонент требует ядро `{required}`, но установлено `{current}`")
            }
            ValidationError::EmptyProvides => "компонент не объявляет ни одного элемента, \
                 атрибута или директивы (`provides` пуст)"
                .to_string(),
            ValidationError::Conflict { tag, current_owner } => {
                format!("конфликт: тег `{tag}` уже предоставлен компонентом `{current_owner}`")
            }
        }
    }

    /// Является ли эта ошибка фатальной (без возможности продолжить).
    pub fn is_fatal(&self) -> bool {
        matches!(
            self,
            ValidationError::NoMetadata | ValidationError::IncompatibleCore { .. }
        )
    }
}
