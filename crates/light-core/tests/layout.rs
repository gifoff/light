//! Тесты раскладки, поиска узлов по точке и областей перерисовки.

extern crate light_fw_core as light;

use std::collections::{BTreeSet, HashMap};

use light::layout::{Layout, Point, Rect, SPACING, TextMetrics, damage_regions, snapshot_rects};
use light::vdom::{
    Align, Edges, NodeKind, Offset, Overflow, Position, Size, Style, VElement, WhiteSpace, diff,
};

/// Метрики заглушка: знак шириной 8px, высота строки 16px.
struct StubMetrics;

impl TextMetrics for StubMetrics {
    fn measure(&self, text: &str, _font_size: f32) -> (f32, f32) {
        (text.chars().count() as f32 * 8.0, 16.0)
    }
}

/// Метрики, у которых высота строки зависит от размера шрифта и не включает
/// line-gap: 20px шрифт даёт 18px типографской строки.
struct SizedMetrics;

impl TextMetrics for SizedMetrics {
    fn measure(&self, text: &str, font_size: f32) -> (f32, f32) {
        (
            text.chars().count() as f32 * font_size * 0.5,
            font_size * 0.9,
        )
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
            button_type: light::vdom::ButtonType::Submit,
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
        padding: Edges::all(10.0),
        ..Style::default()
    };
    let layout = build(&root);

    let child = layout.rect_of("0").expect("дочерний узел");
    assert_eq!(child, Rect::new(10.0, 10.0, 16.0, 16.0), "padding сдвигает");

    let mut root = column("", vec![text("0", "АБ")]);
    root.style = Style {
        margin: Edges::all(5.0),
        ..Style::default()
    };
    let layout = build(&root);
    let child = layout.rect_of("0").expect("дочерний узел");
    assert_eq!(child, Rect::new(5.0, 5.0, 16.0, 16.0), "margin сдвигает");
}

#[test]
fn asymmetric_padding_offsets_each_side() {
    let mut root = column("", vec![text("0", "АБ")]);
    root.style = Style {
        padding: Edges {
            top: 1.0,
            right: 2.0,
            bottom: 3.0,
            left: 4.0,
        },
        ..Style::default()
    };
    let layout = build(&root);
    assert_eq!(
        layout.rect_of("0"),
        Some(Rect::new(4.0, 1.0, 16.0, 16.0)),
        "ребёнок смещён на padding-left/padding-top"
    );
}

#[test]
fn asymmetric_margin_offsets_and_steps_siblings() {
    let mut first = text("0", "АБ");
    first.style = Style {
        margin: Edges {
            top: 2.0,
            right: 8.0,
            bottom: 4.0,
            left: 6.0,
        },
        ..Style::default()
    };
    let root = column("", vec![first, text("1", "АБ")]);
    let layout = build(&root);
    assert_eq!(
        layout.rect_of("0"),
        Some(Rect::new(6.0, 2.0, 16.0, 16.0)),
        "margin-left/margin-top смещают узел"
    );
    // Внешняя высота первого: 16 + top 2 + bottom 4 = 22, плюс SPACING 6 → 28.
    assert_eq!(
        layout.rect_of("1"),
        Some(Rect::new(0.0, 28.0, 16.0, 16.0)),
        "шаг учитывает margin-bottom соседа"
    );
}

#[test]
fn container_height_includes_asymmetric_padding() {
    let mut box_node = column("box", vec![text("0", "АБ")]);
    box_node.style = Style {
        width: Size::Px(200.0),
        padding: Edges {
            top: 5.0,
            right: 0.0,
            bottom: 7.0,
            left: 0.0,
        },
        ..Style::default()
    };
    let root = VElement::container("win", NodeKind::Window, vec![box_node]);
    let layout = Layout::build(&root, (400.0, 300.0), &StubMetrics);
    // Высота контейнера = текст 16 + top 5 + bottom 7 = 28.
    assert_eq!(layout.rect_of("box").unwrap().h, 28.0);
    // Ребёнок смещён вниз на padding-top.
    assert_eq!(layout.rect_of("0").unwrap().y, 5.0);
}

#[test]
fn button_per_side_padding_disables_builtin() {
    let mut btn = button("b", "АБ", false);
    btn.style = Style {
        padding: Edges {
            left: 5.0,
            ..Edges::ZERO
        },
        ..Style::default()
    };
    let root = column("", vec![btn]);
    let layout = build(&root);
    // Встроенные отступы 14/9 отключены: ширина = 16 + 5 (слева) = 21,
    // высота = max(16, BUTTON_MIN_H 34) = 34.
    assert_eq!(
        layout.rect_of("b"),
        Some(Rect::new(0.0, 0.0, 21.0, 34.0)),
        "построчный padding отключает встроенные отступы кнопки"
    );
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
fn text_wraps_at_word_boundaries_by_default() {
    let mut label = text("t", "one two three");
    label.style = Style {
        width: Size::Px(70.0),
        ..Style::default()
    };
    let root = column("", vec![label]);
    let layout = build(&root);

    let rect = layout.rect_of("t").expect("текст");
    assert_eq!(rect.w, 70.0, "ширина ограничена явно");
    assert_eq!(rect.h, 16.0 * 2.0, "две строки по словам");
}

#[test]
fn nowrap_is_inherited_and_keeps_single_line() {
    let mut label = text("t", "one two three");
    label.style = Style {
        width: Size::Px(70.0),
        ..Style::default()
    };
    let mut frame = column("frame", vec![label]);
    frame.style = Style {
        white_space: Some(WhiteSpace::Nowrap),
        ..Style::default()
    };
    let root = column("", vec![frame]);
    let layout = build(&root);

    let rect = layout.rect_of("t").expect("текст");
    assert_eq!(rect.h, 16.0, "nowrap не переносит текст");
    assert_eq!(
        layout.node("t").expect("текст").style.white_space,
        Some(WhiteSpace::Nowrap),
        "режим унаследован"
    );
}

#[test]
fn row_stretches_auto_height_children_to_tallest_sibling() {
    let sidebar = column("sidebar", vec![text("short", "А")]);
    let main = column("main", vec![text("a", "А"), text("b", "Б"), text("c", "В")]);
    let row = VElement::container("row", NodeKind::Row, vec![sidebar, main]);
    let root = column("", vec![row]);
    let layout = build(&root);

    assert_eq!(
        layout.rect_of("main").expect("контент").h,
        16.0 + SPACING + 16.0 + SPACING + 16.0
    );
    assert_eq!(
        layout.rect_of("sidebar").expect("сайдбар").h,
        layout.rect_of("main").expect("контент").h,
        "короткий ребёнок Row растягивается по высоте самого высокого соседа"
    );
}

#[test]
fn overflow_auto_scrolls_and_clips_children() {
    let mut list = column("list", vec![text("a", "А"), text("b", "Б"), text("c", "В")]);
    list.style = Style {
        height: Size::Px(50.0),
        overflow_y: Overflow::Auto,
        ..Style::default()
    };
    let root = column("", vec![list]);
    let mut layout = build(&root);

    let list_rect = layout.rect_of("list").expect("список");
    assert_eq!(list_rect.h, 50.0, "высота контейнера ограничена");
    assert_eq!(
        layout.node("list").expect("список").scroll_height,
        16.0 + SPACING + 16.0 + SPACING + 16.0
    );
    assert_eq!(layout.node("list").expect("список").max_scroll_y(), 10.0);

    layout.apply_scroll_offsets(&HashMap::from([("list", Point::new(0.0, 100.0))]));
    assert_eq!(layout.scroll_offset("list").y, 10.0, "смещение клампится");
    assert_eq!(layout.rect_of("a").expect("первый").y, -10.0);
    assert_eq!(
        layout.node("a").expect("первый").clip_rect,
        Some(Rect::new(0.0, 0.0, 400.0, 50.0))
    );
    assert_eq!(
        layout
            .scroll_target(Point::new(10.0, 10.0), false)
            .map(|node| node.key),
        Some("list")
    );
}

#[test]
fn absolute_child_is_centered_and_out_of_flow() {
    let mut panel = column("panel", vec![text("label", "ОК")]);
    panel.style = Style {
        position: Position::Absolute,
        align_x: Align::Center,
        align_y: Align::Center,
        width: Size::Px(100.0),
        height: Size::Px(50.0),
        ..Style::default()
    };
    let root = column("", vec![text("flow", "А"), panel]);
    let layout = build(&root);

    assert_eq!(layout.rect_of("flow").expect("поток").y, 0.0);
    assert_eq!(
        layout.rect_of("panel").expect("панель"),
        Rect::new(150.0, 125.0, 100.0, 50.0),
        "absolute центрируется в content box и не сдвигает поток"
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
        padding: Edges::all(10.0),
        ..Style::default()
    };
    let mut outer = column("0", vec![inner]);
    outer.style = Style {
        padding: Edges::all(10.0),
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
        padding: Edges::all(12.0),
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
        padding: Edges::all(10.0),
        ..Style::default()
    };
    let mut level1 = column("0", vec![level2]);
    level1.style = Style {
        width: Size::Px(400.0),
        padding: Edges::all(10.0),
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
        padding: Edges::all(10.0),
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
        padding: Edges::all(8.0),
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
        padding: Edges::all(6.0),
        ..Style::default()
    };
    let root = VElement::container("win", NodeKind::Window, vec![tb]);
    let layout = Layout::build(&root, (400.0, 300.0), &StubMetrics);
    let c = layout.rect_of("0").expect("child");
    assert_eq!(c.x, 6.0);
    assert_eq!(c.y, 6.0);
}

/// Узел с заданным `z` (остальные стили по умолчанию).
fn ztext(key: &'static str, content: &str, z: i32) -> VElement {
    let mut node = text(key, content);
    node.style.z = z;
    node
}

#[test]
fn z_orders_children_for_painting() {
    let root = column(
        "",
        vec![ztext("a", "A", 2), ztext("b", "B", 0), ztext("c", "C", 1)],
    );
    let layout = build(&root);
    let keys: Vec<&str> = layout.nodes().iter().map(|node| node.key).collect();
    assert_eq!(
        keys,
        vec!["", "b", "c", "a"],
        "соседи рисуются по возрастанию z"
    );
}

#[test]
fn z_equal_preserves_dom_order() {
    let root = column(
        "",
        vec![ztext("a", "A", 1), ztext("b", "B", 1), ztext("c", "C", 1)],
    );
    let layout = build(&root);
    let keys: Vec<&str> = layout.nodes().iter().map(|node| node.key).collect();
    assert_eq!(
        keys,
        vec!["", "a", "b", "c"],
        "равные z сохраняют DOM-порядок"
    );
}

#[test]
fn z_does_not_move_nodes() {
    let root = column("", vec![ztext("a", "A", 5), ztext("b", "B", 0)]);
    let layout = build(&root);
    assert_eq!(
        layout.rect_of("a"),
        Some(Rect::new(0.0, 0.0, 8.0, 16.0)),
        "z не влияет на раскладку"
    );
    assert_eq!(
        layout.rect_of("b"),
        Some(Rect::new(0.0, 16.0 + SPACING, 8.0, 16.0))
    );
}

#[test]
fn z_orders_hit_test_for_overlap() {
    let mut a = button("a", "AA", false);
    a.style.z = 1;
    let mut b = button("b", "AA", false);
    b.style.offset_y = Offset::Px(-30.0);
    let root = column("", vec![a, b]);
    let layout = build(&root);

    let top = layout
        .hit_interactive(Point::new(10.0, 20.0))
        .expect("точка внутри перекрытия");
    assert_eq!(top.key, "a", "узел с большим z выше под точкой");
}

#[test]
fn z_default_keeps_later_sibling_on_top() {
    let b = button("b", "AA", false);
    let mut a = button("a", "AA", false);
    a.style.offset_y = Offset::Px(30.0);
    let root = column("", vec![a, b]);
    let layout = build(&root);

    let top = layout
        .hit_interactive(Point::new(10.0, 40.0))
        .expect("точка внутри перекрытия");
    assert_eq!(top.key, "b", "при равном z выше поздний сосед");
}

#[test]
fn z_is_local_to_parent() {
    let inner = VElement::container("p1", NodeKind::Column, vec![ztext("deep", "D", 100)]);
    let root = column("", vec![inner, ztext("p2", "E", 0)]);
    let layout = build(&root);
    let keys: Vec<&str> = layout.nodes().iter().map(|node| node.key).collect();
    assert_eq!(
        keys,
        vec!["", "p1", "deep", "p2"],
        "поддерево остаётся над своим родителем (z локальный)"
    );
}

/// Раскладка при заданном размере окна.
fn layout_at(root: &VElement, width: f32, height: f32) -> Layout {
    Layout::build(root, (width, height), &StubMetrics)
}

fn percent_div(key: &'static str, width: f32, height: f32) -> VElement {
    let mut node = column(key, vec![]);
    node.style = Style {
        width: Size::Percent(width),
        height: Size::Percent(height),
        ..Style::default()
    };
    node
}

#[test]
fn percent_width_recalculates_on_window_resize() {
    let root = column("", vec![percent_div("0", 50.0, 100.0)]);
    assert_eq!(
        layout_at(&root, 400.0, 300.0).rect_of("0").unwrap().w,
        200.0
    );
    assert_eq!(
        layout_at(&root, 800.0, 300.0).rect_of("0").unwrap().w,
        400.0
    );
}

#[test]
fn nested_percent_recalculates_on_resize() {
    let inner = percent_div("0", 50.0, 100.0);
    let mut outer = column("outer", vec![inner]);
    outer.style = Style {
        width: Size::Percent(100.0),
        padding: Edges::all(10.0),
        ..Style::default()
    };
    // 400 → content родителя 380 → 50% = 190
    assert_eq!(layout_at(&outer, 400.0, 0.0).rect_of("0").unwrap().w, 190.0);
    // 800 → content 780 → 50% = 390
    assert_eq!(layout_at(&outer, 800.0, 0.0).rect_of("0").unwrap().w, 390.0);
}

#[test]
fn percent_height_recalculates_on_resize() {
    let root = column("", vec![percent_div("0", 100.0, 50.0)]);
    assert_eq!(layout_at(&root, 0.0, 200.0).rect_of("0").unwrap().h, 100.0);
    assert_eq!(layout_at(&root, 0.0, 400.0).rect_of("0").unwrap().h, 200.0);
}

/// Кнопка-контейнер с заданным содержимым.
fn content_button(key: &'static str, children: Vec<VElement>) -> VElement {
    VElement::container(
        key,
        NodeKind::Button {
            disabled: false,
            button_type: light::vdom::ButtonType::Submit,
        },
        children,
    )
}

fn sized_column(key: &'static str, width: Size, height: Size) -> VElement {
    let mut node = column(key, vec![]);
    node.style = Style {
        width,
        height,
        ..Style::default()
    };
    node
}

#[test]
fn button_without_padding_uses_builtin_padding() {
    // "АБ" = 16×16; ширина = 16 + 2*14, высота = max(16 + 2*9, 34) = 34.
    let root = column("", vec![button("b", "АБ", false)]);
    let layout = build(&root);
    assert_eq!(
        layout.rect_of("b"),
        Some(Rect::new(0.0, 0.0, 44.0, 34.0)),
        "встроенные отступы 14/9 действуют по умолчанию"
    );
}

#[test]
fn button_padding_attribute_removes_builtin_padding() {
    let mut btn = button("b", "АБ", false);
    btn.attrs.push(("padding", "0".to_owned()));
    let root = column("", vec![btn]);
    let layout = build(&root);
    assert_eq!(
        layout.rect_of("b"),
        Some(Rect::new(0.0, 0.0, 16.0, 34.0)),
        "padding=\"0\" убирает встроенные отступы"
    );
}

#[test]
fn button_padding_zero_lets_child_fill() {
    let child = sized_column("d", Size::Px(30.0), Size::Px(30.0));
    let mut btn = content_button("b", vec![child]);
    btn.style = Style {
        height: Size::Px(30.0),
        ..Style::default()
    };
    btn.attrs.push(("padding", "0".to_owned()));
    let mut tb = VElement::container("tb", NodeKind::TitleBar, vec![btn]);
    tb.style = Style {
        width: Size::Px(200.0),
        height: Size::Px(30.0),
        ..Style::default()
    };
    let layout = build(&tb);
    assert_eq!(
        layout.rect_of("b"),
        Some(Rect::new(0.0, 0.0, 30.0, 30.0)),
        "кнопка ужимается ровно по содержимому"
    );
    assert_eq!(
        layout.rect_of("d"),
        Some(Rect::new(0.0, 0.0, 30.0, 30.0)),
        "ребёнок вплотную, без смещения"
    );
}

#[test]
fn percent_child_fills_sized_button_without_padding() {
    let child = sized_column("d", Size::Percent(100.0), Size::Percent(100.0));
    let mut btn = content_button("b", vec![child]);
    btn.style = Style {
        width: Size::Px(100.0),
        height: Size::Px(40.0),
        ..Style::default()
    };
    btn.attrs.push(("padding", "0".to_owned()));
    let root = column("", vec![btn]);
    let layout = build(&root);
    assert_eq!(
        layout.rect_of("d"),
        Some(Rect::new(0.0, 0.0, 100.0, 40.0)),
        "100% от контентной области кнопки"
    );
}

#[test]
fn justify_content_center_centers_children_vertically_in_column() {
    let mut root = column("", vec![sized_column("0", Size::Px(50.0), Size::Px(20.0))]);
    root.style = Style {
        justify_content: Align::Center,
        ..Style::default()
    };
    let layout = layout_at(&root, 200.0, 100.0);
    assert_eq!(layout.rect_of("0").unwrap().y, 40.0, "(100 - 20) / 2");
}

#[test]
fn justify_content_end_pushes_children_to_end() {
    let mut root = column("", vec![sized_column("0", Size::Px(50.0), Size::Px(20.0))]);
    root.style = Style {
        justify_content: Align::End,
        ..Style::default()
    };
    let layout = layout_at(&root, 200.0, 100.0);
    assert_eq!(layout.rect_of("0").unwrap().y, 80.0, "100 - 20");
}

#[test]
fn align_items_center_centers_children_horizontally_in_column() {
    let mut root = column("", vec![sized_column("0", Size::Px(50.0), Size::Px(20.0))]);
    root.style = Style {
        align_items: Align::Center,
        ..Style::default()
    };
    let layout = layout_at(&root, 200.0, 100.0);
    assert_eq!(layout.rect_of("0").unwrap().x, 75.0, "(200 - 50) / 2");
}

#[test]
fn justify_content_in_row_centers_horizontally() {
    let mut root = VElement::container(
        "",
        NodeKind::Row,
        vec![sized_column("0", Size::Px(50.0), Size::Px(20.0))],
    );
    root.style = Style {
        justify_content: Align::Center,
        ..Style::default()
    };
    let layout = layout_at(&root, 200.0, 100.0);
    assert_eq!(layout.rect_of("0").unwrap().x, 75.0);
}

#[test]
fn align_items_in_row_centers_vertically() {
    let mut root = VElement::container(
        "",
        NodeKind::Row,
        vec![sized_column("0", Size::Px(50.0), Size::Px(20.0))],
    );
    root.style = Style {
        align_items: Align::Center,
        ..Style::default()
    };
    let layout = layout_at(&root, 200.0, 100.0);
    assert_eq!(layout.rect_of("0").unwrap().y, 40.0);
}

#[test]
fn align_items_center_centers_text_visual_box() {
    let mut label = text("label", "Ag");
    label.style.font_size = Some(20.0);
    let mut root = VElement::container("", NodeKind::Row, vec![label]);
    root.style = Style {
        align_items: Align::Center,
        ..Style::default()
    };
    let layout = Layout::build(&root, (200.0, 100.0), &SizedMetrics);
    let rect = layout.rect_of("label").expect("text node");

    assert!((rect.h - 18.0).abs() < 0.01, "height = 20 * 0.9");
    assert!(
        (rect.y - (100.0 - rect.h) / 2.0).abs() < 0.01,
        "center: y = (100 - 18) / 2"
    );
}

#[test]
fn align_items_end_puts_text_visual_box_on_bottom_edge() {
    let mut label = text("label", "Ag");
    label.style.font_size = Some(20.0);
    let mut root = VElement::container("", NodeKind::Row, vec![label]);
    root.style = Style {
        align_items: Align::End,
        ..Style::default()
    };
    let layout = Layout::build(&root, (200.0, 100.0), &SizedMetrics);
    let rect = layout.rect_of("label").expect("text node");

    assert!((rect.h - 18.0).abs() < 0.01, "height = 20 * 0.9");
    assert!((rect.y - 82.0).abs() < 0.01, "end: y = 100 - 18");
    assert!(
        (rect.y + rect.h - 100.0).abs() < 0.01,
        "bottom edge must reach container bottom"
    );
}

#[test]
fn position_self_overrides_align_items() {
    let mut child = sized_column("0", Size::Px(50.0), Size::Px(20.0));
    child.attrs.push(("position-x", "left".to_owned()));
    let mut root = column("", vec![child]);
    root.style = Style {
        align_items: Align::Center,
        ..Style::default()
    };
    let layout = layout_at(&root, 200.0, 100.0);
    assert_eq!(
        layout.rect_of("0").unwrap().x,
        0.0,
        "position-x переопределяет align-items"
    );
}

#[test]
fn flex_grow_fills_free_space_on_main_axis() {
    let mut grow = column("g", vec![]);
    grow.style = Style {
        flex_grow: 1.0,
        ..Style::default()
    };
    let root = column(
        "",
        vec![sized_column("fixed", Size::Px(50.0), Size::Px(20.0)), grow],
    );
    let layout = layout_at(&root, 200.0, 100.0);
    let fixed = layout.rect_of("fixed").unwrap();
    let grown = layout.rect_of("g").unwrap();
    assert_eq!(fixed, Rect::new(0.0, 0.0, 50.0, 20.0));
    assert_eq!(grown.y, 20.0 + SPACING);
    assert_eq!(grown.h, 100.0 - 20.0 - SPACING, "grow забирает остаток");
}

#[test]
fn flex_grow_shares_row_width() {
    let mut fixed = sized_column("fixed", Size::Px(50.0), Size::Px(20.0));
    fixed.style.flex_shrink = 0.0;
    let mut grow = column("g", vec![]);
    grow.style = Style {
        flex_grow: 2.0,
        ..Style::default()
    };
    let root = VElement::container("", NodeKind::Row, vec![fixed, grow]);
    let layout = layout_at(&root, 200.0, 100.0);
    assert_eq!(layout.rect_of("fixed").unwrap().w, 50.0);
    assert_eq!(
        layout.rect_of("g").unwrap(),
        Rect::new(50.0 + SPACING, 0.0, 200.0 - 50.0 - SPACING, 100.0)
    );
}

#[test]
fn flex_shrink_redistributes_overflow_by_weight() {
    let mut first = sized_column("a", Size::Px(80.0), Size::Px(20.0));
    first.style.flex_shrink = 1.0;
    let mut second = sized_column("b", Size::Px(80.0), Size::Px(20.0));
    second.style.flex_shrink = 2.0;
    let root = VElement::container("", NodeKind::Row, vec![first, second]);
    let layout = layout_at(&root, 100.0, 100.0);
    // Переполнение 66: первый отдаёт 22, второй — 44.
    assert_eq!(layout.rect_of("a").unwrap().w, 58.0);
    assert_eq!(layout.rect_of("b").unwrap().w, 36.0);
    assert_eq!(layout.rect_of("b").unwrap().x, 58.0 + SPACING);
}

#[test]
fn justify_content_centers_bare_text_in_h3() {
    let mut h3 = column("h3", vec![text("t", "Привет")]);
    h3.style = Style {
        width: Size::Percent(100.0),
        height: Size::Percent(100.0),
        justify_content: Align::Center,
        align_items: Align::Center,
        ..Style::default()
    };
    let root = VElement::container("", NodeKind::Row, vec![h3]);
    let layout = layout_at(&root, 200.0, 60.0);
    let h3_rect = layout.rect_of("h3").unwrap();
    let text_rect = layout.rect_of("t").unwrap();
    assert!(
        (h3_rect.y + h3_rect.h / 2.0 - text_rect.y - text_rect.h / 2.0).abs() < 0.5,
        "текст по центру h3 по вертикали"
    );
    assert!(
        (h3_rect.x + h3_rect.w / 2.0 - text_rect.x - text_rect.w / 2.0).abs() < 0.5,
        "текст по центру h3 по горизонтали"
    );
}

#[cfg(feature = "image-support")]
fn image(key: &'static str, width: u32, height: u32) -> VElement {
    VElement::leaf(
        key,
        NodeKind::Image {
            src: key,
            data: b"",
            width,
            height,
            alt: String::new(),
        },
    )
}

#[cfg(feature = "image-support")]
#[test]
fn image_uses_intrinsic_size_when_auto() {
    let root = column("", vec![image("img", 4, 2)]);
    let layout = build(&root);
    assert_eq!(
        layout.rect_of("img").unwrap(),
        Rect::new(0.0, 0.0, 4.0, 2.0)
    );
}

#[cfg(feature = "image-support")]
#[test]
fn image_preserves_aspect_ratio_for_width_and_height() {
    let mut by_width = image("w", 4, 2);
    by_width.style = Style {
        width: Size::Px(100.0),
        ..Style::default()
    };
    let layout = build(&column("", vec![by_width]));
    assert_eq!(
        layout.rect_of("w").unwrap(),
        Rect::new(0.0, 0.0, 100.0, 50.0)
    );

    let mut by_height = image("h", 4, 2);
    by_height.style = Style {
        height: Size::Px(100.0),
        ..Style::default()
    };
    let layout = build(&column("", vec![by_height]));
    assert_eq!(
        layout.rect_of("h").unwrap(),
        Rect::new(0.0, 0.0, 200.0, 100.0)
    );
}

#[cfg(feature = "image-support")]
#[test]
fn image_explicit_size_and_padding() {
    let mut both = image("both", 4, 2);
    both.style = Style {
        width: Size::Px(30.0),
        height: Size::Px(40.0),
        padding: Edges::all(5.0),
        ..Style::default()
    };
    let layout = build(&column("", vec![both]));
    assert_eq!(
        layout.rect_of("both").unwrap(),
        Rect::new(0.0, 0.0, 30.0, 40.0)
    );

    let mut padded = image("pad", 4, 2);
    padded.style = Style {
        width: Size::Px(100.0),
        padding: Edges::all(5.0),
        ..Style::default()
    };
    let layout = build(&column("", vec![padded]));
    assert_eq!(layout.rect_of("pad").unwrap().w, 100.0);
    assert_eq!(layout.rect_of("pad").unwrap().h, 55.0);
}
