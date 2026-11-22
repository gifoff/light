use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex, MutexGuard},
    time::Duration,
};

use crate::{
    component::{Component, WindowConfig},
    plugins,
    vdom::{Align, NodeKind, Overflow, VElement, intern_key},
};

#[cfg(feature = "image-support")]
use crate::vdom::ImageData;

#[cfg(feature = "runtime-templates")]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(feature = "runtime-templates")]
use std::time::SystemTime;

use super::custom::{build_children, store_custom_instance};
use super::fields::{ROOT_KEY, build_checkbox, build_radio, build_select, build_text_field};
use super::style::{
    attr_value, build_style, build_window_config, collect_meta, eval_simple,
    is_unsupported_directive, resolve_button_type, resolve_disabled,
};

#[cfg_attr(not(feature = "runtime-templates"), allow(dead_code))]
/// Внутренний: кэш шаблонов (используется кастомными элементами).
pub(crate) static CACHE: LazyLock<Mutex<HashMap<&'static str, (Duration, String)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[cfg_attr(not(feature = "runtime-templates"), allow(dead_code))]
/// Внутренний: доступ к кэшу (используется кастомными элементами).
pub(crate) fn cache() -> MutexGuard<'static, HashMap<&'static str, (Duration, String)>> {
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
/// Внутренний: диагностика ошибки разбора (используется другими подмодулями).
#[cfg(debug_assertions)]
pub(crate) fn parse_fail(element: &str, attr_name: &str, attr_value: &str) {
    eprintln!(
        "light: runtime parse failed at {:?} (attr {:?}={:?})",
        element, attr_name, attr_value
    );
}
#[cfg(not(debug_assertions))]
/// Внутренний: диагностика ошибки разбора (release — тихо).
#[cfg(not(debug_assertions))]
pub(crate) fn parse_fail(_element: &str, _attr_name: &str, _attr_value: &str) {}

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
    /// Изображение (`<image>`): PNG-данные и `alt`.
    #[cfg(feature = "image-support")]
    Image(ImageData, String),
}

/// Эвалюирует директиву `v-if` в runtime-режиме.
///
/// Поддерживаются два синтаксиса (как и в compile-time макро):
/// 1. **Простое булево поле** (`v-if="show"`, `v-if="!show"`): значение
///    читается через [`Component::eval_field`] / [`eval_simple`](super::style::eval_simple),
///    нормализуется булевыми токенами `true/1/yes/on` по правилу
///    [`parse_bool`](crate::vdom::parse::parse_bool). Пустая строка или
///    нераспознанное слово → «показать» (compile-time семантика).
/// 2. **Метод-геттер без аргументов** (`v-if="is_home"`, `v-if="!is_home"`):
///    если имя не совпадает с полем, но совпадает с именем ноль-аргументного
///    метода, возвращающего `bool`, он вызывается через
///    [`Component::eval_bool_field`]. Это позволяет использовать пользовательские
///    методы для условной видимости в runtime.
///
/// Любой более сложный синтаксис (точки, аргументы, логические операторы)
/// **не** распознаётся — трактуется как «показать», чтобы узел не исчезал из-за
/// неподдерживаемой записи. Возвращаем `false` (скрыть) только когда условие
/// **явно** ложно.
#[cfg_attr(not(feature = "runtime-templates"), allow(dead_code))]
pub(crate) fn eval_v_if<C: Component + ?Sized>(component: &C, value: &str) -> bool {
    let v = value.trim();
    let (negate, name) = match v.strip_prefix('!') {
        Some(rest) => (true, rest.trim()),
        None => (false, v),
    };
    // 1) Простое булево поле (`eval_field`) — базовый путь.
    let raw = eval_simple(component, name).unwrap_or_default();
    if !raw.is_empty() {
        let recognized = matches!(
            raw.to_ascii_lowercase().as_str(),
            "true" | "1" | "yes" | "on" | "false" | "0"
        );
        let value_bool = if recognized {
            crate::vdom::parse::parse_bool(&raw)
        } else {
            true
        };
        return if negate { !value_bool } else { value_bool };
    }
    // 2) Метод-геттер (`v-if="is_home"`, `v-if="!is_home"`): `eval_field`
    //    возвращает None для методов (пустая строка в `eval_simple`), поэтому
    //    пробуем `eval_bool_field` — он вызвает ноль-аргументный метод, если
    //    имя совпадает с именем такого метода на компоненте.
    if let Some(value_bool) = component.eval_bool_field(name) {
        return if negate { !value_bool } else { value_bool };
    }
    // 3) Не удалось ни как поле, ни как метод → «показывай» (compile-time
    //    семантика: статическое дерево уже содержит правильную видимость).
    true
}

/// Строит один элемент (контейнер или лист) вместе со стилем, тегом, атрибутами
/// и, если задан `window_config`, настройками окна.
/// Внутренний: фабрика элемента (используется кастомными элементами).
pub(crate) fn build_element<C: Component + ?Sized>(
    element: &light_fw_html::Element,
    key: &str,
    window_config: Option<WindowConfig>,
    component: &C,
) -> Option<VElement> {
    // Директива `v-if` (compile-time и runtime): если условие явно ложно — узел
    // не строится вообще (видимость «скрываем»). Поддерживается простое поле
    // (`v-if="is_maximized"`) либо его инверсия (`v-if="!is_maximized"`); см.
    // [`eval_v_if`]. Если `v-if` задан — он **не** считается «неподдерживаемой»
    // директивой ниже (в отличие от `v-else-if`/`v-else`/`v-for`/`:attr`/
    // `@focus` и т.п. — их по-прежнему нельзя использовать в runtime).
    if let Some(cond) = attr_value(&element.attrs, "v-if")
        && !eval_v_if(component, cond)
    {
        return None;
    }
    for attr in &element.attrs {
        // `v-if` уже обработан выше — пропускаем его здесь.
        if attr.name == "v-if" {
            continue;
        }
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
        // `<image>`: PNG читается с диска в runtime; `width`/`height` обрабатываются
        // общим путём стилей ниже (layout берёт их против intrinsic-размера).
        #[cfg(feature = "image-support")]
        "image" => {
            let src = attr_value(&element.attrs, "src")
                .map(str::trim)
                .filter(|s| !s.is_empty());
            // Атрибут обязателен (тот же контракт, что в compile-time макросе).
            let Some(src) = src else {
                parse_fail(&element.name, "src", "");
                return None;
            };
            // Слэш-направление допускаем: шаблон может писать `assets\x.png`.
            let src = src.replace('\\', "/");
            let alt = attr_value(&element.attrs, "alt")
                .unwrap_or_default()
                .to_owned();
            // Файл читается относительно CWD — `light dev` работает в каталоге
            // проекта (как и macro: путь из `src` относителен к Cargo.toml).
            let bytes = match std::fs::read(&src) {
                Ok(bytes) => bytes,
                Err(e) => {
                    eprintln!("light: <image> не удалось прочитать файл {src:?}: {e}");
                    return None;
                }
            };
            let image = match ImageData::from_png(bytes) {
                Ok(image) => image,
                Err(e) => {
                    eprintln!("light: <image> не удалось декодировать PNG {src:?}: {e}");
                    return None;
                }
            };
            (ElementKind::Image(image, alt), None)
        }
        #[cfg(not(feature = "image-support"))]
        "image" => {
            // Явное сообщение вместо «неподдерживаемый тег»: тег известен, но
            // фича выключена. Формулировка как в compile-time макросе.
            eprintln!(
                "light: тег <image> недоступен: Cargo-фича `image-support` выключена\n \
                 help: включите её в зависимости, например \
                 `light-fw = {{ features = [\"image-support\"] }}`"
            );
            return None;
        }
        // Нестандартный тег: ищем зарегистрированную фабрику плагина (Plugin API,
        // Этап 3). Если плагин подключён — строим кастомный элемент через его
        // render(); если нет — это неподдерживаемый тег (откат как раньше).
        _ => match build_custom_element(element, key, component) {
            Some(custom) => return Some(custom),
            None => {
                parse_fail(&element.name, "tag", &element.name);
                return None;
            }
        },
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
        #[cfg(feature = "image-support")]
        ElementKind::Image(image, alt) => {
            // `ImageData` — дешёвый clone (Arc-подложка), alt — короткий String.
            VElement::leaf(
                intern_key(key),
                NodeKind::RuntimeImage {
                    image: image.clone(),
                    alt: alt.clone(),
                },
            )
        }
    };
    let style = build_style(&element.name, &element.attrs, default_font, component)?;
    let (raw_tag, raw_attrs) = collect_meta(&element.attrs, &element.name);
    let tag = intern_key(&raw_tag);
    let mut attrs: Vec<(&'static str, String)> = raw_attrs
        .into_iter()
        .map(|(name, value)| (intern_key(&name), value))
        .collect();
    // Для элементов с v-model сохраняем имя поля как data-атрибут, чтобы
    // макрос-генерируемый dispatch_vmodel мог написать новое значение
    // по ключу узла (runtime-дерево использует позиционные ключи).
    if let Some(model) = attr_value(&element.attrs, "v-model") {
        attrs.push(("data-v-model", model.to_owned()));
    }
    // Для radio дополнительно сохраняем data-value (значение этого варианта).
    if element.name == "radio"
        && let Some(val) = attr_value(&element.attrs, "value")
    {
        attrs.push(("data-value", val.to_owned()));
    }
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
    if is_heading
        && matches!(&kind, ElementKind::Container(NodeKind::Column))
        && let Some(child) = elem.children.first_mut()
    {
        child.style.justify_content = Align::Center;
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

/// Строит кастомный элемент, зарегистрированный плагином (Plugin API, Этап 3).
///
/// Ищет фабрику по тегу в distributed-slice [`plugins::REGISTERED_COMPONENTS`],
/// проверяет совместимость версии ядра, создаёт экземпляр и вызывает его
/// [`CustomElement::render`](crate::plugins::CustomElement::render) с
/// атрибутами тега (включая `v-model`, если задан — см. Этап 5) и уже
/// разобранными дочерними узлами. Результат оборачивается в
/// [`NodeKind::Custom`] с интернированным ключом — так layout/paint/diff
/// видят его как атомарное целое, а окно может доставлять события плагину.
///
/// Ключ узла (`key`) — ключ родительского тега (например `"0.1"`). С ним
/// связывается созданный экземпляр элемента в карте [`custom_instances`]:
/// при следующем рендере того же дерева запись перезаписывается свежим
/// экземпляром, а при отрисовке окна [`drain_custom_instances`] отдаёт их
/// оконному бэкенду для доставки событий через [`CustomElement::on_event`].
///
/// Возвращает `None`, если тег не зарегистрирован (это обычный
/// «неподдерживаемый тег» — откат к статическому дереву) или если версия
/// ядра несовместима с требованием плагина. В обоих случаях запись в лог идёт
/// через [`parse_fail`].
/// Внутренний: кастомный элемент (Plugin API).
pub(crate) fn build_custom_element<C: Component + ?Sized>(
    element: &light_fw_html::Element,
    key: &str,
    component: &C,
) -> Option<VElement> {
    let factory = plugins::lookup(&element.name)?;
    if let Err(msg) = plugins::check_core_version(factory.core_version_req) {
        parse_fail(&element.name, "core-version", &msg.to_string());
        return None;
    }
    // Атрибуты тега (имя + значение) передаются плагину как есть. Директивы
    // (`@click`/`:attr`/`v-if`) не поддерживаются в runtime-шаблонах и уже
    // отсеяны вызывающим кодом — сюда доходят только простые атрибуты.
    // `v-model` (если задан) уже присутствует в списке — плагин может
    // использовать его как имя поля в `on_event` (Этап 5).
    let attrs: Vec<(&'static str, String)> = element
        .attrs
        .iter()
        .map(|a| (intern_key(&a.name), a.value.clone()))
        .collect();
    // Дочерние узлы разбираем с обычным компонентом: тексты с `{{ }}`
    // интёрполируются геттером компонента, элементы — рекурсивно. Кастомный
    // тег внутри (если плагин вкладывает свой же тег) снова попадёт в
    // `build_custom_element` через общий путь `build_element`.
    let children = build_children(&element.children, ROOT_KEY, &NodeKind::Column, component)?;
    let instance = (factory.factory)();
    let mut rendered = instance.render(&attrs, &children);
    // Плагин вернул произвольное поддерево. Перекрашиваем корень в
    // Custom(tag): ядро узнаёт, что это чужой элемент, и layout/paint/diff
    // обрабатывают его как атомарное целое. Дочерние узлы остаются в
    // поддереве — layout развернёт их как у обычного контейнера.
    // Если плагин вернул лист (Text/Button и т. п.), оборачиваем его в
    // Column, чтобы Custom гарантированно оставался контейнером.
    if !matches!(
        rendered.kind,
        NodeKind::Column
            | NodeKind::Row
            | NodeKind::TitleBar
            | NodeKind::Window
            | NodeKind::Custom(_)
    ) {
        let col = VElement::container(intern_key(ROOT_KEY), NodeKind::Column, vec![rendered]);
        rendered = col;
    }
    let tag = intern_key(&element.name);
    rendered.kind = NodeKind::Custom(tag);
    // Сохраняем экземпляр по ключу родительского узла, чтобы окно могло
    // доставить ему события через on_event (Этап 5). Ключ интернируется:
    // при следующем рендере того же дерева запись перезапишется, при другом
    // дереве — старая запись будет удалена вместе с map при drain в redraw.
    store_custom_instance(key, instance);
    Some(rendered)
}

// ---------------------------------------------------------------------------
// Реестр кастомных элементов (Plugin API, Этап 5: доставка on_event)
// ---------------------------------------------------------------------------
