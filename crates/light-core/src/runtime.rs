//! Шаблоны во время выполнения (фича `runtime-templates`).
//!
//! В dev-режиме `render()` сгенерированного макросом [`crate::component::Component`]
//! сначала пытается прочитать HTML-шаблон с диска ([`render_dynamic`]) и разобрать
//! его в живое виртуальное дерево ([`parse_template`]). Если файл изменился
//! (mtime), он перечитывается заново, поэтому правка разметки видна без
//! пересборки. Любой неподдерживаемый на этапе выполнения конструкт или ошибка
//! чтения возвращает `None`, и статический код макроса берёт управление на себя —
//! приложение продолжает работать со старой, встроенной в бинарник разметкой.
//!
//! Поддерживается подмножество синтаксиса шаблонов: контейнеры (`div`, `span`,
//! `h1`–`h5`, `titlebar`), кнопки, текстовые поля, а также базовые атрибуты
//! раскладки, событие `@click="обработчик"` и интерполяция `{{ поле }}`.
//! Директивы `v-if`/`v-for`/`:attr` и сложные выражения не поддерживаются во
//! время выполнения и выключают runtime-рендер целиком.

#[cfg(feature = "runtime-templates")]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(feature = "runtime-templates")]
use std::time::SystemTime;
use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex, MutexGuard},
    time::Duration,
};

use crate::{
    component::{Component, WindowConfig},
    vdom::{
        Align, ButtonType, Edges, NodeKind, Offset, Overflow, Position, RequiredIndicator, Resize,
        SelectOption, Size, Style, VElement, intern_key, parse_align_items, parse_bool,
        parse_color, parse_justify, parse_offset, parse_overflow, parse_position,
        parse_required_indicator, parse_resize, parse_size_or_auto, parse_white_space,
    },
};

/// Кэш HTML-шаблонов: путь к файлу → (mtime, текст шаблона).
///
/// Хранит **HTML-строку**, а не [`VElement`]: при каждом вызове
/// [`render_dynamic`] HTML перечитывается из кэша и повторно разбирается через
/// [`parse_template`], так что `{{ поле }}` всегда вычисляется с актуальным
/// state. Мtime сверяется с сохранённым: совпадение — HTML берётся из кэша;
/// файл изменился — он перечитывается с диска. `path` хранится как
/// `&'static str` (макрос генерирует константу через `concat!`/`include_str!`).
#[cfg_attr(not(feature = "runtime-templates"), allow(dead_code))]
static CACHE: LazyLock<Mutex<HashMap<&'static str, (Duration, String)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[cfg_attr(not(feature = "runtime-templates"), allow(dead_code))]
fn cache() -> MutexGuard<'static, HashMap<&'static str, (Duration, String)>> {
    // Отравленное мутекс не является ошибкой, требующим возврата: мы просто
    // теряем кэш (разбор происходит заново) — допустимая деградация.
    CACHE.lock().unwrap_or_else(|guard| guard.into_inner())
}

/// Отмечен ли уже факт отката к статическому дереву (однократная диагностика).
#[cfg(feature = "runtime-templates")]
static FALLBACK_LOGGED: AtomicBool = AtomicBool::new(false);

/// Записывает однократный лог о том, что runtime-шаблон не разобрался и
/// используется статическое дерево из бинарника.
#[cfg(feature = "runtime-templates")]
fn log_fallback_once() {
    let _ = FALLBACK_LOGGED.compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed);
}

/// Логирует ошибку разбора конкретного атрибута элемента.
/// В debug-сборках выводит диагностику; в release — тихо возвращает None.
#[cfg(debug_assertions)]
fn parse_fail(element: &str, attr_name: &str, attr_value: &str) {
    eprintln!(
        "light: runtime parse failed at {:?} (attr {:?}={:?})",
        element, attr_name, attr_value
    );
}
#[cfg(not(debug_assertions))]
fn parse_fail(_element: &str, _attr_name: &str, _attr_value: &str) {}

/// Читает HTML-шаблон с диска и разбирает его в [`VElement`], используя mtime-кэш.
///
/// `path` — `'static` путь к файлу (генерируется макросом); `fallback` — текст
/// шаблона, встроенный в бинарник через `include_str!`, он разбирается, если файл
/// прочитать не удалось; `component` — инстанс, чьи поля читаются при разрешении
/// `{{ … }}`, а методы вызываются из `dispatch_by_name`.
///
/// Кэшируется **HTML-строка**, а не [`VElement`]: при каждом вызове шаблон
/// повторно разбирается через [`parse_template`], так что `{{ поле }}` всегда
/// вычисляется с актуальным state компонента.
///
/// Возвращает `None`, если шаблон содержит неподдерживаемую на этапе выполнения
/// конструкцию или не читается, и fallback тоже не разбирается; в этом случае
/// сгенерированный макросом статический код [`Component::render`] продолжает работу.
///
/// Без фичи `runtime-templates` сразу возвращает `None`.
pub fn render_dynamic<C: Component + ?Sized>(
    path: &'static str,
    fallback: &'static str,
    component: &C,
) -> Option<VElement> {
    #[cfg(feature = "runtime-templates")]
    {
        let html = match std::fs::metadata(path) {
            Ok(metadata) => {
                let mtime = metadata
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok());
                match mtime {
                    Some(mtime) => {
                        let mut cache = cache();
                        if let Some((cached_mtime, cached_html)) = cache.get(path)
                            && *cached_mtime == mtime
                        {
                            cached_html.clone()
                        } else {
                            let html = std::fs::read_to_string(path).ok()?;
                            cache.insert(path, (mtime, html.clone()));
                            html
                        }
                    }
                    None => return parse_template(fallback, component),
                }
            }
            Err(_) => return parse_template(fallback, component),
        };
        let result = parse_template(&html, component);
        if result.is_none() {
            log_fallback_once();
            let result = parse_template(fallback, component);
            if result.is_none() {
                log_fallback_once();
            }
            return result;
        }
        result
    }
    #[cfg(not(feature = "runtime-templates"))]
    {
        let _ = (path, fallback, component);
        None
    }
}

/// Разбирает текст HTML-шаблона в корневой [`VElement`].
///
/// Это строгое runtime-подмножество синтаксиса шаблонов. Поддерживается:
/// - корень `<window>` (одиночный) или произвольная последовательность узлов;
/// - контейнеры `div`/`span` (`direction="horizontal"` → ряд), `h1`–`h5`
///   (заголовочные контейнеры с базовым размером шрифта), `titlebar`;
/// - `button` (`@click`, `type`, `:disabled`), `text`/`password`
///   (`v-model`, `placeholder`, `textarea`, `resize`, `:disabled`);
/// - `checkbox`/`radio`/`select` — только при включённых соответствующих фичах
///   элементов, иначе `None`;
/// - интерполяция `{{ поле }}` (простой идентификатор, без точек), атрибуты
///   раскладки из [`Style`], статические атрибуты (без директив).
///
/// Неподдерживаемые конструкции возвращают `None` — без panics.
pub fn parse_template<C: Component + ?Sized>(html: &str, component: &C) -> Option<VElement> {
    let nodes = match light_fw_html::parse(html) {
        Ok(nodes) => nodes,
        Err(_) => {
            return None;
        }
    };
    match nodes.len() {
        0 => Some(VElement::container("", NodeKind::Column, Vec::new())),
        1 => {
            let is_window = matches!(
                &nodes[0],
                light_fw_html::Node::Element(el) if el.name == "window"
            );
            if is_window {
                let light_fw_html::Node::Element(window) = &nodes[0] else {
                    unreachable!();
                };
                build_element(
                    window,
                    ROOT_KEY,
                    Some(build_window_config(window)),
                    component,
                )
            } else {
                wrap_nodes(&nodes, component)
            }
        }
        _ => wrap_nodes(&nodes, component),
    }
}

/// Оборачивает плоскую последовательность узлов (без корневого `<window>`)
/// в колонку-корень.
fn wrap_nodes<C: Component + ?Sized>(
    nodes: &[light_fw_html::Node],
    component: &C,
) -> Option<VElement> {
    let column_kind = NodeKind::Column;
    let children = build_children(nodes, ROOT_KEY, &column_kind, component)?;
    Some(VElement::container(ROOT_KEY, NodeKind::Column, children))
}

// ---------------------------------------------------------------------------
// Построение дерева
// ---------------------------------------------------------------------------

const HEADING_SIZES: [(u8, f32); 5] = [(1, 32.0), (2, 24.0), (3, 20.0), (4, 18.0), (5, 16.0)];

/// Базовый размер шрифта для заголовков `h1`–`h5`.
fn heading_font(tag: &str) -> Option<f32> {
    let num = tag.strip_prefix('h')?.parse::<u8>().ok()?;
    HEADING_SIZES
        .iter()
        .find(|(n, _)| *n == num)
        .map(|(_, size)| *size)
}

enum ElementKind {
    Container(NodeKind),
    TextLeaf,
    Checkbox,
    Radio,
    Select,
}

/// Строит один элемент (контейнер или лист) вместе со стилем, тегом, атрибутами
/// и, если задан `window_config`, настройками окна.
fn build_element<C: Component + ?Sized>(
    element: &light_fw_html::Element,
    key: &str,
    window_config: Option<WindowConfig>,
    component: &C,
) -> Option<VElement> {
    for attr in &element.attrs {
        if is_unsupported_directive(&attr.name) {
            parse_fail(&element.name, &attr.name, &attr.value);
            return None;
        }
    }
    let (kind, default_font) = match element.name.as_str() {
        "div" | "span" => {
            let direction = attr_value(&element.attrs, "direction");
            let node_kind = match direction {
                // `vertical` и отсутствие атрибута — стандартная колонка (по
                // умолчанию); `horizontal` — ряд. Любое другое значение не
                // является ошибкой: родитель выравнивает его через `align-items`.
                None | Some("vertical") => NodeKind::Column,
                Some("horizontal") => NodeKind::Row,
                Some(bad) => {
                    parse_fail(&element.name, "direction", bad);
                    NodeKind::Column
                }
            };
            (ElementKind::Container(node_kind), None)
        }
        "h1" | "h2" | "h3" | "h4" | "h5" => {
            let font = heading_font(&element.name)?;
            (ElementKind::Container(NodeKind::Column), Some(font))
        }
        "titlebar" => (ElementKind::Container(NodeKind::TitleBar), None),
        "template" => (ElementKind::Container(NodeKind::Column), None),
        "window" => (ElementKind::Container(NodeKind::Window), None),
        "button" => {
            let disabled = resolve_disabled(&element.attrs, component)?;
            let button_type = resolve_button_type(&element.attrs, component)?;
            (
                ElementKind::Container(NodeKind::Button {
                    disabled,
                    button_type,
                }),
                None,
            )
        }
        "text" | "password" => (ElementKind::TextLeaf, None),
        "checkbox" => (ElementKind::Checkbox, None),
        "radio" => (ElementKind::Radio, None),
        "select" => (ElementKind::Select, None),
        _ => {
            parse_fail(&element.name, "tag", &element.name);
            return None;
        }
    };
    let leaf = match &kind {
        ElementKind::Container(node_kind) => {
            let children = build_children(&element.children, key, node_kind, component)?;
            VElement::container(intern_key(key), node_kind.clone(), children)
        }
        ElementKind::TextLeaf => build_text_field(element, key, component)?,
        ElementKind::Checkbox => build_checkbox(element, key, component)?,
        ElementKind::Radio => build_radio(element, key, component)?,
        ElementKind::Select => build_select(element, key, component)?,
    };
    let style = build_style(&element.name, &element.attrs, default_font, component)?;
    let (raw_tag, raw_attrs) = collect_meta(&element.attrs, &element.name);
    let tag = intern_key(&raw_tag);
    let attrs = raw_attrs
        .into_iter()
        .map(|(name, value)| (intern_key(&name), value))
        .collect();
    let mut elem = leaf.with_style(style).with_tag(tag).with_attrs(attrs);
    if let Some(cfg) = window_config {
        elem = elem.with_window(cfg);
    }
    // Заголовки `h1`–`h5`: текст внутри центрируется (`justify-content="center"`),
    // чтобы заголовок отображался по центру, когда родитель имеет
    // `align-items="center"`. Ширина остаётся auto (max-content) — layout.rs
    // сам решает stretch через `flow.stretch` (stretch только при
    // `align_items == Start`).
    let is_heading = matches!(element.name.as_str(), "h1" | "h2" | "h3" | "h4" | "h5");
    if is_heading && matches!(&kind, ElementKind::Container(NodeKind::Column)) {
        if let Some(child) = elem.children.first_mut() {
            child.style.justify_content = Align::Center;
        }
    }
    // Button клипит содержимое по своим границам (аналог overflow:hidden в CSS),
    // но ТОЛЬКО при явном width: без width кнопка Auto по max-content и overflow
    // ломает intrinsic-размер (схлопывается до min-content).
    if matches!(&kind, ElementKind::Container(NodeKind::Button { .. }))
        && attr_value(&element.attrs, "width").is_some()
    {
        let mut s = elem.style;
        s.overflow_x = Overflow::Hidden;
        s.overflow_y = Overflow::Hidden;
        elem = elem.with_style(s);
    }
    Some(elem)
}

/// Строит список дочерних узлов.
fn build_children<C: Component + ?Sized>(
    nodes: &[light_fw_html::Node],
    base: &str,
    parent_kind: &NodeKind,
    component: &C,
) -> Option<Vec<VElement>> {
    let mut out = Vec::with_capacity(nodes.len());
    let mut index = 0usize;
    for node in nodes {
        match node {
            light_fw_html::Node::Text(segments) => {
                let content = render_segments(segments, component)?;
                if content.is_empty() {
                    continue;
                }
                let key = child_key(base, index);
                let mut text = VElement::leaf(
                    intern_key(&key),
                    NodeKind::Text {
                        content: content.clone(),
                    },
                );
                // Растянуть Text по ширине родителя ТОЛЬКО в Column:
                // в Button/Row/TitleBar/Window — Auto (иначе circular layout).
                if matches!(parent_kind, NodeKind::Column) && matches!(text.style.width, Size::Auto)
                {
                    let mut s = text.style;
                    s.width = Size::Percent(100.0);
                    text = text.with_style(s);
                }
                out.push(text);
            }
            light_fw_html::Node::Element(element) => {
                let positional = child_key(base, index);
                // Ключ узла для диспетчеризации: явный `key` (литерал) имеет
                // приоритет; если его нет, берётся нормализованное имя
                // обработчика `@click` (без скобок) — так статический код макроса
                // может дотянуться до него через [`Component::invoke_handler`] и
                // [`Component::dispatch_by_name`] (встроенные имена `close`/`minimize`
                // и т. п. резолвятся в базовом хвосте трейта). Иначе — позиционный.
                let explicit_key = attr_value(&element.attrs, "key")
                    .map(str::trim)
                    .filter(|k| !k.is_empty() && !k.starts_with(':'));
                let click_name = attr_value(&element.attrs, "@click").map(normalize_click);
                let key = explicit_key
                    .or(click_name.as_deref().filter(|n| !n.is_empty()))
                    .unwrap_or(&positional);
                let child = build_element(element, key, None, component)?;
                out.push(child);
            }
        }
        index += 1;
    }
    Some(out)
}

/// Имя обработчика `@click` для диспетчеризации по имени.
///
/// Убирает скобки вызова (`"increment()"` → `"increment"`), пробельные
/// символы вокруг и ведущее `this.` — генерируемый макросом
/// [`Component::dispatch_by_name`] сопоставляет именно это имя.
fn normalize_click(handler: &str) -> String {
    let trimmed = handler.trim();
    let trimmed = trimmed.strip_prefix("this.").unwrap_or(trimmed);
    trimmed
        .strip_suffix("()")
        .map(str::trim)
        .unwrap_or(trimmed)
        .to_owned()
}

/// Текстовый узел: литеральные сегменты склеиваются, `{{ поле }}` разрешается
/// через [`Component::eval_field`]. Сложные выражения (с точками) дают `None`.
fn render_segments<C: Component + ?Sized>(
    segments: &[light_fw_html::Segment],
    component: &C,
) -> Option<String> {
    let mut out = String::new();
    for segment in segments {
        match segment {
            light_fw_html::Segment::Literal(text) => out.push_str(text),
            light_fw_html::Segment::Path(path) => {
                let value = match eval_simple(component, path) {
                    Some(v) => v,
                    None => {
                        return None;
                    }
                };
                out.push_str(&value);
            }
        }
    }
    Some(out)
}

/// Детерминированный ключ узла: `base#index`. Ключи стабильны между перерендерами
/// одного шаблона (порядок узлов не меняется), поэтому сопоставление событий и
/// отслеживание реактивности работают без изменений дерева.
fn child_key(base: &str, index: usize) -> String {
    format!("{base}.{index}")
}

/// Ключ корневого узла: `""` (корень) — совпадает с конвенцией виртуального дерева.
const ROOT_KEY: &str = "";

// ---------------------------------------------------------------------------
// Листы
// ---------------------------------------------------------------------------

/// `<text>` / `<password>`: значение из `v-model` (если простой идентификатор),
/// остальные свойства из атрибутов.
fn build_text_field<C: Component + ?Sized>(
    element: &light_fw_html::Element,
    key: &str,
    component: &C,
) -> Option<VElement> {
    let masked = element.name == "password";
    let model = attr_value(&element.attrs, "v-model");
    let value = match model {
        Some(m) => eval_simple(component, m)?,
        None => String::new(),
    };
    let placeholder = attr_value(&element.attrs, "placeholder")
        .unwrap_or("")
        .to_string();
    let disabled = resolve_disabled(&element.attrs, component)?;
    let multiline = attr_value(&element.attrs, "textarea").is_some();
    let resize = match attr_value(&element.attrs, "resize") {
        Some(r) => {
            let parsed = parse_resize_value(r);
            if parsed.is_none() {
                parse_fail(&element.name, "resize", r);
            }
            parsed?
        }
        None => Resize::default(),
    };
    Some(VElement::leaf(
        intern_key(key),
        NodeKind::TextField {
            value,
            placeholder,
            disabled,
            multiline,
            resize,
            masked,
        },
    ))
}

#[cfg(feature = "elements-checkbox")]
fn build_checkbox<C: Component + ?Sized>(
    element: &light_fw_html::Element,
    key: &str,
    component: &C,
) -> Option<VElement> {
    let label = text_content(&element.children)?;
    let model = attr_value(&element.attrs, "v-model")?;
    let checked = eval_bool(component, model)?;
    let disabled = resolve_disabled(&element.attrs, component)?;
    Some(VElement::leaf(
        intern_key(key),
        NodeKind::Checkbox {
            label,
            checked,
            disabled,
        },
    ))
}

#[cfg(not(feature = "elements-checkbox"))]
fn build_checkbox<C: Component + ?Sized>(
    _element: &light_fw_html::Element,
    _key: &str,
    _component: &C,
) -> Option<VElement> {
    None
}

#[cfg(feature = "elements-radio")]
fn build_radio<C: Component + ?Sized>(
    element: &light_fw_html::Element,
    key: &str,
    component: &C,
) -> Option<VElement> {
    let label = text_content(&element.children)?;
    let value = attr_value(&element.attrs, "value")
        .unwrap_or("")
        .to_string();
    let model = attr_value(&element.attrs, "v-model")?;
    let current = eval_simple(component, model)?;
    let selected = current == value;
    let disabled = resolve_disabled(&element.attrs, component)?;
    Some(VElement::leaf(
        intern_key(key),
        NodeKind::Radio {
            label,
            value,
            selected,
            disabled,
        },
    ))
}

#[cfg(not(feature = "elements-radio"))]
fn build_radio<C: Component + ?Sized>(
    _element: &light_fw_html::Element,
    _key: &str,
    _component: &C,
) -> Option<VElement> {
    None
}

#[cfg(feature = "elements-select")]
fn build_select<C: Component + ?Sized>(
    element: &light_fw_html::Element,
    key: &str,
    component: &C,
) -> Option<VElement> {
    let model = attr_value(&element.attrs, "v-model")?;
    let value = eval_simple(component, model)?;
    let options = collect_options(&element.children)?;
    let disabled = resolve_disabled(&element.attrs, component)?;
    Some(VElement::leaf(
        intern_key(key),
        NodeKind::Select {
            value,
            options,
            disabled,
        },
    ))
}

#[cfg(not(feature = "elements-select"))]
fn build_select<C: Component + ?Sized>(
    _element: &light_fw_html::Element,
    _key: &str,
    _component: &C,
) -> Option<VElement> {
    None
}

#[cfg(feature = "elements-select")]
fn collect_options(children: &[light_fw_html::Node]) -> Option<Vec<SelectOption>> {
    let mut options = Vec::new();
    for child in children {
        let light_fw_html::Node::Element(option) = child else {
            continue;
        };
        if option.name != "option" {
            continue;
        }
        let label = text_content(&option.children)?;
        let value = attr_value(&option.attrs, "value")
            .map(str::to_string)
            .unwrap_or(label.clone());
        options.push(SelectOption { label, value });
    }
    Some(options)
}

#[cfg(not(feature = "elements-select"))]
#[allow(dead_code)]
fn collect_options(_children: &[light_fw_html::Node]) -> Option<Vec<SelectOption>> {
    None
}

// ---------------------------------------------------------------------------
// Стили
// ---------------------------------------------------------------------------

/// Строит [`Style`] из атрибутов элемента. `default_font` задаёт размер шрифта
/// заголовкам `h1`–`h5`, если атрибут `font-size` не задан.
fn build_style<C: Component + ?Sized>(
    tag: &str,
    attrs: &[light_fw_html::Attr],
    default_font: Option<f32>,
    _component: &C,
) -> Option<Style> {
    let mut width = Size::Auto;
    let mut height = Size::Auto;
    let mut min_width = None;
    let mut min_height = None;
    let mut max_width = None;
    let mut max_height = None;
    let mut flex_grow = 0.0f32;
    let mut flex_shrink = 1.0f32;
    let mut position = Position::Flow;
    let mut overflow = Overflow::Visible;
    let mut overflow_x = None;
    let mut overflow_y = None;
    let mut white_space = None;
    let mut align_x = Align::Start;
    let mut align_y = Align::Start;
    let mut offset_x = Offset::Px(0.0);
    let mut offset_y = Offset::Px(0.0);
    let mut justify_content = Align::Start;
    let mut align_items = Align::Start;
    let mut padding = Edges::ZERO;
    let mut margin = Edges::ZERO;
    let mut background = None;
    let mut font_color = None;
    let mut border_color = None;
    let mut font_size = default_font;
    let mut border_size = 0.0f32;
    let mut visible = true;
    let mut required = false;
    let mut required_indicator = RequiredIndicator::Asterisk;
    let mut z = 0i32;

    for attr in attrs {
        let name = attr.name.as_str();
        let value = attr.value.as_str();
        // Логирует `parse_fail` при `None` и прокидывает значение дальше.
        // Четыре монотипных замыкания — по одному на каждый тип значения
        // (`f32`, `Size`, `u32`, `i32`), так как Rust не разрешает генерики
        // в замыкании без явной аннотации, а явная аннотация делает вызов
        // громоздким (см. замечание о generic-inference trap).
        let strict = |v: Option<f32>| -> Option<f32> {
            if v.is_none() {
                parse_fail(tag, name, value);
            }
            v
        };
        let strict_size = |n: &str, v: &str, s: Option<Size>| -> Option<Size> {
            if s.is_none() {
                parse_fail(tag, n, v);
            }
            s
        };
        let strict_color = |n: &str, v: &str, c: Option<u32>| -> Option<u32> {
            if c.is_none() {
                parse_fail(tag, n, v);
            }
            c
        };
        let strict_i32 = |n: &str, v: &str, i: Option<i32>| -> Option<i32> {
            if i.is_none() {
                parse_fail(tag, n, v);
            }
            i
        };
        match name {
            "width" => width = strict_size(name, value, parse_size(value))?,
            "height" => height = strict_size(name, value, parse_size(value))?,
            "min-width" => min_width = strict_size(name, value, parse_size(value)),
            "max-width" => max_width = strict_size(name, value, parse_size(value)),
            "min-height" => min_height = strict_size(name, value, parse_size(value)),
            "max-height" => max_height = strict_size(name, value, parse_size(value)),
            "flex-grow" => flex_grow = strict(parse_flex(value))?,
            "flex-shrink" => flex_shrink = strict(parse_flex(value))?,
            "flex" => {
                let parts: Vec<&str> = value.split_whitespace().collect();
                if !parts.is_empty() {
                    flex_grow = strict(parse_flex(parts[0]))?;
                }
                if parts.len() > 1 {
                    flex_shrink = strict(parse_flex(parts[1]))?;
                }
            }
            "position" => position = parse_position(value),
            "overflow" => overflow = parse_overflow(value),
            "overflow-x" => overflow_x = Some(parse_overflow(value)),
            "overflow-y" => overflow_y = Some(parse_overflow(value)),
            "white-space" => white_space = Some(parse_white_space(value)),
            // `left`/`center`/`right` (и `top`/`center`/`bottom`) принимаются
            // в любом регистре; любое другое значение не является ошибкой —
            // родитель выравнивает его через `align-items`.
            "position-x" => {
                let t = value.trim().to_ascii_lowercase();
                align_x = match t.as_str() {
                    "left" => Align::Start,
                    "center" => Align::Center,
                    "right" => Align::End,
                    _ => Align::Start,
                }
            }
            "position-y" => {
                let t = value.trim().to_ascii_lowercase();
                align_y = match t.as_str() {
                    "top" => Align::Start,
                    "center" => Align::Center,
                    "bottom" => Align::End,
                    _ => Align::Start,
                }
            }
            "position-offset-x" => offset_x = parse_offset(value),
            "position-offset-y" => offset_y = parse_offset(value),
            "justify-content" => justify_content = parse_justify(value),
            "align-items" => align_items = parse_align_items(value),
            "padding" => set_edges_all(&mut padding, strict(parse_px_strict(value))?),
            "margin" => set_edges_all(&mut margin, strict(parse_px_strict(value))?),
            "padding-top" | "padding-right" | "padding-bottom" | "padding-left" => {
                set_edge_side(&mut padding, name, strict(parse_px_strict(value))?)
            }
            "margin-top" | "margin-right" | "margin-bottom" | "margin-left" => {
                set_edge_side(&mut margin, name, strict(parse_px_strict(value))?)
            }
            "border-size" => border_size = strict(parse_px_strict(value))?,
            "font-size" => font_size = strict(parse_px_strict(value)),
            "background" => background = strict_color(name, value, parse_color(value)),
            "font-color" => font_color = strict_color(name, value, parse_color(value)),
            "border-color" => border_color = strict_color(name, value, parse_color(value)),
            "visible" => visible = !matches!(value, "false" | "0" | "no"),
            "required" => required = !matches!(value, "false" | "0" | "no"),
            "required-indicator" => required_indicator = parse_required_indicator(value),
            "z" => z = strict_i32(name, value, value.trim().parse::<i32>().ok()).unwrap_or(0),
            other if other.starts_with(':') => {
                parse_fail(tag, other, value);
                return None;
            }
            _ => {}
        }
    }
    let ox = overflow_x.unwrap_or(overflow);
    let oy = overflow_y.unwrap_or(overflow);
    Some(Style {
        width,
        height,
        min_width,
        min_height,
        max_width,
        max_height,
        flex_grow,
        flex_shrink,
        position,
        overflow_x: ox,
        overflow_y: oy,
        white_space,
        align_x,
        align_y,
        offset_x,
        offset_y,
        justify_content,
        align_items,
        padding,
        margin,
        background,
        font_color,
        border_color,
        font_size,
        border_size,
        visible,
        required,
        invalid: false,
        required_indicator,
        z,
    })
}

fn parse_size(value: &str) -> Option<Size> {
    Some(parse_size_or_auto(value))
}

fn parse_flex(value: &str) -> Option<f32> {
    let n: f32 = value.trim().parse().ok()?;
    if !n.is_finite() || n < 0.0 {
        return None;
    }
    Some(n)
}

/// Строго разбирает значение в пикселях (`12`, `12px`): нечисло — ошибка
/// (возвращает `None`). Пустое значение даёт `0.0`.
fn parse_px_strict(value: &str) -> Option<f32> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Some(0.0);
    }
    let number = trimmed.strip_suffix("px").unwrap_or(trimmed).trim();
    number.parse::<f32>().ok()
}

fn set_edges_all(edges: &mut Edges, px: f32) {
    edges.top = px;
    edges.right = px;
    edges.bottom = px;
    edges.left = px;
}

fn set_edge_side(edges: &mut Edges, name: &str, px: f32) {
    match name {
        "padding-top" | "margin-top" => edges.top = px,
        "padding-right" | "margin-right" => edges.right = px,
        "padding-bottom" | "margin-bottom" => edges.bottom = px,
        "padding-left" | "margin-left" => edges.left = px,
        _ => {}
    }
}

/// Строго разбирает атрибут `resize`: неизвестное значение — ошибка (возвращает
/// `None`). Пустое и `"none"` дают [`Resize::None`].
fn parse_resize_value(value: &str) -> Option<Resize> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("none") {
        return Some(Resize::None);
    }
    Some(parse_resize(trimmed))
}

// ---------------------------------------------------------------------------
// Вспомогательные функции
// ---------------------------------------------------------------------------

/// Значение первого атрибута с именем `name`.
fn attr_value<'a>(attrs: &'a [light_fw_html::Attr], name: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|a| a.name == name)
        .map(|a| a.value.as_str())
}

/// Является ли имя атрибута конструкцией, не поддерживаемой во время выполнения
/// (директива `v-*` или обработчик, кроме простого `@click`).
fn is_unsupported_directive(name: &str) -> bool {
    name.starts_with("v-") && name != "v-model" || (name.starts_with('@') && name != "@click")
}

/// Текстовое содержимое элемента (все текстовые дети склеены).
///
/// Возвращает `None`, если в подписи встречается интерполяция `{{ … }}`:
/// динамические подписи во время выполнения не поддерживаются.
#[cfg_attr(
    all(not(feature = "elements-checkbox"), not(feature = "elements-radio")),
    allow(dead_code)
)]
fn text_content(children: &[light_fw_html::Node]) -> Option<String> {
    let mut out = String::new();
    for child in children {
        if let light_fw_html::Node::Text(segments) = child {
            for segment in segments {
                match segment {
                    light_fw_html::Segment::Literal(text) => out.push_str(text),
                    light_fw_html::Segment::Path(_path) => {
                        return None;
                    }
                }
            }
        }
    }
    Some(out)
}

/// Запрашивает строковое значение поля через [`Component::eval_field`];
/// простой идентификатор (без точек) обязателен.
fn eval_simple<C: Component + ?Sized>(component: &C, name: &str) -> Option<String> {
    if name.contains('.') || name.is_empty() {
        return None;
    }
    match component.eval_field(name) {
        Some(v) => Some(v),
        None => {
            // Метод (а не поле): возвращаем пустую строку, чтобы runtime-рендер
            // продолжал работу. Статическое дерево будет содержать правильное
            // значение при следующей пересборке.
            Some(String::new())
        }
    }
}

/// Запрашивает булево значение поля: `"true"/"1"/"yes"/"on"` → `true`.
fn eval_bool<C: Component + ?Sized>(component: &C, name: &str) -> Option<bool> {
    let raw = eval_simple(component, name)?;
    Some(parse_bool(&raw))
}

/// Разрешает атрибут `:disabled`/`disabled`: динамический — через
/// [`Component::eval_field`], статический — по булеву флагу.
fn resolve_disabled<C: Component + ?Sized>(
    attrs: &[light_fw_html::Attr],
    component: &C,
) -> Option<bool> {
    if let Some(v) = attr_value(attrs, ":disabled") {
        return eval_bool(component, v.trim());
    }
    attr_value(attrs, "disabled")
        .map(|v| !matches!(v, "false" | "0" | "no"))
        .or(Some(false))
}

/// Разрешает атрибут `type` кнопки: `:type` — через [`Component::eval_field`],
/// статический `type` — по имени.
fn resolve_button_type<C: Component + ?Sized>(
    attrs: &[light_fw_html::Attr],
    component: &C,
) -> Option<ButtonType> {
    if let Some(v) = attr_value(attrs, ":type") {
        let name = v.trim();
        let field = eval_simple(component, name)?;
        return button_type_from_name(&field);
    }
    let name = attr_value(attrs, "type").unwrap_or("");
    button_type_from_name(name)
}

fn button_type_from_name(name: &str) -> Option<ButtonType> {
    let trimmed = name.trim().to_ascii_lowercase();
    match trimmed.as_str() {
        "" | "submit" => Some(ButtonType::Submit),
        "button" => Some(ButtonType::Button),
        "reset" => Some(ButtonType::Reset),
        _ => None,
    }
}

/// Собирает статические (не-директивные) атрибуты и тег для `with_tag`/`with_attrs`.
fn collect_meta(attrs: &[light_fw_html::Attr], tag: &str) -> (String, Vec<(String, String)>) {
    let mut out = Vec::new();
    for attr in attrs {
        let name = attr.name.as_str();
        if name.starts_with('@')
            || name.starts_with("v-")
            || name.starts_with(':')
            || matches!(name, "key" | "required-indicator" | "src" | "validator")
        {
            continue;
        }
        out.push((name.to_string(), attr.value.clone()));
    }
    (tag.to_string(), out)
}

/// Строит [`WindowConfig`] из атрибутов корневого `<window>`.
///
/// Иконка не поддерживается во время выполнения (требует `&'static [u8]`),
/// поэтому она игнорируется.
fn build_window_config(element: &light_fw_html::Element) -> WindowConfig {
    let mut cfg = WindowConfig::default();
    let get = |name: &str| attr_value(&element.attrs, name).map(str::to_string);
    if let Some(v) = get("width") {
        cfg.width = v.parse().unwrap_or(cfg.width);
    }
    if let Some(v) = get("height") {
        cfg.height = v.parse().unwrap_or(cfg.height);
    }
    if let Some(v) = get("min-width") {
        cfg.min_width = Some(v.parse().unwrap_or(cfg.width));
    }
    if let Some(v) = get("min-height") {
        cfg.min_height = Some(v.parse().unwrap_or(cfg.height));
    }
    if let Some(v) = get("max-width") {
        cfg.max_width = Some(v.parse().unwrap_or(cfg.width));
    }
    if let Some(v) = get("max-height") {
        cfg.max_height = Some(v.parse().unwrap_or(cfg.height));
    }
    if let Some(v) = get("title") {
        cfg.title = v.trim().to_string();
    }
    if let Some(v) = get("modal") {
        cfg.modal = if v.trim().is_empty() {
            true
        } else {
            parse_bool(&v)
        };
    }
    if let Some(v) = get("visible") {
        cfg.visible = if v.trim().is_empty() {
            true
        } else {
            parse_bool(&v)
        };
    }
    if let Some(v) = get("background")
        && let Some(color) = parse_color(&v)
    {
        cfg.background = color;
    }
    if let Some(v) = get("transparent") {
        cfg.transparent = if v.trim().is_empty() {
            true
        } else {
            parse_bool(&v)
        };
    }
    if let Some(v) = get("resizable") {
        cfg.resizable = parse_bool(&v);
    }
    if let Some(v) = get("id") {
        let id = v.trim().to_string();
        if !id.is_empty() {
            cfg.id = Some(id);
        }
    }
    if let Some(v) = get("parent") {
        let parent = v.trim().to_string();
        if !parent.is_empty() {
            cfg.parent = Some(parent);
        }
    }
    cfg
}
