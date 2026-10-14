//! Тесты раскладки, поиска узлов по точке и областей перерисовки.

use std::collections::BTreeSet;

use relay::layout::{Layout, Point, Rect, SPACING, TextMetrics, damage_regions, snapshot_rects};
use relay::vdom::{Align, NodeKind, Offset, Size, Style, VElement, diff};

/// Метрики заглушка: знак шириной 8px, высота строки 16px.
struct StubMetrics;

impl TextMetrics for StubMetrics {
    fn measure(&self, text: &str, _font_size: f32) -> (f32, f32) {
        (text.chars().count() as f32 * 8.0, 16.0)
    }
}

fn text(key: &'static str, content: &str) -> VElement {
    VElement::leaf(
        key,
        NodeKind::Text {
            content: content.to_owned(),
        },
    )
}

fn button(key: &'static str, label: &str, disabled: bool) -> VElement {
    VElement::container(
        key,
        NodeKind::Button {
            disabled,
            button_type: relay::vdom::ButtonType::Submit,
        },
        vec![text("t", label)],
    )
}

fn column(key: &'static str, children: Vec<VElement>) -> VElement {
    VElement::container(key, NodeKind::Column, children)
}

fn build(root: &VElement) -> Layout {
    Layout::build(root, (400.0, 300.0), &StubMetrics)
}

#[test]
fn column_stacks_children_with_spacing_and_zero_default_padding() {
    let root = column("", vec![text("0", "АБВ"), text("1", "Г")]);
    let layout = build(&root);

    let first = layout.rect_of("0").expect("первый узел");
    assert_eq!(
        first,
        Rect::new(0.0, 0.0, 24.0, 16.0),
        "padding по умолчанию 0"
    );
    let second = layout.rect_of("1").expect("второй узел");
    assert_eq!(second.x, 0.0);
    assert_eq!(second.y, 16.0 + SPACING, "второй узел ниже первого");

    let root_rect = layout.rect_of("").expect("корень");
    assert_eq!(
        root_rect,
        Rect::new(0.0, 0.0, 400.0, 300.0),
        "корень — окно"
    );
}

#[test]
fn padding_and_margin_offset_content() {
    let mut root = column("", vec![text("0", "АБ")]);
    root.style = Style {
        padding: 10.0,
        ..Style::default()
    };
    let layout = build(&root);

    let child = layout.rect_of("0").expect("дочерний узел");
    assert_eq!(child, Rect::new(10.0, 10.0, 16.0, 16.0), "padding сдвигает");

    let mut root = column("", vec![text("0", "АБ")]);
    root.style = Style {
        margin: 5.0,
        ..Style::default()
    };
    let layout = build(&root);
    let child = layout.rect_of("0").expect("дочерний узел");
    assert_eq!(child, Rect::new(5.0, 5.0, 16.0, 16.0), "margin сдвигает");
}

#[test]
fn percentage_size_is_relative_to_parent() {
    let mut child = text("0", "АБ");
    child.style = Style {
        width: Size::Percent(50.0),
        height: Size::Percent(25.0),
        ..Style::default()
    };
    let root = column("", vec![child]);
    let layout = build(&root);

    let rect = layout.rect_of("0").expect("дочерний узел");
    assert_eq!(rect.w, 200.0, "50% от ширины окна");
    assert_eq!(rect.h, 75.0, "25% от высоты окна");
}

#[test]
fn row_places_children_side_by_side() {
    let row = VElement::container(
        "0",
        NodeKind::Row,
        vec![text("0.0", "АБ"), text("0.1", "В")],
    );
    let root = column("", vec![row]);
    let layout = build(&root);

    let left = layout.rect_of("0.0").expect("левый");
    let right = layout.rect_of("0.1").expect("правый");
    assert_eq!(left.y, right.y, "в одной строке");
    assert_eq!(right.x, left.x + left.w + SPACING, "справа с отступом");
}

#[test]
fn button_centers_label_within_fixed_size() {
    let mut btn = button("0", "АБ", false);
    btn.style = Style {
        width: Size::Px(200.0),
        height: Size::Px(100.0),
        ..Style::default()
    };
    let root = column("", vec![btn]);
    let layout = build(&root);

    let button_rect = layout.rect_of("0").expect("кнопка");
    assert_eq!(
        button_rect,
        Rect::new(0.0, 0.0, 200.0, 100.0),
        "размер задан"
    );
    let label = layout.rect_of("t").expect("подпись");
    assert!(
        (label.x + label.w / 2.0 - 100.0).abs() < 0.01,
        "подпись по центру по X"
    );
    assert!(
        (label.y + label.h / 2.0 - 50.0).abs() < 0.01,
        "подпись по центру по Y"
    );
}

#[test]
fn center_alignment_positions_node_in_container() {
    let mut child = text("0", "АБ");
    child.style = Style {
        align_x: Align::Center,
        align_y: Align::Center,
        ..Style::default()
    };
    let mut frame = column("c", vec![child]);
    frame.style = Style {
        width: Size::Px(400.0),
        height: Size::Px(200.0),
        ..Style::default()
    };
    let root = column("", vec![frame]);
    let layout = build(&root);

    assert_eq!(
        layout.rect_of("0").expect("узел"),
        Rect::new(192.0, 92.0, 16.0, 16.0),
        "position-x/y=center выравнивает по обеим осям"
    );
}

#[test]
fn auto_container_stretches_in_vertical_flow() {
    let frame = column("c", vec![text("0", "АБ"), text("1", "ВГ")]);
    let root = column("", vec![frame]);
    let layout = build(&root);

    assert_eq!(
        layout.rect_of("c").expect("рамка"),
        Rect::new(0.0, 0.0, 400.0, 16.0 + SPACING + 16.0),
        "в вертикальном потоке контейнер без width растягивается по ширине"
    );
}

#[test]
fn auto_container_hugs_content_in_horizontal_flow() {
    // В горизонтальном потоке ширина — главная ось: контейнер по содержимому.
    let inner = column("inner", vec![text("0", "АБ"), text("1", "ВГ")]);
    let row = VElement::container("row", NodeKind::Row, vec![inner]);
    let root = column("", vec![row]);
    let layout = build(&root);

    assert_eq!(
        layout.rect_of("inner").expect("рамка"),
        Rect::new(0.0, 0.0, 16.0, 16.0 + SPACING + 16.0),
        "в горизонтальном потоке ширина контейнера — по содержимому"
    );
}

#[test]
fn nested_auto_containers_stretch_recursively() {
    let mut leaf = text("0.0.0", "А");
    leaf.style = Style {
        width: Size::Percent(100.0),
        ..Style::default()
    };
    let mut inner = column("0.0", vec![leaf]);
    inner.style = Style {
        padding: 10.0,
        ..Style::default()
    };
    let mut outer = column("0", vec![inner]);
    outer.style = Style {
        padding: 10.0,
        ..Style::default()
    };
    let root = column("", vec![outer]);
    let layout = build(&root);

    // 400 → outer content 380 → inner content 360 → leaf 100% = 360.
    assert_eq!(layout.rect_of("0").expect("внешний").w, 400.0);
    assert_eq!(layout.rect_of("0.0").expect("внутренний").w, 380.0);
    assert_eq!(layout.rect_of("0.0.0").expect("лист").w, 360.0);
}

#[test]
fn percent_width_of_auto_vertical_container_resolves_to_parent_content() {
    let mut field = text("0", "АБ");
    field.style = Style {
        width: Size::Percent(100.0),
        ..Style::default()
    };
    let mut form = column("form", vec![field]);
    form.style = Style {
        padding: 12.0,
        ..Style::default()
    };
    let root = column("", vec![form]);
    let layout = build(&root);

    assert_eq!(
        layout.rect_of("form").expect("форма").w,
        400.0,
        "форма растягивается на ширину окна"
    );
    assert_eq!(
        layout.rect_of("0").expect("поле").w,
        376.0,
        "100% от контентной ширины формы (400 - 2×12)"
    );
}

#[test]
fn percent_width_nested_three_levels() {
    let mut level3 = column("0.0.0", vec![]);
    level3.style = Style {
        width: Size::Percent(100.0),
        ..Style::default()
    };
    let mut level2 = column("0.0", vec![level3]);
    level2.style = Style {
        width: Size::Percent(100.0),
        padding: 10.0,
        ..Style::default()
    };
    let mut level1 = column("0", vec![level2]);
    level1.style = Style {
        width: Size::Px(400.0),
        padding: 10.0,
        ..Style::default()
    };
    let layout = build(&level1);

    // 400 → content 380 → level2 = 380 → content 360 → level3 = 360.
    assert_eq!(layout.rect_of("0.0.0").expect("третий").w, 360.0);
}

#[test]
fn percent_width_button_stays_clickable() {
    let mut btn = button("0", "OK", false);
    btn.style = Style {
        width: Size::Percent(100.0),
        ..Style::default()
    };
    let mut frame = column("c", vec![btn]);
    frame.style = Style {
        width: Size::Px(200.0),
        ..Style::default()
    };
    let root = column("", vec![frame]);
    let layout = build(&root);

    let rect = layout.rect_of("0").expect("кнопка");
    assert_eq!(rect.w, 200.0, "100% от ширины родителя");
    assert!(
        layout
            .hit_interactive(Point::new(100.0, rect.y + rect.h / 2.0))
            .is_some(),
        "кнопка остаётся кликабельной"
    );
}

#[test]
fn percent_height_does_not_collapse_with_auto_parent() {
    let mut inner = column("0", vec![]);
    inner.style = Style {
        height: Size::Percent(50.0),
        ..Style::default()
    };
    let mut frame = column("c", vec![inner]);
    frame.style = Style {
        width: Size::Px(200.0),
        ..Style::default()
    };
    let root = column("", vec![frame]);
    let layout = build(&root);

    let rect = layout.rect_of("0").expect("внутренний");
    assert!(rect.h.is_finite(), "высота не NaN/бесконечность");
    assert_eq!(rect.h, 0.0, "50% от Auto-высоты родителя => 0");
}

#[test]
fn position_is_relative_to_parent_content_box() {
    // Окно 400×300, рамка 200×100 в левом верхнем углу.
    let mut child = text("0", "АБ");
    child.style = Style {
        width: Size::Px(100.0),
        align_x: Align::End,
        align_y: Align::End,
        ..Style::default()
    };
    let mut frame = column("c", vec![child]);
    frame.style = Style {
        width: Size::Px(200.0),
        height: Size::Px(100.0),
        ..Style::default()
    };
    let root = column("", vec![frame]);
    let layout = build(&root);

    assert_eq!(
        layout.rect_of("0").expect("узел"),
        Rect::new(100.0, 84.0, 100.0, 16.0),
        "выравнивание относительно родителя, а не окна"
    );
}

#[test]
fn percentage_size_is_relative_to_parent_content_box() {
    let mut child = text("0", "АБ");
    child.style = Style {
        width: Size::Percent(50.0),
        height: Size::Percent(50.0),
        ..Style::default()
    };
    let mut frame = column("c", vec![child]);
    frame.style = Style {
        width: Size::Px(200.0),
        height: Size::Px(100.0),
        padding: 10.0,
        ..Style::default()
    };
    let root = column("", vec![frame]);
    let layout = build(&root);

    assert_eq!(
        layout.rect_of("0").expect("узел"),
        Rect::new(10.0, 10.0, 90.0, 40.0),
        "проценты от контентной области родителя с учётом padding"
    );
}

#[test]
fn percentage_of_auto_parent_outside_flow_resolves_to_zero() {
    let mut child = text("0", "АБ");
    child.style = Style {
        width: Size::Percent(50.0),
        height: Size::Percent(50.0),
        ..Style::default()
    };
    // Родитель без width в горизонтальном потоке — ширина по содержимому,
    // базы для процента нет (документированное поведение).
    let frame = column("c", vec![child]);
    let row = VElement::container("row", NodeKind::Row, vec![frame]);
    let root = column("", vec![row]);
    let layout = build(&root);

    let rect = layout.rect_of("0").expect("узел");
    assert_eq!(rect.w, 0.0, "50% от Auto-ширины вне flow => 0");
    assert_eq!(rect.h, 0.0, "50% от Auto-высоты => 0");
}

#[test]
fn position_offset_applies_after_alignment() {
    let mut child = text("0", "АБ");
    child.style = Style {
        align_x: Align::End,
        offset_x: Offset::Px(-10.0),
        ..Style::default()
    };
    let mut frame = column("c", vec![child]);
    frame.style = Style {
        width: Size::Px(200.0),
        height: Size::Px(100.0),
        ..Style::default()
    };
    let root = column("", vec![frame]);
    let layout = build(&root);

    assert_eq!(
        layout.rect_of("0").expect("узел").x,
        174.0,
        "offset применяется после выравнивания End"
    );
}

#[test]
fn percentage_position_offset_is_relative_to_parent() {
    let mut child = text("0", "АБ");
    child.style = Style {
        offset_x: Offset::Percent(25.0),
        ..Style::default()
    };
    let mut frame = column("c", vec![child]);
    frame.style = Style {
        width: Size::Px(200.0),
        height: Size::Px(100.0),
        ..Style::default()
    };
    let root = column("", vec![frame]);
    let layout = build(&root);

    assert_eq!(
        layout.rect_of("0").expect("узел").x,
        50.0,
        "процент offset — от ширины контентной области родителя"
    );
}

#[test]
fn bare_text_gets_layout_rect() {
    let mut root = column("", vec![text("0", "АБВ")]);
    root.style = Style {
        padding: 8.0,
        ..Style::default()
    };
    let layout = build(&root);

    assert_eq!(
        layout.rect_of("0").expect("текст"),
        Rect::new(8.0, 8.0, 24.0, 16.0),
        "голый текст получает прямоугольник внутри padding"
    );
}

#[test]
fn hit_test_finds_button_and_misses_outside() {
    let root = column("", vec![button("0", "Ок", false)]);
    let layout = build(&root);
    let rect = layout.rect_of("0").expect("кнопка");

    let hit = layout
        .hit_interactive(Point::new(rect.x + rect.w / 2.0, rect.y + rect.h / 2.0))
        .expect("кнопка под курсором");
    assert_eq!(hit.key, "0");
    assert!(
        layout.hit_interactive(Point::new(390.0, 290.0)).is_none(),
        "вне кнопки — промах"
    );
}

#[test]
fn hit_skips_disabled_widgets() {
    let root = column("", vec![button("0", "Ок", true)]);
    let layout = build(&root);
    let rect = layout.rect_of("0").expect("кнопка");
    assert!(
        layout
            .hit_interactive(Point::new(rect.x + rect.w / 2.0, rect.y + rect.h / 2.0))
            .is_none(),
        "заблокированная кнопка не кликается"
    );
}

#[test]
fn no_damage_when_nothing_changed() {
    let root = column("", vec![text("0", "А"), text("1", "Б")]);
    let before = build(&root);
    let old_rects = snapshot_rects(&before);
    let after = build(&root);

    let damage = damage_regions(&old_rects, &after, &BTreeSet::new());
    assert!(damage.is_empty(), "без изменений нет областей перерисовки");
}

#[test]
fn content_change_damages_only_that_node() {
    let old_root = column("", vec![text("0", "А"), text("1", "Б")]);
    let new_root = column("", vec![text("0", "АА"), text("1", "Б")]);
    let before = build(&old_root);
    let old_rects = snapshot_rects(&before);
    let after = build(&new_root);
    let patch = diff(&old_root, &new_root);

    let damage = damage_regions(&old_rects, &after, &patch.changed);
    assert_eq!(
        damage.len(),
        1,
        "повреждена только область изменившегося узла"
    );
    let expected = before
        .rect_of("0")
        .expect("старый rect")
        .union(after.rect_of("0").expect("новый rect"));
    assert_eq!(
        damage[0], expected,
        "область — объединение старого и нового"
    );
}

#[test]
fn removed_node_and_shifted_siblings_get_damage() {
    let old_root = column("", vec![text("0", "А"), text("1", "Б")]);
    let new_root = column("", vec![text("1", "Б")]);
    let before = build(&old_root);
    let old_rects = snapshot_rects(&before);
    let after = build(&new_root);
    let patch = diff(&old_root, &new_root);

    let damage = damage_regions(&old_rects, &after, &patch.changed);
    assert!(
        damage.contains(&old_rects[&"0"]),
        "старая область удалённого узла в повреждениях"
    );
    let moved = old_rects[&"1"].union(after.rect_of("1").expect("новый rect «Б»"));
    assert!(damage.contains(&moved), "сдвинутый сосед перерисовывается");
}

#[test]
fn min_and_max_size_clamp_resolved_size() {
    let mut child = text("0", "АБ");
    child.style = Style {
        min_width: Some(Size::Px(120.0)),
        ..Style::default()
    };
    let root = column("", vec![child]);
    let layout = build(&root);
    assert_eq!(layout.rect_of("0").expect("узел").w, 120.0, "min-width");

    let mut child = text("0", "АБ");
    child.style = Style {
        width: Size::Px(50.0),
        max_width: Some(Size::Px(10.0)),
        ..Style::default()
    };
    let root = column("", vec![child]);
    let layout = build(&root);
    assert_eq!(layout.rect_of("0").expect("узел").w, 10.0, "max-width");
}

#[test]
fn hit_stack_orders_deepest_first() {
    let root = column("", vec![button("0", "Ок", false)]);
    let layout = build(&root);
    let rect = layout.rect_of("0").expect("кнопка");
    let stack = layout.hit_stack(Point::new(rect.x + rect.w / 2.0, rect.y + rect.h / 2.0));

    assert_eq!(
        stack.first().map(|node| node.key),
        Some("t"),
        "текст кнопки — самый глубокий узел"
    );
    assert!(
        stack.iter().any(|node| node.key == "0"),
        "кнопка есть в стеке"
    );
    assert_eq!(stack.last().map(|node| node.key), Some(""), "корень");
}

#[test]
fn bare_text_inherits_font_size_and_color() {
    let mut root = column("", vec![text("0", "АБ")]);
    root.style = Style {
        font_size: Some(24.0),
        font_color: Some(0xFF00_00FF),
        ..Style::default()
    };
    let layout = build(&root);

    let child = layout.node("0").expect("текст");
    assert_eq!(child.style.font_size, Some(24.0), "размер унаследован");
    assert_eq!(
        child.style.font_color,
        Some(0xFF00_00FF),
        "цвет унаследован"
    );
}

#[test]
fn hidden_nodes_are_skipped_in_layout() {
    let mut hidden = button("1", "Скрыто", false);
    hidden.style = Style {
        visible: false,
        ..Style::default()
    };
    let root = column("", vec![text("0", "АБ"), hidden, text("2", "ВГ")]);
    let layout = build(&root);

    assert!(
        layout.rect_of("1").is_none(),
        "скрытый узел не попадает в раскладку"
    );
    assert_eq!(
        layout.rect_of("2").expect("третий узел").y,
        16.0 + SPACING,
        "скрытый узел не оставляет промежутка"
    );
    assert_eq!(
        layout
            .hit_stack(Point::new(2.0, 16.0 + SPACING + 2.0))
            .last()
            .map(|node| node.key),
        Some(""),
        "скрытый узел не перехватывает события"
    );
}

fn titlebar(key: &'static str, children: Vec<VElement>) -> VElement {
    VElement::container(key, NodeKind::TitleBar, children)
}

#[test]
fn titlebar_respects_position_x() {
    let mut child = VElement::container("0", NodeKind::Column, vec![]);
    child.style = Style {
        width: Size::Px(50.0),
        height: Size::Px(20.0),
        align_x: Align::End,
        ..Style::default()
    };
    let mut tb = titlebar("tb", vec![child]);
    tb.style = Style {
        width: Size::Percent(100.0),
        height: Size::Px(40.0),
        ..Style::default()
    };
    let root = VElement::container("win", NodeKind::Window, vec![tb]);
    let layout = Layout::build(&root, (400.0, 300.0), &StubMetrics);
    let c = layout.rect_of("0").expect("child");
    assert_eq!(c.x, 350.0, "right of titlebar");
}

#[test]
fn titlebar_respects_position_y() {
    let mut child = VElement::container("0", NodeKind::Column, vec![]);
    child.style = Style {
        width: Size::Px(50.0),
        height: Size::Px(20.0),
        align_y: Align::End,
        ..Style::default()
    };
    let mut tb = titlebar("tb", vec![child]);
    tb.style = Style {
        width: Size::Percent(100.0),
        height: Size::Px(40.0),
        ..Style::default()
    };
    let root = VElement::container("win", NodeKind::Window, vec![tb]);
    let layout = Layout::build(&root, (400.0, 300.0), &StubMetrics);
    let c = layout.rect_of("0").expect("child");
    assert_eq!(c.y, 20.0, "bottom of titlebar");
}

#[test]
fn titlebar_respects_position_center() {
    let child = text("0", "T");
    let mut tb = titlebar("tb", vec![child]);
    tb.style = Style {
        width: Size::Percent(100.0),
        height: Size::Px(40.0),
        ..Style::default()
    };
    // center via align
    let mut child = text("0", "T");
    child.style.align_x = Align::Center;
    child.style.align_y = Align::Center;
    let mut tb = titlebar("tb", vec![child]);
    tb.style = Style {
        width: Size::Percent(100.0),
        height: Size::Px(40.0),
        ..Style::default()
    };
    let root = VElement::container("win", NodeKind::Window, vec![tb]);
    let layout = Layout::build(&root, (400.0, 300.0), &StubMetrics);
    let c = layout.rect_of("0").expect("child");
    assert_eq!(c.x, 196.0); // (400-8)/2
    assert_eq!(c.y, 12.0); // (40-16)/2
}

#[test]
fn titlebar_padding_offsets_content_box() {
    let child = text("0", "T");
    let mut tb = titlebar("tb", vec![child]);
    tb.style = Style {
        width: Size::Percent(100.0),
        height: Size::Px(40.0),
        padding: 6.0,
        ..Style::default()
    };
    let root = VElement::container("win", NodeKind::Window, vec![tb]);
    let layout = Layout::build(&root, (400.0, 300.0), &StubMetrics);
    let c = layout.rect_of("0").expect("child");
    assert_eq!(c.x, 6.0);
    assert_eq!(c.y, 6.0);
}
