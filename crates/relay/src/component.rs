//! Компонент: виртуальное дерево интерфейса, обработка событий и описание
//! системного окна.

use crate::vdom::{Event, VElement};

/// Фон окна по умолчанию (`#F7F8FA`, полностью непрозрачный).
pub const DEFAULT_BACKGROUND: u32 = 0xF7_F8_FA_FF;

/// Разбирает цвет `0xRRGGBBAA` на компоненты `(r, g, b, a)`.
pub const fn background_rgba(color: u32) -> (u8, u8, u8, u8) {
    (
        (color >> 24) as u8,
        (color >> 16) as u8,
        (color >> 8) as u8,
        color as u8,
    )
}

/// Настройки системного окна — из корневого элемента `<window>` шаблона.
///
/// Окно создаётся **без системной рамки** (`borderless`): заголовок и кнопки
/// управления задаются элементами интерфейса (`<titlebar>`, `<button>`).
/// Если шаблон не содержит `<window>`, [`Component::window_config`] возвращает
/// `None` и окно не создаётся.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowConfig {
    /// Заголовок окна для панели задач. Обычно переопределяется текстом
    /// элемента `<titlebar>`.
    pub title: String,
    /// Ширина окна в логических пикселях.
    pub width: u32,
    /// Высота окна в логических пикселях.
    pub height: u32,
    /// Цвет фона клиентской области в формате `0xRRGGBBAA` (альфа включена).
    pub background: u32,
    /// Можно ли менять размер окна.
    pub resizable: bool,
    /// Поддерживает ли окно прозрачность (композиция с рабочим столом).
    pub transparent: bool,
    /// Минимальная ширина окна (`min-width`), в логических пикселях.
    pub min_width: Option<u32>,
    /// Минимальная высота окна (`min-height`), в логических пикселях.
    pub min_height: Option<u32>,
    /// Максимальная ширина окна (`max-width`), в логических пикселях.
    pub max_width: Option<u32>,
    /// Максимальная высота окна (`max-height`), в логических пикселях.
    pub max_height: Option<u32>,
    /// Открывается ли окно как модальное (блокирует родительское окно).
    pub modal: bool,
    /// Видимо ли окно при создании (`visible`), по умолчанию `true`.
    pub visible: bool,
    /// Уникальный идентификатор окна из атрибута `<window id="...">`.
    ///
    /// Служит ключом для команд [`crate::control::WindowCommand::Open`],
    /// [`crate::control::WindowCommand::CloseWindow`],
    /// [`crate::control::WindowCommand::ShowWindow`] и
    /// [`crate::control::WindowCommand::HideWindow`].
    pub id: Option<String>,
    /// Идентификатор родительского окна из атрибута `<window parent="...">`.
    ///
    /// Дочернее окно закрывается вместе с родителем, а модальный потомок
    /// блокирует ввод родителю.
    pub parent: Option<String>,
}

impl WindowConfig {
    /// Компоненты фона `(r, g, b, a)`.
    pub const fn background_rgba(&self) -> (u8, u8, u8, u8) {
        background_rgba(self.background)
    }
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            title: String::from("Relay"),
            width: 640,
            height: 480,
            background: DEFAULT_BACKGROUND,
            resizable: true,
            transparent: false,
            min_width: None,
            min_height: None,
            max_width: None,
            max_height: None,
            modal: false,
            visible: true,
            id: None,
            parent: None,
        }
    }
}

/// UI-компонент: структура, описывающая свойства и интерфейс.
///
/// Реализация обычно генерируется атрибутом
/// `#[template("...")]` из HTML-шаблона — шаблон полностью замещает
/// форму компонента: [`Component::render`] возвращает именно дерево из
/// шаблона, без какого-либо стандартного оформления сверх него.
pub trait Component {
    /// Строит виртуальное дерево интерфейса компонента.
    fn render(&self) -> VElement;

    /// Обрабатывает событие `event`, адресованное узлу `key`.
    ///
    /// Возвращает `true`, если событие было обработано (тогда оконный бэкенд
    /// прекращает всплытие к родительским узлам). Реализация генерируется
    /// макросом `#[template]` из директив `@click`, `@mousedown`, `@keydown`,
    /// `v-model` и т. п.
    fn dispatch(&self, key: &str, event: Event) -> bool {
        let _ = (key, event);
        false
    }

    /// Проверяет отдельное поле (директива `:validator`).
    ///
    /// Реализация генерируется макросом `#[template]`; метод вызывается для
    /// полей, у которых задан `:validator="имя"`, с текущим значением поля.
    /// По умолчанию поле считается валидным.
    fn validate_field(&self, field: &str, value: &str) -> Result<(), String> {
        let _ = (field, value);
        Ok(())
    }

    /// Проверяет всю форму (директива `@validate`).
    ///
    /// Возвращает `None`, если своя проверка формы не задана (тогда выполняется
    /// только стандартная проверка `required`), либо `Some(errors)` со списком
    /// пар «ключ поля — сообщение». Реализация генерируется макросом
    /// `#[template]`.
    fn validate_form(&self, form: &str) -> Option<Vec<(String, String)>> {
        let _ = form;
        None
    }

    /// Развёрнуто ли окно в данный момент.
    ///
    /// Доступно в шаблонах как свойство: `v-if="is_maximized"`,
    /// `v-if="!is_maximized"`.
    fn is_maximized(&self) -> bool {
        crate::control::is_maximized()
    }

    /// Описание системного окна компонента.
    ///
    /// По умолчанию берётся из корневого элемента дерева, возвращённого
    /// [`Component::render`] (узел `<window>`). Если корень не описывает окно,
    /// возвращается `None` и системное окно не создаётся.
    fn window_config(&self) -> Option<WindowConfig> {
        self.render().window_config().cloned()
    }

    /// Компонент дочернего модального окна, открываемого по команде
    /// [`crate::control::WindowCommand::OpenModal`].
    ///
    /// Макрос `#[template("…", modal = Компонент)]` генерирует эту реализацию
    /// автоматически. По умолчанию модальное окно не предусмотрено.
    fn modal(&self) -> Option<Box<dyn Component>> {
        None
    }

    /// Компонент ещё одного **главного** окна, открываемого по команде
    /// [`crate::control::WindowCommand::OpenWindow`].
    ///
    /// Макрос `#[template("…", window = Компонент)]` генерирует эту реализацию
    /// автоматически. По умолчанию дополнительное главное окно не предусмотрено.
    fn window_factory(&self) -> Option<Box<dyn Component>> {
        None
    }

    /// Фабрики окон, открываемых по идентификатору.
    ///
    /// Макрос `#[template("...", windows = [A, B])]` генерирует эту реализацию.
    /// Каждая фабрика создаёт компонент окна, чей `id` (из `<window id="...">`)
    /// служит ключом для [`crate::control::WindowCommand::Open`]. По умолчанию
    /// список пуст.
    fn windows(&self) -> Vec<fn() -> Box<dyn Component>> {
        Vec::new()
    }
}
