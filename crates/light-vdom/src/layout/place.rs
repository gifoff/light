use super::base::*;
#[cfg(debug_assertions)]
use super::regions::debug_trace;
use super::regions::flex_weight;
use crate::vdom::{Align, NodeKind, Position, Size, VElement, WhiteSpace};

impl super::base::Layout {
    /// Помещает узел в поток в точке `(origin.x, origin.y)` и раскладывает его
    /// детей.
    ///
    /// `flow` — контекст потока родителя: контентная область (база процентов),
    /// известность размеров и ось `cross`-растяжения. `margin` даёт внешний
    /// отступ узла, `padding`/`border-size` — внутренний.
    ///
    /// `parent` — индекс родительского узла в плоском списке (`None` для корня).
    /// `inherited` — унаследованный размер и цвет шрифта.
    ///
    /// Контейнер с `Auto` размером по `cross`-оси родителя **растягивается** на
    /// доступную ширину/высоту (как block/`stretch` во flexbox); листья и кнопки
    /// — по содержимому. Дети позиционируются относительно **итогового** content
    /// box узла (см. [`Self::align_children`]), а не относительно окна.
    /// Возвращает внешний размер узла (вместе с `margin`) — шаг для соседей.
    pub(crate) fn place(
        &mut self,
        vnode: &VElement,
        origin: Point,
        flow: FlowContext,
        parent: Option<usize>,
        metrics: &dyn TextMetrics,
        inherited: Inherited,
    ) -> (f32, f32) {
        let style = vnode.style;
        if !style.visible {
            return (0.0, 0.0);
        }
        let inset = style.inset();
        let margin = style.margin;
        let flow_x = origin.x + margin.left;
        let flow_y = origin.y + margin.top;
        let font_size = style.font_size.unwrap_or(inherited.size);
        let font_color = style.font_color.or(inherited.color);
        let white_space = style.white_space.unwrap_or(inherited.white_space);
        // Проценты размера отсчитываются от контентной области родителя.
        let avail_w = flow.content.width;
        let avail_h = flow.content.height;
        // Полный шаг узла по осям (включая внешние отступы) — для родителей.
        let step_w = margin.left + margin.right;
        let step_h = margin.top + margin.bottom;
        // Поддерево узла занимает в плоском списке диапазон `[start, len)`.
        let start = self.nodes.len();
        // Корень и окно занимают всю доступную область. Контейнер с `Auto`
        // размером по `cross`-оси родителя растягивается на доступный размер;
        // иначе `Auto` даёт базу `0` (как неопределённая высота в CSS).
        let root = parent.is_none() || matches!(vnode.kind, NodeKind::Window);
        // Кастомный элемент (Plugin API) кладётся как контейнер: плагин
        // вернул произвольное поддерево, и layout разворачивает его по обычным
        // правилам потока (вертикально, как Column).
        let is_container = matches!(
            vnode.kind,
            NodeKind::Column
                | NodeKind::Row
                | NodeKind::TitleBar
                | NodeKind::Window
                | NodeKind::Custom(_)
        );
        let stretch_w = is_container && flow.stretch == Some(Axis::Width);
        let stretch_h = is_container && flow.stretch == Some(Axis::Height);
        let definite_w = flow.forced_w.unwrap_or(if root {
            avail_w
        } else if matches!(style.width, Size::Auto) {
            if stretch_w { avail_w } else { 0.0 }
        } else {
            style.resolve_width(0.0, avail_w)
        });
        let definite_h = flow.forced_h.unwrap_or(if root {
            avail_h
        } else if matches!(style.height, Size::Auto) {
            if stretch_h { avail_h } else { 0.0 }
        } else {
            style.resolve_height(0.0, avail_h)
        });
        // Определён ли размер узла по оси (для процентов его детей).
        let width_known = flow.forced_w.is_some()
            || root
            || matches!(style.width, Size::Px(_))
            || (matches!(style.width, Size::Percent(_)) && flow.width_known)
            || (matches!(style.width, Size::Auto) && stretch_w);
        let height_known = flow.forced_h.is_some()
            || root
            || matches!(style.height, Size::Px(_))
            || (matches!(style.height, Size::Percent(_)) && flow.height_known)
            || (matches!(style.height, Size::Auto) && stretch_h);
        // Контентная область, которую узел отдаёт детям под размеры-проценты.
        let child_content = ContentBox {
            left: flow_x + inset.left,
            top: flow_y + inset.top,
            width: (definite_w - inset.left - inset.right).max(0.0),
            height: (definite_h - inset.top - inset.bottom).max(0.0),
        };
        // Ось, по которой узел растягивает своих детей (если размер известен).
        // При `align-items != start` дети не растягиваются — они авторазмеряются,
        // и выравнивание (Center/End) вычисляется в `align_container_content`.
        let child_stretch = if style.align_items == Align::Start {
            match vnode.kind {
                NodeKind::Row | NodeKind::TitleBar if height_known => Some(Axis::Height),
                NodeKind::Column | NodeKind::Window if width_known => Some(Axis::Width),
                _ => None,
            }
        } else {
            None
        };
        let child_flow = FlowContext {
            content: child_content,
            width_known,
            height_known,
            stretch: child_stretch,
            forced_w: None,
            forced_h: None,
        };

        let (w, h) = match &vnode.kind {
            NodeKind::Text { content } => {
                let explicit_w = flow.forced_w.or_else(|| {
                    (!matches!(style.width, Size::Auto)).then(|| style.resolve_width(0.0, avail_w))
                });
                let limit = if let Some(width) = explicit_w {
                    width
                } else if let Some(max) = style.max_width {
                    max.resolve(0.0, avail_w)
                } else if flow.width_known {
                    avail_w
                } else {
                    f32::INFINITY
                };
                let limit = if let Some(max) = style.max_width {
                    limit.min(max.resolve(0.0, avail_w))
                } else {
                    limit
                };
                let limit = (limit - inset.horizontal()).max(1.0);
                let (content_w, content_h) = match white_space {
                    WhiteSpace::Normal => {
                        let lines =
                            crate::text_wrap::wrap_words_with_widths(content, limit, |line| {
                                metrics.measure(line, font_size).0
                            });
                        let width = lines.iter().map(|(_, width)| *width).fold(0.0, f32::max);
                        let row_h = metrics.measure("", font_size).1.max(1.0);
                        (width, lines.len() as f32 * row_h)
                    }
                    WhiteSpace::Nowrap => metrics.measure(content, font_size),
                };
                let w = flow.forced_w.unwrap_or_else(|| {
                    style.resolve_width(content_w + inset.horizontal(), avail_w)
                });
                let h = flow
                    .forced_h
                    .unwrap_or_else(|| style.resolve_height(content_h + inset.vertical(), avail_h));
                self.push(vnode, Rect::new(flow_x, flow_y, w, h), parent);
                (w, h)
            }
            #[cfg(feature = "image-support")]
            NodeKind::Image { width, height, .. } => {
                let intrinsic_w = *width as f32;
                let intrinsic_h = *height as f32;
                let explicit_w = flow.forced_w.or_else(|| {
                    (!matches!(style.width, Size::Auto))
                        .then(|| style.resolve_width(intrinsic_w, avail_w))
                });
                let explicit_h = flow.forced_h.or_else(|| {
                    (!matches!(style.height, Size::Auto))
                        .then(|| style.resolve_height(intrinsic_h, avail_h))
                });
                let (w, h) = match (explicit_w, explicit_h) {
                    (Some(w), Some(h)) => (w, h),
                    (Some(w), None) => {
                        let inner_w = (w - inset.horizontal()).max(0.0);
                        let inner_h = if intrinsic_w > 0.0 {
                            inner_w * intrinsic_h / intrinsic_w
                        } else {
                            0.0
                        };
                        (w, inner_h + inset.vertical())
                    }
                    (None, Some(h)) => {
                        let inner_h = (h - inset.vertical()).max(0.0);
                        let inner_w = if intrinsic_h > 0.0 {
                            inner_h * intrinsic_w / intrinsic_h
                        } else {
                            0.0
                        };
                        (inner_w + inset.horizontal(), h)
                    }
                    (None, None) => (
                        intrinsic_w + inset.horizontal(),
                        intrinsic_h + inset.vertical(),
                    ),
                };
                self.push(vnode, Rect::new(flow_x, flow_y, w, h), parent);
                (w, h)
            }
            #[cfg(feature = "image-support")]
            NodeKind::RuntimeImage { image, .. } => {
                let intrinsic_w = image.width() as f32;
                let intrinsic_h = image.height() as f32;
                let explicit_w = flow.forced_w.or_else(|| {
                    (!matches!(style.width, Size::Auto))
                        .then(|| style.resolve_width(intrinsic_w, avail_w))
                });
                let explicit_h = flow.forced_h.or_else(|| {
                    (!matches!(style.height, Size::Auto))
                        .then(|| style.resolve_height(intrinsic_h, avail_h))
                });
                let (w, h) = match (explicit_w, explicit_h) {
                    (Some(w), Some(h)) => (w, h),
                    (Some(w), None) => {
                        let inner_w = (w - inset.horizontal()).max(0.0);
                        let inner_h = if intrinsic_w > 0.0 {
                            inner_w * intrinsic_h / intrinsic_w
                        } else {
                            0.0
                        };
                        (w, inner_h + inset.vertical())
                    }
                    (None, Some(h)) => {
                        let inner_h = (h - inset.vertical()).max(0.0);
                        let inner_w = if intrinsic_h > 0.0 {
                            inner_h * intrinsic_w / intrinsic_h
                        } else {
                            0.0
                        };
                        (inner_w + inset.horizontal(), h)
                    }
                    (None, None) => (
                        intrinsic_w + inset.horizontal(),
                        intrinsic_h + inset.vertical(),
                    ),
                };
                self.push(vnode, Rect::new(flow_x, flow_y, w, h), parent);
                (w, h)
            }
            NodeKind::Button { .. } => {
                let index = self.push(vnode, Rect::default(), parent);
                // Внутренняя область кнопки: собственная рамка и `padding`.
                // Встроенные отступы кнопки (14/9) — значения по умолчанию,
                // которые переопределяет заданный пользователем `padding`.
                let extra = button_extra_padding(vnode, &style);
                let btn_inset = inset.plus(extra);
                let inner_left = flow_x + btn_inset.left;
                let inner_top = flow_y + btn_inset.top;
                // Auto-ширина кнопки: дети всегда измеряются с неограниченной
                // шириной (max-content), даже во второй проходе раскладки
                // (`stretch_row_children`), где кнопка получает `forced_w` из
                // первого прохода. Иначе текст повторно переносится на узкой
                // ширине и кнопка становится tall-and-narrow.
                let inner_w = if matches!(style.width, Size::Auto) {
                    f32::INFINITY
                } else {
                    (definite_w - btn_inset.horizontal()).max(0.0)
                };
                let inner_h = (definite_h - btn_inset.vertical()).max(0.0);
                let content = ContentBox {
                    left: inner_left,
                    top: inner_top,
                    width: inner_w,
                    height: inner_h,
                };
                // Содержимое выкладывается горизонтально (как в `Row`).
                let mut cursor_x = inner_left;
                let mut content_w = 0.0_f32;
                let mut content_h = 0.0_f32;
                let mut any_child = false;
                for child in &vnode.children {
                    if !child.style.visible || child.style.position == Position::Absolute {
                        continue;
                    }
                    let (child_w, child_h) = self.place(
                        child,
                        Point::new(cursor_x, inner_top),
                        FlowContext {
                            content,
                            width_known: inner_w > 0.0,
                            height_known: inner_h > 0.0,
                            stretch: None,
                            forced_w: None,
                            forced_h: None,
                        },
                        Some(index),
                        metrics,
                        Inherited {
                            size: font_size,
                            color: font_color,
                            white_space,
                        },
                    );
                    cursor_x += child_w + SPACING;
                    content_h = content_h.max(child_h);
                    any_child = true;
                }
                if any_child {
                    content_w = cursor_x - inner_left - SPACING;
                }
                // Auto-ширина Button — всегда по содержимому (max-content),
                // независимо от forced_w (stretch_row_children не должен
                // схлопывать кнопку). Явная ширина или проценты — forced_w
                // переопределяет.
                let w = match (style.width, flow.forced_w) {
                    (Size::Auto, _) => content_w + btn_inset.horizontal(),
                    (_, Some(fw)) => fw,
                    (_, None) => style.resolve_width(content_w + btn_inset.horizontal(), avail_w),
                };
                let h = flow.forced_h.unwrap_or_else(|| {
                    style.resolve_height(
                        (content_h + btn_inset.vertical()).max(BUTTON_MIN_H),
                        avail_h,
                    )
                });
                let node = &mut self.nodes[index];
                node.rect = Rect::new(flow_x, flow_y, w, h);
                #[cfg(debug_assertions)]
                debug_trace(vnode, node.rect, child_flow);
                // Содержимое кнопки центрируется по обеим осям (группа целиком).
                if any_child {
                    let pad_box = ContentBox {
                        left: node.rect.x + btn_inset.left,
                        top: node.rect.y + btn_inset.top,
                        width: (node.rect.w - btn_inset.horizontal()).max(0.0),
                        height: (node.rect.h - btn_inset.vertical()).max(0.0),
                    };
                    let first = self.nodes[index].children[0];
                    let dx = pad_box.left + ((pad_box.width - content_w) / 2.0).max(0.0)
                        - self.nodes[first].rect.x;
                    let dy = pad_box.top + ((pad_box.height - content_h) / 2.0).max(0.0)
                        - self.nodes[first].rect.y;
                    if dx != 0.0 || dy != 0.0 {
                        for child in &mut self.nodes[index + 1..] {
                            child.rect.x += dx;
                            child.rect.y += dy;
                        }
                    }
                }
                (w, h)
            }
            NodeKind::TextField { multiline, .. } => {
                let default_h = if *multiline {
                    FIELD_MIN_H + 2.0 * FIELD_LINE_H
                } else {
                    FIELD_MIN_H
                };
                let w = flow.forced_w.unwrap_or_else(|| {
                    style.resolve_width(FIELD_WIDTH + inset.horizontal(), avail_w)
                });
                let h = flow
                    .forced_h
                    .unwrap_or_else(|| style.resolve_height(default_h + inset.vertical(), avail_h));
                self.push(vnode, Rect::new(flow_x, flow_y, w, h), parent);
                (w, h)
            }
            #[cfg(feature = "elements-checkbox")]
            NodeKind::Checkbox { label, .. } => {
                let (label_w, label_h) = metrics.measure(label, font_size);
                let content_w = CHECKBOX_BOX + CHECKBOX_GAP + label_w;
                let content_h = CHECKBOX_BOX.max(label_h);
                let w = flow.forced_w.unwrap_or_else(|| {
                    style.resolve_width(content_w + inset.horizontal(), avail_w)
                });
                let h = flow
                    .forced_h
                    .unwrap_or_else(|| style.resolve_height(content_h + inset.vertical(), avail_h));
                self.push(vnode, Rect::new(flow_x, flow_y, w, h), parent);
                (w, h)
            }
            #[cfg(feature = "elements-radio")]
            NodeKind::Radio { label, .. } => {
                let (label_w, label_h) = metrics.measure(label, font_size);
                let content_w = CHECKBOX_BOX + CHECKBOX_GAP + label_w;
                let content_h = CHECKBOX_BOX.max(label_h);
                let w = flow.forced_w.unwrap_or_else(|| {
                    style.resolve_width(content_w + inset.horizontal(), avail_w)
                });
                let h = flow
                    .forced_h
                    .unwrap_or_else(|| style.resolve_height(content_h + inset.vertical(), avail_h));
                self.push(vnode, Rect::new(flow_x, flow_y, w, h), parent);
                (w, h)
            }
            #[cfg(feature = "elements-select")]
            NodeKind::Select { options, .. } => {
                let widest = options
                    .iter()
                    .map(|option| metrics.measure(&option.label, font_size).0)
                    .fold(0.0_f32, f32::max);
                let content_w = SELECT_WIDTH.max(widest + 2.0 * SELECT_PAD_X + CHECKBOX_BOX);
                let w = flow.forced_w.unwrap_or_else(|| {
                    style.resolve_width(content_w + inset.horizontal(), avail_w)
                });
                let h = flow.forced_h.unwrap_or_else(|| {
                    style.resolve_height(FIELD_MIN_H + inset.vertical(), avail_h)
                });
                self.push(vnode, Rect::new(flow_x, flow_y, w, h), parent);
                (w, h)
            }
            // Custom — атомарный контейнер: его поддерево (выданное плагином)
            // раскладывается по обычным правилам потока, как Column.
            NodeKind::Column
            | NodeKind::Row
            | NodeKind::TitleBar
            | NodeKind::Window
            | NodeKind::Custom(_) => {
                let index = self.push(vnode, Rect::default(), parent);
                // Кастомный элемент течёт вертикально (как Column): горизонтально
                // остаются только Row и TitleBar.
                let horizontal = matches!(vnode.kind, NodeKind::Row | NodeKind::TitleBar);
                // Заголовок `h1`–`h5` (`Column` с тегом-меткой) или текстовый
                // контейнер (колонка, чьё содержимое — только атомарные узлы:
                // текст/поле/изображение, например `<div>{{ count }}</div>` из
                // runtime-шаблона) при `align-items != start` родителя получают
                // нулевую базу ширины, и их внутренний текст (`width=100%`)
                // обнуляется. Как у кнопки: авторазмер по max-content — детей
                // раскладываем в безлимитной ширине и считаем её по содержимому.
                //
                // `!stretch_w` — когда родитель растягивает (`align-items=start`),
                // авто-ширина уже получает полную ширину (`definite_w = avail_w`),
                // и max-content не нужен: обычный путь корректен.
                let is_head = !root
                    && !stretch_w
                    && matches!(style.width, Size::Auto)
                    && (is_heading(vnode) || is_text_container(vnode));
                // Контентная область, которую узел отдаёт детям под проценты:
                // собственный определённый размер минус внутренние отступы.
                // Для заголовка с авто-шириной — ширина не ограничена, чтобы внутренний
                // `width=100%` текст измерялся при intrinsic-размере (не `Infinity`).
                let measure_content = if is_head {
                    ContentBox {
                        left: flow_x + inset.left,
                        top: flow_y + inset.top,
                        width: f32::INFINITY,
                        height: f32::INFINITY,
                    }
                } else {
                    child_content
                };
                let mut cursor_x = measure_content.left;
                let mut cursor_y = measure_content.top;
                let mut content_w = 0.0_f32;
                let mut content_h = 0.0_f32;
                let mut child_sizes = Vec::with_capacity(vnode.children.len());
                let mut any_flow = false;
                for child in &vnode.children {
                    if !child.style.visible || child.style.position == Position::Absolute {
                        continue;
                    }
                    any_flow = true;
                    // Для заголовка: копируем ребёнка и сбрасываем `width` в `Auto`,
                    // чтобы внутренний текст (`width=100%`) измерялся при своей
                    // intrinsic-ширине, а не растягивался до `Infinity`. Копия
                    // временная: в дереве остаётся исходный `Percent(100%)`, который
                    // мы растянем на итоговую ширину в блоке ниже.
                    let (child_w, child_h) = if is_head {
                        let mut copy = child.clone();
                        copy.style.width = Size::Auto;
                        let ctx = FlowContext {
                            content: measure_content,
                            width_known: false,
                            height_known: true,
                            stretch: None,
                            forced_w: None,
                            forced_h: None,
                        };
                        self.place(
                            &copy,
                            Point::new(cursor_x, cursor_y),
                            ctx,
                            Some(index),
                            metrics,
                            Inherited {
                                size: font_size,
                                color: font_color,
                                white_space,
                            },
                        )
                    } else {
                        self.place(
                            child,
                            Point::new(cursor_x, cursor_y),
                            child_flow,
                            Some(index),
                            metrics,
                            Inherited {
                                size: font_size,
                                color: font_color,
                                white_space,
                            },
                        )
                    };
                    child_sizes.push((child_w, child_h));
                    if horizontal {
                        cursor_x += child_w + SPACING;
                        content_h = content_h.max(child_h);
                    } else {
                        cursor_y += child_h + SPACING;
                        content_w = content_w.max(child_w);
                    }
                }
                if any_flow {
                    if horizontal {
                        content_w = cursor_x - child_content.left - SPACING;
                    } else {
                        content_h = cursor_y - child_content.top - SPACING;
                    }
                }
                // Второй проход: распределяем свободное место (`flex-grow`) или
                // переполнение (`flex-shrink`) между прямыми детьми по main-оси.
                let main_avail = if horizontal {
                    child_content.width
                } else {
                    child_content.height
                };
                let used_main = if horizontal { content_w } else { content_h };
                let main_known = if horizontal {
                    width_known
                } else {
                    height_known
                };
                let scrolls_main = if horizontal {
                    style.overflow_x.scrollable()
                } else {
                    style.overflow_y.scrollable()
                };
                let raw_free = main_avail - used_main;
                // В контейнере с размером по содержимому нет переполнения,
                // а содержимое scroll-контейнера не сжимаем — иначе пропадёт
                // прокрутка.
                let free = if main_known && (raw_free >= 0.0 || !scrolls_main) {
                    raw_free
                } else {
                    0.0
                };
                let total_flex: f32 = vnode
                    .children
                    .iter()
                    .filter(|child| child.style.visible && child.style.position == Position::Flow)
                    .map(|child| flex_weight(child, free > 0.0))
                    .sum();
                if free != 0.0 && total_flex > 0.0 {
                    let forced: Vec<(Option<f32>, Option<f32>)> = vnode
                        .children
                        .iter()
                        .filter(|child| {
                            child.style.visible && child.style.position == Position::Flow
                        })
                        .zip(child_sizes.iter())
                        .map(|(child, &(child_w, child_h))| {
                            let weight = flex_weight(child, free > 0.0);
                            let extra = free * weight / total_flex;
                            if horizontal {
                                let step = child.style.margin.left + child.style.margin.right;
                                (Some((child_w - step + extra).max(0.0)), None)
                            } else {
                                let step = child.style.margin.top + child.style.margin.bottom;
                                (None, Some((child_h - step + extra).max(0.0)))
                            }
                        })
                        .collect();
                    // Пересобираем поддерево с принудительными main-размерами.
                    self.nodes.truncate(index + 1);
                    self.nodes[index].children.clear();
                    cursor_x = child_content.left;
                    cursor_y = child_content.top;
                    content_w = 0.0;
                    content_h = 0.0;
                    let mut visible = vnode
                        .children
                        .iter()
                        .filter(|child| {
                            child.style.visible && child.style.position == Position::Flow
                        })
                        .zip(forced.iter());
                    for (child, &(forced_w, forced_h)) in &mut visible {
                        let (child_w, child_h) = self.place(
                            child,
                            Point::new(cursor_x, cursor_y),
                            FlowContext {
                                forced_w,
                                forced_h,
                                ..child_flow
                            },
                            Some(index),
                            metrics,
                            Inherited {
                                size: font_size,
                                color: font_color,
                                white_space,
                            },
                        );
                        if horizontal {
                            cursor_x += child_w + SPACING;
                            content_h = content_h.max(child_h);
                        } else {
                            cursor_y += child_h + SPACING;
                            content_w = content_w.max(child_w);
                        }
                    }
                    if any_flow {
                        if horizontal {
                            content_w = cursor_x - child_content.left - SPACING;
                        } else {
                            content_h = cursor_y - child_content.top - SPACING;
                        }
                    }
                }
                // Заголовок `h1`–`h5` с авто-шириной: ширина по max-content
                // (аналог кнопки) — `content_w` уже посчитан по intrinsic-размерам
                // детей, выложенных в безлимитной ширине выше. Явная ширина или
                // процент — обычный путь; у заголовка они совпадают (100% → тот же
                // max-content, потому что дети измерялись в Infinity).
                let w = if is_head {
                    if let Some(fw) = flow.forced_w {
                        fw
                    } else {
                        content_w + inset.horizontal()
                    }
                } else {
                    flow.forced_w.unwrap_or(if root {
                        avail_w
                    } else if stretch_w {
                        style.resolve_width(avail_w, avail_w)
                    } else {
                        style.resolve_width(content_w + inset.horizontal(), avail_w)
                    })
                };
                let h = flow.forced_h.unwrap_or(if root {
                    avail_h
                } else if stretch_h {
                    style.resolve_height(avail_h, avail_h)
                } else {
                    style.resolve_height(content_h + inset.vertical(), avail_h)
                });
                self.nodes[index].rect = Rect::new(flow_x, flow_y, w, h);
                let content = ContentBox::from_rect(self.nodes[index].rect, inset);
                if is_head {
                    // Заголовок / текстовый контейнер с авто-шириной: дети
                    // выкладывались в безлимитной content box на нулевой базе;
                    // пересаживаем их целиком в итоговый (расширенный) content
                    // box: позиция — по content box, ширина `width=100%` текста
                    // растягивается на его `w`. Аналог центрирования кнопки.
                    let dx = content.left - measure_content.left;
                    let dy = content.top - measure_content.top;
                    for ci in self.nodes[index].children.clone() {
                        let lnode = &mut self.nodes[ci];
                        lnode.rect.x += dx;
                        lnode.rect.y += dy;
                        // Прямой текст заголовка с `width=100%` заполняет ширину
                        // content box; остальные размеры не трогаем.
                        if matches!(lnode.kind, NodeKind::Text { .. })
                            && matches!(lnode.style.width, Size::Percent(_))
                        {
                            lnode.rect.w = (content.width).max(0.0);
                        }
                    }
                }
                // В горизонтальном контейнере с высотой по содержимому дети с
                // `height:auto` растягиваются на итоговую высоту после того,
                // как она стала известна (аналог `align-items: stretch`).
                if horizontal && !height_known && style.align_items == Align::Start {
                    (content_w, content_h) = self.stretch_row_children(
                        vnode,
                        index,
                        content,
                        metrics,
                        Inherited {
                            size: font_size,
                            color: font_color,
                            white_space,
                        },
                    );
                }
                // Абсолютные дети не участвуют в потоке и раскладываются после
                // того, как известен итоговый content box родителя.
                for child in &vnode.children {
                    if !child.style.visible || child.style.position != Position::Absolute {
                        continue;
                    }
                    self.place(
                        child,
                        Point::new(content.left, content.top),
                        FlowContext {
                            content,
                            width_known: true,
                            height_known: true,
                            stretch: None,
                            forced_w: None,
                            forced_h: None,
                        },
                        Some(index),
                        metrics,
                        Inherited {
                            size: font_size,
                            color: font_color,
                            white_space,
                        },
                    );
                }
                self.nodes[index].scroll_width = (content_w + inset.horizontal()).max(w);
                self.nodes[index].scroll_height = (content_h + inset.vertical()).max(h);
                let used_main = if horizontal { content_w } else { content_h };
                let cross_attr = if horizontal {
                    "position-y"
                } else {
                    "position-x"
                };
                let explicit_cross: Vec<bool> = vnode
                    .children
                    .iter()
                    .filter(|child| child.style.visible && child.style.position == Position::Flow)
                    .map(|child| child.attr(cross_attr).is_some())
                    .collect();
                self.align_container_content(
                    index,
                    content,
                    horizontal,
                    used_main,
                    &explicit_cross,
                );
                self.align_children(index, content);
                (w, h)
            }
        };

        if !is_container {
            self.nodes[start].scroll_width = w;
            self.nodes[start].scroll_height = h;
        }
        // Записываем разрешённый размер/цвет шрифта, чтобы отрисовщик и
        // наследование стилей (голый текст в контейнерах) работали корректно.
        self.nodes[start].style.font_size = Some(font_size);
        self.nodes[start].style.white_space = Some(white_space);
        if let Some(color) = font_color
            && self.nodes[start].style.font_color.is_none()
        {
            self.nodes[start].style.font_color = Some(color);
        }
        (w + step_w, h + step_h)
    }
}
