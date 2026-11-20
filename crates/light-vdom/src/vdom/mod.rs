//! Виртуальное дерево интерфейса и его сравнение.
//!
//! Каждая отрисовка компонента строит новое дерево [`VElement`]; функция
//! [`diff`] сравнивает его с предыдущим и возвращает список точечных
//! изменений ([`Op`]). Неизменившиеся узлы не попадают ни в один оп —
//! именно это обеспечивает перерисовку только изменившихся элементов.

/// Сопоставление двух деревьев и генерация списков точечных изменений.
pub mod diff;
/// Типы событий, размеров, выравнивания и другие примитивы VDOM.
pub mod events;
/// Изображения, загруженные во время выполнения (feature `image-support`).
#[cfg(feature = "image-support")]
pub mod image;
/// Структура виртуального элемента и трейты узлов дерева.
pub mod node;
/// Операции патчинга и структура результата диффа.
pub mod ops;
/// Парсеры строковых значений в типы стиля VDOM.
pub mod parse;
/// Структуры стилей и опций форм элементов.
pub mod style;
#[cfg(test)]
mod tests;
/// Трейты узлов дерева и их реализации для VElement.
pub mod traits;

pub use diff::diff;
pub use events::{
    Align, ButtonType, Cursor, Edges, Event, KeyInfo, MouseButton, Offset, Overflow, Position,
    RequiredIndicator, Resize, Size, WhiteSpace, intern_key,
};
#[cfg(feature = "image-support")]
pub use image::ImageData;
pub use node::{NodeKind, VElement};
pub use ops::{Op, Patch};
pub use parse::{
    parse_align_items, parse_align_x, parse_align_y, parse_bool, parse_button_type, parse_color,
    parse_justify, parse_offset, parse_overflow, parse_position, parse_px,
    parse_required_indicator, parse_resize, parse_size_or_auto, parse_white_space,
};
pub use style::{SelectOption, Style};
pub use traits::{VNode, VWindow};
