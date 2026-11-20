use light_reactivity::Collection;

pub use crate::component::{DEFAULT_BACKGROUND, WindowConfig, background_rgba};

use super::events::{ButtonType, Resize};
#[cfg(feature = "image-support")]
use super::image::ImageData;
#[cfg(feature = "elements-select")]
use super::style::SelectOption;
use super::style::Style;

#[derive(Debug, Clone, PartialEq)]
/// Тип содержимого виртуального узла (текст, изображение, форма, окно).
pub enum NodeKind {
    /// Контейнер, раскладывающий детей вертикально (`<div>` по умолчанию).
    Column,
    /// Контейнер, раскладывающий детей горизонтально (`direction="horizontal"`).
    Row,
    /// Титульная полоса окна (`<titlebar>`): горизонтальный контейнер, за
    /// который перетаскивается безрамочное окно. Его текст задаёт заголовок.
    TitleBar,
    /// Корневой узел окна (`<window>`). Контейнер клиентской области; несёт
    /// настройки системного окна в поле [`VElement::window`].
    Window,
    /// Текстовый узел.
    Text {
        /// Отображаемый текст.
        content: String,
    },
    /// Изображение (`<image>`), встроенное в бинарник на этапе компиляции.
    #[cfg(feature = "image-support")]
    Image {
        /// Путь к файлу из атрибута `src` (используется как ключ кэша и имя).
        src: &'static str,
        /// Содержимое PNG-файла.
        data: &'static [u8],
        /// Ширина исходного изображения в пикселях.
        width: u32,
        /// Высота исходного изображения в пикселях.
        height: u32,
        /// Альтернативный текст (`alt`) для дерева доступности.
        alt: String,
    },
    /// Изображение, загруженное во время выполнения.
    #[cfg(feature = "image-support")]
    RuntimeImage {
        /// Данные изображения и уникальный ключ кэша.
        image: ImageData,
        /// Альтернативный текст (`alt`) для дерева доступности.
        alt: String,
    },
    /// Кнопка-контейнер: содержимое (`children`) выкладывается горизонтально и
    /// центрируется по обеим осям. Подпись собирается из текста детей.
    Button {
        /// Заблокирована ли кнопка (`:disabled`).
        disabled: bool,
        /// Тип кнопки (`type`): влияет на запуск валидации формы.
        button_type: ButtonType,
    },
    /// Текстовое поле (`<text>`); при `multiline` — многострочное
    /// (аналог HTML `<textarea>`).
    TextField {
        /// Текущее значение (`v-model`).
        value: String,
        /// Подсказка, когда значение пусто (`placeholder`).
        placeholder: String,
        /// Заблокировано ли поле (`:disabled`).
        disabled: bool,
        /// Многострочный режим (`textarea`).
        multiline: bool,
        /// Возможность изменения размера (`resize`).
        resize: Resize,
        /// Скрывать ли вводимый текст (`<password>`): значение отображается
        /// точками, но хранится как есть.
        masked: bool,
    },
    /// Флажок (`<checkbox>`): переключает булево значение `v-model`.
    #[cfg(feature = "elements-checkbox")]
    Checkbox {
        /// Подпись рядом с флажком.
        label: String,
        /// Отмечен ли флажок (`v-model`).
        checked: bool,
        /// Заблокирован ли флажок (`:disabled`).
        disabled: bool,
    },
    /// Переключатель (`<radio>`): выбирает одно значение из группы,
    /// объединённой общим `v-model`.
    #[cfg(feature = "elements-radio")]
    Radio {
        /// Подпись рядом с переключателем.
        label: String,
        /// Значение, записываемое в `v-model` при выборе.
        value: String,
        /// Совпадает ли текущее значение `v-model` с полем `value` варианта
        /// [`NodeKind::Radio`].
        selected: bool,
        /// Заблокирован ли переключатель (`:disabled`).
        disabled: bool,
    },
    /// Выпадающий список (`<select>`).
    #[cfg(feature = "elements-select")]
    Select {
        /// Текущее выбранное значение (`v-model`).
        value: String,
        /// Варианты списка (`<option>`).
        options: Vec<SelectOption>,
        /// Заблокирован ли список (`:disabled`).
        disabled: bool,
    },
    /// Кастомный элемент, зарегистрированный плагином (Plugin API, Этап 3).
    ///
    /// Аргумент — имя тега элемента (например `"datepicker"`). Ядро
    /// обрабатывает такой узел как **атомарное целое**: layout/paint/diff не
    /// заглядывают внутрь поддерева, рендер которого произвёл плагин. При
    /// сравнении деревьев два `Custom`-узла совместимы, если их теги равны
    /// (см. [`kind_compatible`]).
    Custom(&'static str),
}

/// Узел виртуального дерева.
///
/// `key` — стабильный путь к узлу от корня (назначается макросом по позиции
/// в шаблоне): он переживает между отрисовками и служит идентичностью узла
/// при сравнении деревьев.
#[derive(Debug, Clone, PartialEq)]
pub struct VElement {
    /// Стабильный путь-идентификатор узла (`""` — корень).
    pub key: &'static str,
    /// Тип элемента.
    pub kind: NodeKind,
    /// Размеры узла из атрибутов `width`/`height`.
    pub style: Style,
    /// Имя HTML-тега источника (`"div"`, `"button"`, …). Для программно
    /// созданных узлов и текстовых сегментов — пустая строка.
    pub tag: &'static str,
    /// Атрибуты элемента `(имя, значение)` без директив и обработчиков.
    ///
    /// Динамические привязки (`:disabled="locked"`) хранятся под именем без
    /// двоеточия со строковым значением на момент отрисовки.
    pub attrs: Vec<(&'static str, String)>,
    /// Настройки системного окна — только у корневого узла с типом
    /// [`NodeKind::Window`]; у остальных узлов — `None`.
    pub window: Option<WindowConfig>,
    /// Дочерние узлы (пусто для текста и поля).
    pub children: Vec<VElement>,
}

impl VElement {
    /// Создаёт узел с пустым списком детей.
    pub fn leaf(key: &'static str, kind: NodeKind) -> Self {
        Self {
            key,
            kind,
            style: Style::default(),
            tag: "",
            attrs: Vec::new(),
            window: None,
            children: Vec::new(),
        }
    }

    /// Создаёт контейнер с детьми.
    pub fn container(key: &'static str, kind: NodeKind, children: Vec<VElement>) -> Self {
        Self {
            key,
            kind,
            style: Style::default(),
            tag: "",
            attrs: Vec::new(),
            window: None,
            children,
        }
    }

    /// Задаёт размеры узла (атрибуты `width`/`height`).
    pub fn with_style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Задаёт настройки системного окна (для корневого узла [`NodeKind::Window`]).
    pub fn with_window(mut self, config: WindowConfig) -> Self {
        self.window = Some(config);
        self
    }

    /// Настройки системного окна, если узел — окно.
    pub fn window_config(&self) -> Option<&WindowConfig> {
        self.window.as_ref()
    }

    /// Задаёт имя HTML-тега источника.
    pub fn with_tag(mut self, tag: &'static str) -> Self {
        self.tag = tag;
        self
    }

    /// Добавляет один атрибут элемента.
    pub fn with_attr(mut self, name: &'static str, value: impl Into<String>) -> Self {
        self.attrs.push((name, value.into()));
        self
    }

    /// Задаёт список атрибутов элемента.
    pub fn with_attrs(mut self, attrs: Vec<(&'static str, String)>) -> Self {
        self.attrs = attrs;
        self
    }

    /// Значение атрибута по имени, если он задан.
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| value.as_str())
    }

    /// JSON-представление узла для инструментов разработки (DevTools).
    ///
    /// Содержит имя HTML-тега (`tag`), ключ узла (`key`), атрибуты (`attrs`,
    /// пары `[имя, значение]`) и рекурсивно собранные дети (`children`).
    /// Ключ нужен инспектору: по нему клиент формирует команду `CLICK <key>`.
    /// Тип узла (`kind`) и стиль намеренно не сериализуются — их полный разбор
    /// требует перебора вариантов с условной компиляцией и вынесен в отдельную
    /// задачу. Метод доступен всегда (0.15.1): без фичи `devtools` его просто
    /// никто из библиотеки не вызывает, а `serde_json` больше не опционален.
    pub fn to_json(&self) -> serde_json::Value {
        let children: Vec<_> = self.children.iter().map(VElement::to_json).collect();
        serde_json::json!({
            "tag": self.tag,
            "key": self.key,
            "attrs": &self.attrs,
            "children": children,
        })
    }

    /// Ищет узел по ключу (обход в глубину).
    pub fn get(&self, key: &str) -> Option<&VElement> {
        if self.key == key {
            return Some(self);
        }
        self.children.iter().find_map(|child| child.get(key))
    }

    /// Первый узел поддерева с указанным именем HTML-тега.
    pub fn find_by_tag(&self, tag: &str) -> Option<&VElement> {
        if self.tag == tag {
            return Some(self);
        }
        self.children
            .iter()
            .find_map(|child| child.find_by_tag(tag))
    }

    /// Первый узел поддерева с атрибутом `name`, равным `value`.
    pub fn find_by_attr(&self, name: &str, value: &str) -> Option<&VElement> {
        if self.attr(name) == Some(value) {
            return Some(self);
        }
        self.children
            .iter()
            .find_map(|child| child.find_by_attr(name, value))
    }

    /// Ищет узел по ключу для изменения (обход в глубину).
    pub fn get_mut(&mut self, key: &str) -> Option<&mut VElement> {
        if self.key == key {
            return Some(self);
        }
        self.children
            .iter_mut()
            .find_map(|child| child.get_mut(key))
    }

    /// Находит узел по ключу (read-only, обход в глубину).
    pub fn find_by_key(&self, key: &str) -> Option<&VElement> {
        if self.key == key {
            return Some(self);
        }
        self.children
            .iter()
            .find_map(|child| child.find_by_key(key))
    }

    /// Текстовое содержимое узла для поиска (только у текстовых узлов).
    pub fn content(&self) -> Option<&str> {
        match &self.kind {
            NodeKind::Text { content } => Some(content),
            _ => None,
        }
    }

    /// Все узлы поддерева (включая сам узел) в порядке обхода в глубину.
    ///
    /// Возвращается [`Collection`], поэтому результат можно сразу фильтровать
    /// и преобразовывать цепочкой методов.
    pub fn descendants(&self) -> Collection<&VElement> {
        let mut nodes = Collection::new();
        self.collect_descendants(&mut nodes);
        nodes
    }

    fn collect_descendants<'a>(&'a self, out: &mut Collection<&'a VElement>) {
        out.push(self);
        for child in &self.children {
            child.collect_descendants(out);
        }
    }

    /// Ищет первый узел с заданным текстовым содержимым.
    pub fn find_by_content(&self, content: &str) -> Option<&VElement> {
        if self.content() == Some(content) {
            return Some(self);
        }
        self.children
            .iter()
            .find_map(|child| child.find_by_content(content))
    }

    /// Ищет первый узел, содержимое которого содержит подстроку `needle`.
    pub fn find_containing(&self, needle: &str) -> Option<&VElement> {
        if self
            .content()
            .is_some_and(|content| content.contains(needle))
        {
            return Some(self);
        }
        self.children
            .iter()
            .find_map(|child| child.find_containing(needle))
    }

    /// Первый узел титульной полосы (`<titlebar>`), если он есть.
    fn find_titlebar(&self) -> Option<&VElement> {
        if matches!(self.kind, NodeKind::TitleBar) {
            return Some(self);
        }
        self.children.iter().find_map(VElement::find_titlebar)
    }

    /// Собирает простой текст поддерева, пропуская интерактивные элементы.
    fn collect_plain_text(&self, out: &mut String) {
        match &self.kind {
            NodeKind::Text { content } => out.push_str(content),
            NodeKind::Button { .. } | NodeKind::TextField { .. } => {}
            #[cfg(feature = "elements-checkbox")]
            NodeKind::Checkbox { .. } => {}
            #[cfg(feature = "elements-radio")]
            NodeKind::Radio { .. } => {}
            #[cfg(feature = "elements-select")]
            NodeKind::Select { .. } => {}
            _ => {
                for child in &self.children {
                    child.collect_plain_text(out);
                }
            }
        }
    }

    /// ТЗ 0.16.0: публичная версия [`collect_plain_text`] — собирает текст
    /// поддерева (пропуская интерактивные элементы) в переданную строку.
    ///
    /// Используется командой `FIND` DevTools для поиска кнопки по подписи:
    /// текст кнопки = конкатенация текстовых потомков, без вложенных
    /// кнопок/полей.
    pub fn collect_plain_text_public(&self, out: &mut String) {
        self.collect_plain_text(out)
    }

    /// Заголовок окна, выведенный из текста первого `<titlebar>`.
    ///
    /// Интерактивные элементы (кнопки управления) в заголовок не попадают.
    /// Возвращает `None`, если титульной полосы нет или её текст пуст.
    pub fn titlebar_text(&self) -> Option<String> {
        let bar = self.find_titlebar()?;
        let mut text = String::new();
        bar.collect_plain_text(&mut text);
        let trimmed = text.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_owned())
        }
    }
}
