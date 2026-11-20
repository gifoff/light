use super::node::{NodeKind, VElement};
use super::style::{SelectOption, Style};
pub use crate::component::{DEFAULT_BACKGROUND, WindowConfig, background_rgba};
#[cfg(feature = "window")]
use crate::window_backend::WindowCommand;

/// Общий интерфейс любого элемента дерева: чтение и изменение свойств плюс
/// отправка действий оконному бэкенду.
///
/// Это «базовый класс» для узлов интерфейса: конкретный элемент — [`VElement`],
/// который реализует этот трейт. Операции над окном описаны в [`VWindow`].
pub trait VNode {
    /// Стабильный ключ узла.
    fn key(&self) -> &'static str;
    /// Тип элемента.
    fn kind(&self) -> &NodeKind;
    /// Изменяемый тип элемента.
    fn kind_mut(&mut self) -> &mut NodeKind;
    /// Стиль узла.
    fn style(&self) -> &Style;
    /// Изменяемый стиль узла.
    fn style_mut(&mut self) -> &mut Style;
    /// Имя HTML-тега источника.
    fn tag(&self) -> &'static str;
    /// Атрибуты элемента.
    fn attrs(&self) -> &[(&'static str, String)];
    /// Дочерние узлы.
    fn children(&self) -> &[VElement];

    /// Значение атрибута по имени.
    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs()
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| value.as_str())
    }

    /// Текст узла (для [`NodeKind::Text`]).
    fn text(&self) -> Option<&str> {
        match self.kind() {
            NodeKind::Text { content } => Some(content),
            _ => None,
        }
    }

    /// Задаёт текст узла (для [`NodeKind::Text`]).
    fn set_text(&mut self, value: impl Into<String>) {
        if let NodeKind::Text { content } = self.kind_mut() {
            *content = value.into();
        }
    }

    /// Виден ли узел.
    fn visible(&self) -> bool {
        self.style().visible
    }

    /// Показывает или скрывает узел.
    fn set_visible(&mut self, value: bool) {
        self.style_mut().visible = value;
    }

    /// Заблокирован ли узел.
    fn disabled(&self) -> bool {
        match self.kind() {
            NodeKind::Button { disabled, .. } | NodeKind::TextField { disabled, .. } => *disabled,
            #[cfg(feature = "elements-checkbox")]
            NodeKind::Checkbox { disabled, .. } => *disabled,
            #[cfg(feature = "elements-radio")]
            NodeKind::Radio { disabled, .. } => *disabled,
            #[cfg(feature = "elements-select")]
            NodeKind::Select { disabled, .. } => *disabled,
            _ => false,
        }
    }

    /// Блокирует или разблокирует узел.
    fn set_disabled(&mut self, value: bool) {
        match self.kind_mut() {
            NodeKind::Button { disabled, .. } | NodeKind::TextField { disabled, .. } => {
                *disabled = value;
            }
            #[cfg(feature = "elements-checkbox")]
            NodeKind::Checkbox { disabled, .. } => *disabled = value,
            #[cfg(feature = "elements-radio")]
            NodeKind::Radio { disabled, .. } => *disabled = value,
            #[cfg(feature = "elements-select")]
            NodeKind::Select { disabled, .. } => *disabled = value,
            _ => {}
        }
    }

    /// Текущее значение (поля ввода или списка).
    fn value(&self) -> Option<&str> {
        match self.kind() {
            NodeKind::TextField { value, .. } => Some(value),
            #[cfg(feature = "elements-select")]
            NodeKind::Select { value, .. } => Some(value),
            _ => None,
        }
    }

    /// Задаёт текущее значение.
    fn set_value(&mut self, new_value: impl Into<String>) {
        let new_value = new_value.into();
        match self.kind_mut() {
            NodeKind::TextField { value, .. } => {
                *value = new_value;
            }
            #[cfg(feature = "elements-select")]
            NodeKind::Select { value, .. } => {
                *value = new_value;
            }
            _ => {}
        }
    }

    /// Отмечен ли флажок.
    fn checked(&self) -> bool {
        #[cfg(feature = "elements-checkbox")]
        {
            matches!(self.kind(), NodeKind::Checkbox { checked: true, .. })
        }
        #[cfg(not(feature = "elements-checkbox"))]
        {
            false
        }
    }

    /// Ставит или снимает флажок.
    fn set_checked(&mut self, value: bool) {
        #[cfg(feature = "elements-checkbox")]
        if let NodeKind::Checkbox { checked, .. } = self.kind_mut() {
            *checked = value;
        }
        #[cfg(not(feature = "elements-checkbox"))]
        let _ = value;
    }

    /// Выбран ли переключатель.
    fn selected(&self) -> bool {
        #[cfg(feature = "elements-radio")]
        {
            matches!(self.kind(), NodeKind::Radio { selected: true, .. })
        }
        #[cfg(not(feature = "elements-radio"))]
        {
            false
        }
    }

    /// Выбирает или снимает переключатель.
    fn set_selected(&mut self, value: bool) {
        #[cfg(feature = "elements-radio")]
        if let NodeKind::Radio { selected, .. } = self.kind_mut() {
            *selected = value;
        }
        #[cfg(not(feature = "elements-radio"))]
        let _ = value;
    }

    /// Варианты выпадающего списка.
    fn options(&self) -> &[SelectOption] {
        match self.kind() {
            #[cfg(feature = "elements-select")]
            NodeKind::Select { options, .. } => options,
            _ => &[],
        }
    }

    /// Задаёт варианты выпадающего списка.
    fn set_options(&mut self, value: Vec<SelectOption>) {
        #[cfg(feature = "elements-select")]
        if let NodeKind::Select { options, .. } = self.kind_mut() {
            *options = value;
        }
        #[cfg(not(feature = "elements-select"))]
        let _ = value;
    }

    /// Отправляет действие оконному бэкенду для окна, событие которого
    /// обрабатывается в данный момент.
    #[cfg(feature = "window")]
    fn act(&self, command: WindowCommand) {
        crate::window_backend::request(command);
    }
}

/// Операции над окном — расширение интерфейса узла [`VNode`].
///
/// Реализуется корневым узлом [`NodeKind::Window`]; методы отправляют команду
/// активному окну (см. [`crate::window_backend`]). Без фичи `window` трейт
/// не содержит методов управления окном.
#[cfg(feature = "window")]
pub trait VWindow: VNode {
    /// Настройки системного окна, если узел — окно.
    fn config(&self) -> Option<&WindowConfig>;

    /// Закрывает окно.
    fn close(&self) {
        self.act(WindowCommand::Close);
    }

    /// Сворачивает окно.
    fn minimize(&self) {
        self.act(WindowCommand::Minimize);
    }

    /// Разворачивает окно.
    fn maximize(&self) {
        self.act(WindowCommand::Maximize);
    }

    /// Возвращает окно в обычный размер.
    fn restore(&self) {
        self.act(WindowCommand::Restore);
    }

    /// Переключает развёрнутость окна.
    fn toggle_maximize(&self) {
        self.act(WindowCommand::ToggleMaximize);
    }

    /// Показывает скрытое окно.
    fn show(&self) {
        self.act(WindowCommand::Show);
    }

    /// Скрывает окно.
    fn hide(&self) {
        self.act(WindowCommand::Hide);
    }

    /// Открывает дочернее модальное окно поверх текущего.
    fn open_modal(&self) {
        self.act(WindowCommand::OpenModal);
    }

    /// Открывает ещё одно главное окно приложения.
    fn open_window(&self) {
        self.act(WindowCommand::OpenWindow);
    }

    /// Открывает (или фокусирует) окно с идентификатором `id`.
    fn open(&self, id: &str) {
        self.act(WindowCommand::Open(id.to_owned()));
    }

    /// Закрывает окно с идентификатором `id` вместе с потомками.
    fn close_window(&self, id: &str) {
        self.act(WindowCommand::CloseWindow(id.to_owned()));
    }

    /// Показывает окно с идентификатором `id`.
    fn show_window(&self, id: &str) {
        self.act(WindowCommand::ShowWindow(id.to_owned()));
    }

    /// Скрывает окно с идентификатором `id`.
    fn hide_window(&self, id: &str) {
        self.act(WindowCommand::HideWindow(id.to_owned()));
    }

    /// Развёрнуто ли окно в данный момент.
    fn is_maximized(&self) -> bool {
        crate::window_backend::is_maximized()
    }
}

/// Пустая заглушка трейта [`VWindow`] без фичи `window`.
///
/// Макрос `#[template]` генерирует `use VWindow as _` безусловно,
/// поэтому тип должен существовать всегда. Без фичи `window`
/// методов управления окном нет — трейт пустой.
#[cfg(not(feature = "window"))]
pub trait VWindow: VNode {}

impl VNode for VElement {
    fn key(&self) -> &'static str {
        self.key
    }

    fn kind(&self) -> &NodeKind {
        &self.kind
    }

    fn kind_mut(&mut self) -> &mut NodeKind {
        &mut self.kind
    }

    fn style(&self) -> &Style {
        &self.style
    }

    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }

    fn tag(&self) -> &'static str {
        self.tag
    }

    fn attrs(&self) -> &[(&'static str, String)] {
        &self.attrs
    }

    fn children(&self) -> &[VElement] {
        &self.children
    }
}

#[cfg(feature = "window")]
impl VWindow for VElement {
    fn config(&self) -> Option<&WindowConfig> {
        self.window.as_ref()
    }
}
