//! Курсоры: перевод `Cursor` в `CursorIcon`, определение курсора по узлу.

use light_vdom::layout::{Layout, Point};
use light_vdom::{Cursor, NodeKind, VElement};
use winit::window::CursorIcon;

pub(super) fn cursor_icon(cursor: Cursor) -> CursorIcon {
    match cursor {
        Cursor::Default => CursorIcon::Default,
        Cursor::Pointer => CursorIcon::Pointer,
        Cursor::Text => CursorIcon::Text,
        Cursor::NotAllowed => CursorIcon::NotAllowed,
        Cursor::Wait => CursorIcon::Wait,
        Cursor::Progress => CursorIcon::Progress,
        Cursor::Help => CursorIcon::Help,
        Cursor::Crosshair => CursorIcon::Crosshair,
        Cursor::Move => CursorIcon::Move,
        Cursor::Grab => CursorIcon::Grab,
        Cursor::Grabbing => CursorIcon::Grabbing,
        Cursor::EwResize => CursorIcon::EwResize,
        Cursor::NsResize => CursorIcon::NsResize,
        Cursor::NeswResize => CursorIcon::NeswResize,
        Cursor::NwseResize => CursorIcon::NwseResize,
    }
}

/// Курсор, который узел задаёт сам (`cursor="…"`), либо `None`.
pub(super) fn explicit_cursor(element: &VElement) -> Option<Cursor> {
    element.attr("cursor").and_then(Cursor::from_name)
}

/// Похож ли узел на ссылку: `role="link"` или HTML-тег `a`.
pub(super) fn is_link(element: &VElement) -> bool {
    element.tag.eq_ignore_ascii_case("a")
        || element
            .attr("role")
            .is_some_and(|role| role.eq_ignore_ascii_case("link"))
}

/// Курсор по умолчанию для узла, если атрибут `cursor` не задан.
///
/// Отключённые элементы показывают `not-allowed`, ссылки и элементы управления
/// — `pointer`, поля ввода — `text`, остальные — системный курсор.
pub fn default_cursor_for(element: &VElement) -> Cursor {
    if crate::a11y::is_disabled(element) {
        return Cursor::NotAllowed;
    }
    if is_link(element) {
        return Cursor::Pointer;
    }
    match &element.kind {
        NodeKind::Button { .. } => Cursor::Pointer,
        #[cfg(feature = "elements-checkbox")]
        NodeKind::Checkbox { .. } => Cursor::Pointer,
        #[cfg(feature = "elements-radio")]
        NodeKind::Radio { .. } => Cursor::Pointer,
        #[cfg(feature = "elements-select")]
        NodeKind::Select { .. } => Cursor::Pointer,
        NodeKind::TextField { .. } => Cursor::Text,
        _ => Cursor::Default,
    }
}

/// Курсор для узла: явный `cursor`, иначе значение по роли.
#[allow(dead_code)]
pub fn cursor_for(element: &VElement) -> Cursor {
    explicit_cursor(element).unwrap_or_else(|| default_cursor_for(element))
}

/// Определяет курсор для точки окна.
///
/// Узлы перебираются от самого глубокого к корню (`hit_stack`), поэтому
/// значение может «всплывать»: если у дочернего узла нет собственного курсора
/// (он возвращает `Default`), берётся курсор ближайшего подходящего родителя.
/// Явно заданный `cursor="default"` прекращает всплытие.
pub fn resolve_cursor(layout: &Layout, tree: &VElement, point: Point) -> Cursor {
    for node in layout.hit_stack(point) {
        let Some(element) = tree.get(node.key) else {
            continue;
        };
        if let Some(cursor) = explicit_cursor(element) {
            return cursor;
        }
        let cursor = default_cursor_for(element);
        if cursor != Cursor::Default {
            return cursor;
        }
    }
    Cursor::Default
}

#[cfg(test)]
mod tests {
    use crate::window::keys::keycode_letter;
    use crate::window::render_loop::word_range;
    use crate::window::scrollbar::{prune_scroll_offsets, scrollbar_strip_damage};
    use light_vdom::layout::{Point, Rect};
    use winit::keyboard::KeyCode;

    #[test]
    pub(super) fn keycode_letter_is_layout_independent() {
        assert_eq!(keycode_letter(KeyCode::KeyA).as_deref(), Some("a"));
        assert_eq!(keycode_letter(KeyCode::KeyX).as_deref(), Some("x"));
        assert_eq!(keycode_letter(KeyCode::KeyZ).as_deref(), Some("z"));
        assert_eq!(keycode_letter(KeyCode::Digit1), None);
        assert_eq!(keycode_letter(KeyCode::Escape), None);
    }

    #[test]
    pub(super) fn word_range_selects_word_under_index() {
        assert_eq!(word_range("hello world", 2), (0, 5));
        assert_eq!(word_range("hello world", 8), (6, 11));
        assert_eq!(word_range("hello world", 0), (0, 5));
        assert_eq!(word_range("hello world", 11), (6, 11));
        // Позиция сразу после слова «прилипает» к нему.
        assert_eq!(word_range("hello world", 5), (0, 5));
        assert_eq!(word_range("hello,world", 5), (0, 5));
        // Изолированный несловесный символ выделяется сам.
        assert_eq!(word_range("a  b", 2), (2, 3));
        assert_eq!(word_range("", 0), (0, 0));
        assert_eq!(word_range("word", 4), (0, 4));
        assert_eq!(word_range("_x_9", 1), (0, 4));
    }

    #[test]
    pub(super) fn scrollbar_strip_damage_covers_old_bar_when_content_shrinks() {
        use crate::window::paint as wp;
        use light_vdom::layout::TextMetrics;
        use light_vdom::{NodeKind, Overflow, Size, Style, VElement};

        struct M;
        impl TextMetrics for M {
            fn measure(&self, _text: &str, _size: f32) -> (f32, f32) {
                (0.0, 0.0)
            }
        }

        // Контейнер с overflow-y=auto и padding, внутри — «контент»-колонка
        // полной ширины, но только внутри content box (не захватывает полосу
        // прокрутки, которая лежит в правом padding-крае). Обёрнут в общий
        // корень, чтобы сам контейнер не растянулся на весь viewport.
        fn container(content_height: f32) -> VElement {
            let mut content = VElement::container("c", NodeKind::Column, Vec::new());
            content.style = Style {
                height: Size::Px(content_height),
                ..Style::default()
            };
            let mut scroller = VElement::container("s", NodeKind::Column, vec![content]);
            scroller.style = Style {
                overflow_y: Overflow::Auto,
                height: Size::Px(100.0),
                padding: light_vdom::Edges::all(16.0),
                ..Style::default()
            };
            VElement::container("r", NodeKind::Column, vec![scroller])
        }

        let old = light_vdom::layout::Layout::build(&container(240.0), (400.0, 300.0), &M);
        let new = light_vdom::layout::Layout::build(&container(60.0), (400.0, 300.0), &M);

        let base = light_vdom::layout::damage_regions(
            &light_vdom::layout::snapshot_rects(&old),
            &new,
            &std::collections::BTreeSet::new(),
        );
        // Полоса прокрутки лежит за пределами контента и не попадает в
        // обычные области перерисовки (удалённый/сдвинутый контент её не
        // накрывает).
        let bar = wp::SCROLLBAR_SIZE;
        let old_container = old.rect_of("s").expect("контейнер");
        let right_strip = Rect::new(
            old_container.x + old_container.w - bar,
            old_container.y,
            bar,
            old_container.h,
        );
        assert!(
            base.iter()
                .all(|rect| rect.x + rect.w <= old_container.x + old_container.w - bar + 0.5),
            "обычный damage не покрывает правую полосу прокрутки"
        );

        let strips = scrollbar_strip_damage(&old, &new, 1.0);
        assert!(
            strips.contains(&right_strip),
            "область старой полосы прокрутки попадает в damage: {strips:?}"
        );
    }

    #[test]
    pub(super) fn prune_scroll_offsets_drops_offsets_for_containers_without_scroll() {
        use light_vdom::layout::{Layout, TextMetrics};
        use light_vdom::{NodeKind, Overflow, Size, Style, VElement};

        struct M;
        impl TextMetrics for M {
            fn measure(&self, _text: &str, _size: f32) -> (f32, f32) {
                (0.0, 0.0)
            }
        }

        fn container(content_height: f32) -> VElement {
            let mut content = VElement::container("c", NodeKind::Column, Vec::new());
            content.style = Style {
                height: Size::Px(content_height),
                ..Style::default()
            };
            let mut scroller = VElement::container("s", NodeKind::Column, vec![content]);
            scroller.style = Style {
                overflow_y: Overflow::Auto,
                height: Size::Px(100.0),
                ..Style::default()
            };
            VElement::container("r", NodeKind::Column, vec![scroller])
        }

        let tall = Layout::build(&container(240.0), (400.0, 300.0), &M);
        let mut offsets = std::collections::HashMap::new();
        offsets.insert("s", Point::new(0.0, 50.0));
        prune_scroll_offsets(&mut offsets, &tall);
        assert_eq!(
            offsets.get("s"),
            Some(&Point::new(0.0, 50.0)),
            "пока контейнер прокручивается, смещение сохраняется"
        );

        let short = Layout::build(&container(60.0), (400.0, 300.0), &M);
        prune_scroll_offsets(&mut offsets, &short);
        assert!(
            offsets.is_empty(),
            "смещение сброшено, когда контент больше не прокручивается"
        );
    }
}
