//! Компонент: виртуальное дерево интерфейса, обработка событий и описание
//! системного окна.

use std::any::Any;

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
    /// PNG-содержимое иконки окна из атрибута `<window icon="...">`.
    ///
    /// Файл встраивается в бинарник на этапе компиляции. Если иконка не
    /// задана, используется встроенная иконка Light.
    pub icon: Option<&'static [u8]>,
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
            title: String::from("Light"),
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
            icon: None,
        }
    }
}

/// UI-компонент: структура, описывающая свойства и интерфейс.
///
/// Реализация обычно генерируется атрибутом
/// `#[template("...")]` из HTML-шаблона — шаблон полностью замещает
/// форму компонента: [`Component::render`] возвращает именно дерево из
/// шаблона, без какого-либо стандартного оформления сверх него.
pub trait Component: Any {
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

    /// Читает значение поля по имени для шаблонов во время выполнения.
    ///
    /// Используется макросом `#[template]` с фичей `runtime-templates`: шаблон
    /// `{{ поле }}` обращается к этому методу вместо встроенного на этапе
    /// сборки значения. По умолчанию — `None` (поле неизвестно), и генерируемый
    /// макросом код переопределяет его для каждого поля структуры через
    /// геттер (`self.count()`).
    fn eval_field(&self, _name: &str) -> Option<String> {
        None
    }

    /// Диспетчирует событие по имени обработчика для шаблонов в runtime.
    ///
    /// Имя — уже без скобок вызова (runtime-парсер срезает их). Сопоставляются
    /// встроенные методы окна (`"close"`, `"minimize"`, `"toggle_maximize"`,
    /// `"open_modal"`); остальные имена пробуются через
    /// [`Component::invoke_handler`], что позволяет сгенерированному макросом
    /// коду донести имена пользовательских методов до этой точки.
    fn dispatch_by_name(&self, handler: &str, event: Event) -> bool {
        // Не-Click события (Mousemove, Mouseenter и т. д.) не должны
        // запускать @click-обработчики по имени.
        if !matches!(event, Event::Click) {
            return false;
        }
        match handler {
            "close" => {
                self.close();
                true
            }
            "minimize" => {
                self.minimize();
                true
            }
            "toggle_maximize" => {
                self.toggle_maximize();
                true
            }
            "open_modal" => {
                self.open_modal();
                true
            }
            _ => self.invoke_handler(handler, event),
        }
    }

    /// Вызывает пользовательский обработчик `@click` по имени.
    ///
    /// По умолчанию — `false` (имя неизвестно). Генерируемый макросом код
    /// переопределяет его `match`-цепочкой на имена методов компонента,
    /// найденных в статическом шаблоне. Имя сопоставляется как точный
    /// литерал (`"increment"`) и как ключ узла (`key="inc_btn"`): runtime-
    /// дерево использует имя обработчика в качестве ключа, а DevTools может
    /// отправлять `CLICK <ключ>` для произвольного ключа. Вызывается только
    /// после промаха встроенных методов.
    fn invoke_handler(&self, _handler: &str, _event: Event) -> bool {
        false
    }

    /// Снимок реактивных свойств компонента для инструментов разработки.
    ///
    /// По умолчанию — `null`: компонент без реактивных свойств ничего не
    /// сообщает. Реализацию генерирует макрос `#[template]`: каждое поле
    /// появляется под своим именем со значением геттера, неподдерживаемые
    /// типы — как `null`/`"<unsupported>"`. Метод доступен всегда: с фичей
    /// `devtools` его используют инспектор и сохранение состояния, без неё —
    /// только при явном вызове.
    fn inspect_state(&self) -> serde_json::Value {
        serde_json::Value::Null
    }

    /// Вставляет состояние, снятое через [`inspect_state`](Self::inspect_state),
    /// обратно в реактивные поля компонента.
    ///
    /// По умолчанию — операция без действия: компонент без реактивных свойств
    /// не принимает значения. Реализация генерируется макросом `#[template]`
    /// (фича `devtools`): каждое поле перезаписывается значением из объекта
    /// `state`, если оно там присутствует и успешно десериализуется в тип
    /// поля. Поддерживаются те же типы, что и в `inspect_state`. Метод
    /// доступен всегда: с фичей `devtools` его применяют при запуске
    /// (восстановление `.light/state.json`) и по команде `SET_STATE` из
    /// DevTools; без фичи — только при явном вызове. Он не уведомляет
    /// обозревателей — перерисовку организует вызывающая сторона (см.
    /// `window/mod.rs`).
    fn inject_state(&mut self, state: &serde_json::Value) {
        let _ = state;
    }

    /// Доступ к конкретному типу компонента через `Any`.
    ///
    /// Используется `post_update` для downcast-а к целевому типу компонента
    /// в UI-потоке: `UserEvent::Action` хранит `TypeId` цели, а обработчик
    /// находит первое совпадение по `type_id()` среди всех окон. Прямое
    /// приведение `&mut dyn Component` → `&mut dyn Any` невозможно:
    /// `dyn Component` не `Sized`, а `Any: Sized`. Реализация генерируется
    /// макросом `#[template]`.
    fn as_any_mut(&mut self) -> &mut dyn Any;

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

    /// Закрывает активное окно.
    ///
    /// Доступно в шаблонах как обработчик: `@click="close()"`.
    fn close(&self) {
        crate::control::request(crate::control::WindowCommand::Close);
    }

    /// Сворачивает активное окно.
    ///
    /// Доступно в шаблонах как обработчик: `@click="minimize()"`.
    fn minimize(&self) {
        crate::control::request(crate::control::WindowCommand::Minimize);
    }

    /// Переключает развёрнутое состояние активного окна.
    ///
    /// Доступно в шаблонах как обработчик: `@click="toggle_maximize()"`.
    fn toggle_maximize(&self) {
        crate::control::request(crate::control::WindowCommand::ToggleMaximize);
    }

    /// Открывает модальное окно, описанное [`Component::modal`].
    ///
    /// Доступно в шаблонах как обработчик: `@click="open_modal()"`.
    fn open_modal(&self) {
        crate::control::request(crate::control::WindowCommand::OpenModal);
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

#[cfg(test)]
mod tests {
    use std::any::Any;
    use std::cell::Cell;

    use super::Component;
    use crate::vdom::{Event, VElement};

    /// Тестовый компонент: не переопределяет `dispatch_by_name` (тестируем
    /// базовую реализацию с Click-фильтром), перехватывает только
    /// `invoke_handler` для пользовательских имён.
    struct FakeComp {
        handler_called: Cell<bool>,
    }

    impl FakeComp {
        fn new() -> Self {
            Self {
                handler_called: Cell::new(false),
            }
        }
    }

    impl Default for FakeComp {
        fn default() -> Self {
            Self::new()
        }
    }

    impl Component for FakeComp {
        fn render(&self) -> VElement {
            VElement::container("", crate::vdom::NodeKind::Column, Vec::new())
        }

        fn invoke_handler(&self, handler: &str, event: Event) -> bool {
            if handler == "increment" && matches!(event, Event::Click) {
                self.handler_called.set(true);
                return true;
            }
            false
        }

        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }
    }

    /// `dispatch_by_name("close", Mousemove)` → `false`.
    #[test]
    fn mousemove_does_not_trigger_builtin_close() {
        let c = FakeComp::default();
        // close() шлёт WindowCommand через глобальный канал; в unit-тесте
        // главное — что базовая реализация НЕ доходит до self.close().
        let result = c.dispatch_by_name("close", Event::Mousemove { x: 1.0, y: 1.0 });
        assert!(
            !result,
            "non-Click events must not trigger builtin handlers"
        );
    }

    /// `dispatch_by_name("minimize", Mouseenter)` → `false`.
    #[test]
    fn mouseenter_does_not_trigger_builtin_minimize() {
        let c = FakeComp::default();
        let result = c.dispatch_by_name("minimize", Event::Mouseenter);
        assert!(
            !result,
            "non-Click events must not trigger builtin handlers"
        );
    }

    /// `dispatch_by_name("increment", Mousemove)` → `false`, обработчик не вызван.
    #[test]
    fn mousemove_does_not_trigger_user_handler() {
        let c = FakeComp::default();
        let result = c.dispatch_by_name("increment", Event::Mousemove { x: 2.0, y: 3.0 });
        assert!(!result, "user handler must not fire on Mousemove");
        assert!(
            !c.handler_called.get(),
            "invoke_handler must not be called on Mousemove"
        );
    }

    /// `dispatch_by_name("increment", Click)` → `true`, обработчик вызван.
    #[test]
    fn click_triggers_user_handler() {
        let c = FakeComp::default();
        let result = c.dispatch_by_name("increment", Event::Click);
        assert!(result, "user handler must fire on Click");
        assert!(
            c.handler_called.get(),
            "invoke_handler must be called on Click"
        );
    }
}
