/// Текстовое содержимое берётся только из `NodeKind::Text`; у контейнеров
/// `text = null`, а `tag` — атрибут `tag` элемента (если задан), иначе `null`.
pub fn serialize_tree(element: &light_vdom::VElement) -> serde_json::Value {
    serialize_tree_inner(element)
}

fn serialize_tree_inner(el: &light_vdom::VElement) -> serde_json::Value {
    use light_vdom::NodeKind;
    let tag = if el.tag.is_empty() {
        None
    } else {
        Some(serde_json::Value::String(el.tag.to_owned()))
    };
    let text = match &el.kind {
        NodeKind::Text { content } => Some(serde_json::Value::String(content.clone())),
        _ => None,
    };
    let children: Vec<serde_json::Value> = el.children.iter().map(serialize_tree_inner).collect();
    let mut obj = serde_json::Map::new();
    obj.insert("key".into(), serde_json::Value::String(el.key.to_owned()));
    obj.insert("tag".into(), tag.unwrap_or(serde_json::Value::Null));
    obj.insert("text".into(), text.unwrap_or(serde_json::Value::Null));
    obj.insert("children".into(), serde_json::Value::Array(children));
    serde_json::Value::Object(obj)
}

/// ТЗ 0.16.0: сериализует плоское дерево раскладки в JSON для команды
/// `LAYOUT`.
///
/// Раскладка хранится как плоский список [`light_vdom::layout::PlacedNode`] с
/// `children: Vec<usize>` (индексы). Рекурсируем от узла с индексом 0
/// (корень), формируя `{"key":..,"tag":..,"x":..,"y":..,"w":..,"h":..,
/// "children":[...]}`. `tag` берётся из VDOM-дерева по ключу (если найден).
pub fn serialize_layout(
    layout: &light_vdom::layout::Layout,
    tree: Option<&light_vdom::VElement>,
) -> serde_json::Value {
    let nodes = layout.nodes();
    if nodes.is_empty() {
        return serde_json::Value::Null;
    }
    // Корень — узел с key "0" (первый элемент дерева) или индекс 0.
    // В нашей раскладке корень всегда индекс 0.
    serialize_layout_node(nodes, tree, 0, 0)
}

fn serialize_layout_node(
    nodes: &[light_vdom::layout::PlacedNode],
    tree: Option<&light_vdom::VElement>,
    index: usize,
    depth: usize,
) -> serde_json::Value {
    if depth > 100 || index >= nodes.len() {
        return serde_json::Value::Null;
    }
    let node = &nodes[index];
    // Tag: ищем элемент в VDOM по ключу.
    let tag = tree.and_then(|t| t.get(node.key)).and_then(|el| {
        if el.tag.is_empty() {
            None
        } else {
            Some(el.tag.to_owned())
        }
    });
    let rect = node.rect;
    let children: Vec<serde_json::Value> = node
        .children
        .iter()
        .map(|&ci| serialize_layout_node(nodes, tree, ci, depth + 1))
        .collect();
    let mut obj = serde_json::Map::new();
    obj.insert("key".into(), serde_json::Value::String(node.key.to_owned()));
    obj.insert(
        "tag".into(),
        tag.map(serde_json::Value::String)
            .unwrap_or(serde_json::Value::Null),
    );
    obj.insert("x".into(), serde_json::json!(rect.x));
    obj.insert("y".into(), serde_json::json!(rect.y));
    obj.insert("w".into(), serde_json::json!(rect.w));
    obj.insert("h".into(), serde_json::json!(rect.h));
    obj.insert("children".into(), serde_json::Value::Array(children));
    serde_json::Value::Object(obj)
}

/// ТЗ 0.16.0: выполняет поиск элемента по запросу (таг, текст, атрибуты).
///
/// Запрос — JSON-объект вида `{"tag":"button","text":"+1","attr":{"class":"btn"}}`.
/// Поля необязательны; совпадение — точное. Возвращает
/// `{"key":..,"tag":..,"rect":{"x":..,"y":..,"w":..,"h":..}}` или `None`,
/// если элемент не найден.
pub fn find_element(
    tree: &light_vdom::VElement,
    layout: Option<&light_vdom::layout::Layout>,
    query: &str,
) -> Option<serde_json::Value> {
    let q: serde_json::Value = serde_json::from_str(query).ok()?;
    let want_tag = q.get("tag").and_then(|v| v.as_str());
    let want_text = q.get("text").and_then(|v| v.as_str());
    let want_attrs: Vec<(String, String)> = q
        .get("attr")
        .and_then(|a| a.as_object())
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_owned())))
                .collect()
        })
        .unwrap_or_default();

    for el in tree.descendants() {
        if let Some(want_tag) = want_tag
            && el.tag != want_tag
        {
            continue;
        }
        if let Some(want_text) = want_text {
            // Текст узла: у текстовых узлов — собственный `content`; у
            // контейнеров (кнопки, поля) — конкатенация текстовых потомков
            // ВКЛЮЧИТЕЛЬНО через кнопку/поле (их подпись — это текст внутри).
            // Это позволяет искать кнопку по её подписи:
            // `{"tag":"button","text":"+1"}`.
            let mut buf = String::new();
            collect_match_text(el, &mut buf);
            if buf != want_text {
                continue;
            }
        }
        if !want_attrs.is_empty() {
            let all_match = want_attrs
                .iter()
                .all(|(k, v)| el.attr(k) == Some(v.as_str()));
            if !all_match {
                continue;
            }
        }
        // Найдено — собираем ответ с rect из раскладки.
        let tag = if el.tag.is_empty() {
            None
        } else {
            Some(el.tag.to_owned())
        };
        let rect = layout
            .and_then(|l| l.node(el.key))
            .map(|n| {
                serde_json::json!({
                    "x": n.rect.x,
                    "y": n.rect.y,
                    "w": n.rect.w,
                    "h": n.rect.h,
                })
            })
            .unwrap_or(serde_json::json!({"x": 0, "y": 0, "w": 0, "h": 0}));
        let mut obj = serde_json::Map::new();
        obj.insert("key".into(), serde_json::Value::String(el.key.to_owned()));
        obj.insert(
            "tag".into(),
            tag.map(serde_json::Value::String)
                .unwrap_or(serde_json::Value::Null),
        );
        obj.insert("rect".into(), rect);
        return Some(serde_json::Value::Object(obj));
    }
    None
}

/// Собирает текст поддерева для поиска `FIND`: текстовые узлы дают свой
/// `content`, всё остальное — суммарный текст потомков (включая кнопки/поля,
/// в отличие от [`VElement::collect_plain_text_public`], которая их
/// пропускает). Это нужно, чтобы найти кнопку по её подписи.
fn collect_match_text(el: &light_vdom::VElement, out: &mut String) {
    use light_vdom::NodeKind;
    if let NodeKind::Text { content } = &el.kind {
        out.push_str(content);
        return;
    }
    for child in &el.children {
        collect_match_text(child, out);
    }
}
