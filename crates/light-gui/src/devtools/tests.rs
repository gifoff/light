#[allow(clippy::module_inception)]
#[cfg(test)]
mod tests {
    //! Юнит-тесты ТЗ 0.16.0: сериализаторы `TREE`/`LAYOUT`, поиск `FIND`,
    //! и кодирование `SHOT`.

    use crate::devtools::*;
    use light_vdom::layout::{Layout, TextMetrics};
    use light_vdom::{ButtonType, NodeKind, VElement};

    /// Заглушка метрик текста: фиксированная ширина/высота (для тестов
    /// раскладки не важно — важны ключи и иерархия узлов).
    struct StubMetrics;
    impl TextMetrics for StubMetrics {
        fn measure(&self, _text: &str, font_size: f32) -> (f32, f32) {
            (font_size * 4.0, font_size * 1.5)
        }
    }

    /// Дерево для тестов: `window > div > [button "+1", text "Count"]`.
    fn sample_tree() -> VElement {
        VElement::container(
            "0",
            NodeKind::Window,
            vec![
                VElement::container(
                    "0.0",
                    NodeKind::Column,
                    vec![
                        VElement::container(
                            "0.0.0",
                            NodeKind::Button {
                                disabled: false,
                                button_type: ButtonType::default(),
                            },
                            vec![
                                VElement::leaf(
                                    "0.0.0.0",
                                    NodeKind::Text {
                                        content: "+1".to_owned(),
                                    },
                                )
                                .with_tag("span"),
                            ],
                        )
                        .with_tag("button"),
                        VElement::leaf(
                            "0.1",
                            NodeKind::Text {
                                content: "Count".to_owned(),
                            },
                        ),
                    ],
                )
                .with_tag("div"),
            ],
        )
        .with_tag("window")
        .with_window(light_vdom::component::WindowConfig::default())
    }

    /// `TREE`: текстовые узлы несут `text`, контейнеры — `tag` и пустой `text`.
    #[test]
    fn serialize_tree_shape() {
        let v = serialize_tree(&sample_tree());
        assert_eq!(v["key"], "0");
        assert_eq!(v["tag"], serde_json::json!("window"));
        assert_eq!(v["text"], serde_json::Value::Null);
        let div = &v["children"][0];
        assert_eq!(div["key"], "0.0");
        assert_eq!(div["tag"], serde_json::json!("div"));
        let button = &div["children"][0];
        assert_eq!(button["tag"], serde_json::json!("button"));
        // Кнопка — контейнер: собственный text = null (подпись — в потомках).
        assert_eq!(button["text"], serde_json::Value::Null);
        let text = &div["children"][1];
        assert_eq!(text["text"], serde_json::json!("Count"));
        assert_eq!(text["tag"], serde_json::Value::Null);
    }

    /// `LAYOUT`: плоское дерево с rect, children по индексу, tag из VDOM.
    #[test]
    fn serialize_layout_shape() {
        let tree = sample_tree();
        let layout = Layout::build(&tree, (400.0, 300.0), &StubMetrics);
        let v = serialize_layout(&layout, Some(&tree));
        assert_eq!(v["key"], "0");
        assert_eq!(v["tag"], serde_json::json!("window"));
        // Корень занимает весь viewport.
        assert_eq!(v["w"], 400.0);
        assert_eq!(v["h"], 300.0);
        assert!(v["x"].is_number());
        assert!(v["y"].is_number());
        assert!(!v["children"].as_array().unwrap().is_empty());
    }

    /// `FIND` по тегу + тексту кнопки: возвращает key и rect.
    #[test]
    fn find_element_by_tag_and_text() {
        let tree = sample_tree();
        let layout = Layout::build(&tree, (400.0, 300.0), &StubMetrics);
        let res = find_element(&tree, Some(&layout), r#"{"tag":"button","text":"+1"}"#)
            .expect("button found");
        assert_eq!(res["key"], "0.0.0");
        assert_eq!(res["tag"], serde_json::json!("button"));
        assert!(res["rect"]["w"].is_number());
    }

    /// `FIND` по атрибуту.
    #[test]
    fn find_element_by_attr() {
        let mut tree = sample_tree();
        // Добавляем атрибут на кнопку.
        if let Some(div) = tree.children.get_mut(0)
            && let Some(btn) = div.children.get_mut(0)
        {
            btn.attrs.push(("class", "btn".to_owned()));
        }
        let res = find_element(&tree, None, r#"{"tag":"button","attr":{"class":"btn"}}"#)
            .expect("found by attr");
        assert_eq!(res["key"], "0.0.0");
    }

    /// `FIND` без совпадения → `None`; невалидный JSON → `None`.
    #[test]
    fn find_element_no_match_and_bad_json() {
        let tree = sample_tree();
        assert!(
            find_element(&tree, None, r#"{"tag":"input"}"#).is_none(),
            "no <input> in tree"
        );
        assert!(
            find_element(&tree, None, "not-json").is_none(),
            "bad json -> None"
        );
    }

    /// `SHOT`: выход начинается с `base64:` и декодируется в PNG-заголовок.
    #[cfg(feature = "devtools")]
    #[test]
    fn encode_screenshot_is_png_base64() {
        // 2x1 RGBA8.
        let buf: Vec<u8> = vec![255, 0, 0, 255, 0, 255, 0, 255];
        let out = encode_screenshot(&buf, 2, 1);
        let b64 = out.strip_prefix("base64:").expect("base64: prefix");
        use base64::Engine as _;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .expect("decodes");
        // PNG-магический байт-паттерн.
        assert_eq!(
            &bytes[0..8],
            &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]
        );
    }
}
