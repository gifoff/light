/// Сегмент текстового узла шаблона.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment {
    /// Буквальный текст.
    Literal(String),
    /// Путь к реактивному свойству (например, `state.count`).
    Path(String),
}

/// Атрибут элемента шаблона.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attr {
    /// Имя атрибута (возможно, директива: `@click`, `v-model`, `:disabled`).
    pub name: String,
    /// Значение атрибута в кавычках.
    pub value: String,
}

/// Элемент шаблона.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Element {
    /// Имя тега (как в источнике).
    pub name: String,
    /// Атрибуты в порядке появления.
    pub attrs: Vec<Attr>,
    /// Дочерние узлы.
    pub children: Vec<Node>,
}

/// Узел шаблона.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    /// Элемент с атрибутами и детьми.
    Element(Element),
    /// Текст с интерполяцией.
    Text(Vec<Segment>),
}
