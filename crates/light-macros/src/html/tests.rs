#[allow(clippy::module_inception)]

#[cfg(test)]
mod tests {
    use crate::html::*;

    fn text_of(node: &Node) -> String {
        match node {
            Node::Text(segments) => segments
                .iter()
                .map(|segment| match segment {
                    Segment::Literal(text) => text.clone(),
                    Segment::Path(path) => format!("«{path}»"),
                })
                .collect(),
            Node::Element(_) => panic!("ожидался текстовый узел"),
        }
    }

    #[test]
    fn parses_nested_elements() {
        let nodes = parse("<div><text>Привет</text><button @click=\"go\">Ок</button></div>")
            .expect("должен разобраться");
        assert_eq!(nodes.len(), 1);
        let Node::Element(root) = &nodes[0] else {
            panic!("ожидался элемент");
        };
        assert_eq!(root.name, "div");
        assert_eq!(root.children.len(), 2);
        let Node::Element(button) = &root.children[1] else {
            panic!("ожидался элемент");
        };
        assert_eq!(button.name, "button");
        assert_eq!(button.attrs[0].name, "@click");
        assert_eq!(button.attrs[0].value, "go");
    }

    #[test]
    fn parses_self_closing_element() {
        let nodes =
            parse("<text v-model=\"name\" placeholder=\"Имя\" />").expect("должен разобраться");
        let Node::Element(element) = &nodes[0] else {
            panic!("ожидался элемент");
        };
        assert_eq!(element.name, "text");
        assert_eq!(element.attrs.len(), 2);
        assert!(element.children.is_empty());
    }

    #[test]
    fn parses_boolean_attribute() {
        let nodes = parse("<text v-model=\"text\" textarea></text>").expect("должен разобраться");
        let Node::Element(element) = &nodes[0] else {
            panic!("ожидался элемент");
        };
        let flag = element
            .attrs
            .iter()
            .find(|attr| attr.name == "textarea")
            .expect("атрибут textarea");
        assert!(flag.value.is_empty(), "флаг без значения");
    }

    #[test]
    fn parses_interpolation() {
        let nodes = parse("<div>Счётчик: {{ counter.count }}</div>").expect("должен разобраться");
        let Node::Element(element) = &nodes[0] else {
            panic!("ожидался элемент");
        };
        assert_eq!(
            element.children,
            vec![Node::Text(vec![
                Segment::Literal("Счётчик: ".into()),
                Segment::Path("counter.count".into()),
            ])]
        );
    }

    #[test]
    fn drops_whitespace_only_text_nodes() {
        let nodes =
            parse("<div>\n  <text>А</text>\n  <text>Б</text>\n</div>").expect("должен разобраться");
        let Node::Element(root) = &nodes[0] else {
            panic!("ожидался элемент");
        };
        assert_eq!(root.children.len(), 2);
    }

    #[test]
    fn collapses_multiline_content() {
        let nodes =
            parse("<text>\n    Привет,\n    {{ name }}!\n</text>").expect("должен разобраться");
        let Node::Element(element) = &nodes[0] else {
            panic!("ожидался элемент");
        };
        assert_eq!(
            element.children,
            vec![Node::Text(vec![
                Segment::Literal("Привет,".into()),
                Segment::Path("name".into()),
                Segment::Literal("!".into()),
            ])]
        );
    }

    #[test]
    fn single_quotes_are_accepted() {
        let nodes = parse("<button @click='go'>Ок</button>").expect("должен разобраться");
        let Node::Element(element) = &nodes[0] else {
            panic!("ожидался элемент");
        };
        assert_eq!(element.attrs[0].value, "go");
    }

    #[test]
    fn comments_are_skipped() {
        let nodes = parse("<!-- комментарий -->А").expect("должен разобраться");
        assert_eq!(nodes.len(), 1);
        assert_eq!(text_of(&nodes[0]), "А");
    }

    #[test]
    fn error_on_unclosed_tag() {
        let error = parse("<div><text>А</text>").expect_err("должна быть ошибка");
        assert!(
            error.contains("не закрыт тег"),
            "неожиданное сообщение: {error}"
        );
    }

    #[test]
    fn error_on_mismatched_tag() {
        let error = parse("<div></span>").expect_err("должна быть ошибка");
        assert!(
            error.contains("несоответствие тегов"),
            "неожиданное сообщение: {error}"
        );
    }

    #[test]
    fn error_on_unclosed_interpolation() {
        let error = parse("<text>{{ name</text>").expect_err("должна быть ошибка");
        assert!(
            error.contains("не закрыта конструкция"),
            "неожиданное сообщение: {error}"
        );
    }

    #[test]
    fn error_on_invalid_path() {
        let error = parse("<text>{{ name() }}</text>").expect_err("должна быть ошибка");
        assert!(
            error.contains("путь к свойству"),
            "неожиданное сообщение: {error}"
        );
    }

    #[test]
    fn error_on_unquoted_attr_value() {
        let error = parse("<div direction=vertical></div>").expect_err("должна быть ошибка");
        assert!(error.contains("кавычках"), "неожиданное сообщение: {error}");
    }

    fn root_element(node: &Node) -> &Element {
        match node {
            Node::Element(element) => element,
            Node::Text(_) => panic!("ожидался элемент"),
        }
    }

    fn attr_value<'a>(element: &'a Element, name: &str) -> Option<&'a str> {
        element
            .attrs
            .iter()
            .find(|attr| attr.name == name)
            .map(|attr| attr.value.as_str())
    }

    #[test]
    fn parses_static_a11y_attrs() {
        let nodes = parse(
            "<div role=\"tab\" aria-label=\"Вкладка\" aria-selected=\"true\" tabindex=\"0\">x</div>",
        )
        .expect("должен разобраться");
        let element = root_element(&nodes[0]);
        assert_eq!(attr_value(element, "role"), Some("tab"));
        assert_eq!(attr_value(element, "aria-label"), Some("Вкладка"));
        assert_eq!(attr_value(element, "aria-selected"), Some("true"));
        assert_eq!(attr_value(element, "tabindex"), Some("0"));
    }

    #[test]
    fn parses_dynamic_aria_binding() {
        let nodes = parse("<div :aria-expanded=\"state.open\" tabindex=\"-1\">x</div>")
            .expect("должен разобраться");
        let element = root_element(&nodes[0]);
        assert_eq!(attr_value(element, ":aria-expanded"), Some("state.open"));
        assert_eq!(attr_value(element, "aria-expanded"), None);
        assert_eq!(attr_value(element, "tabindex"), Some("-1"));
    }

    #[test]
    fn parses_boolean_a11y_attr() {
        let nodes = parse("<button required>Ок</button>").expect("должен разобраться");
        let element = root_element(&nodes[0]);
        assert_eq!(attr_value(element, "required"), Some(""));
    }
}

#[cfg(test)]
mod file_tests {
    #[test]
    fn parses_demo_template() {
        let src = include_str!("../../../light-demo/templates/main.html");
        let nodes = crate::html::parse(src).expect("шаблон демо-приложения должен разбираться");
        assert_eq!(nodes.len(), 1, "в корне ровно один элемент");
    }

    #[test]
    fn parses_doc_template() {
        let src = include_str!("../../../light/templates/doc_example.html");
        let nodes = crate::html::parse(src).expect("шаблон из документации должен разбираться");
        assert_eq!(nodes.len(), 1, "в корне ровно один элемент");
    }
}
