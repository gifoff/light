//! Разбор HTML-шаблонов Light.
//!
//! Поддерживаемый синтаксис:
//! * элементы `<div>`, `<span>`, `<h1>`…`<h5>`, `<text>`, `<button>`,
//!   `<titlebar>`, `<include>`, `<template>` (проверка имён — в `template.rs`);
//! * атрибуты в двойных или одинарных кавычках, включая директивы (`@click`, `v-model`, …);
//! * самозакрывающиеся теги `<text ... />`;
//! * комментарии `<!-- ... -->`;
//! * текст с интерполяцией свойств `{{ state.count }}`.
//!
//! Пробельные текстовые узлы между элементами отбрасываются; внутри содержимого
//! переводы строк нормализуются в пробелы, края обрезаются.

#![deny(missing_docs)]

/// Сегмент текстового узла шаблона.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment {
    /// Буквальный текст.
    Literal(String),
    /// Путь к реактивному свойству (например, `state.count`).
    Path(String),
}

/// Атрибут элемента шаблона.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attr {
    /// Имя атрибута (возможно, директива: `@click`, `v-model`, `:disabled`).
    pub name: String,
    /// Значение атрибута в кавычках.
    pub value: String,
}

/// Элемент шаблона.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Element {
    /// Имя тега (как в источнике).
    pub name: String,
    /// Атрибуты в порядке появления.
    pub attrs: Vec<Attr>,
    /// Дочерние узлы.
    pub children: Vec<Node>,
}

/// Узел шаблона.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    /// Элемент с атрибутами и детьми.
    Element(Element),
    /// Текст с интерполяцией.
    Text(Vec<Segment>),
}

/// Разбирает источник HTML-шаблона в список корневых узлов.
///
/// Ошибки возвращаются в виде сообщения с номером строки.
pub fn parse(source: &str) -> Result<Vec<Node>, String> {
    let mut parser = Parser {
        source,
        pos: 0,
        line: 1,
    };
    parser.parse_sequence(false)
}

struct Parser<'a> {
    source: &'a str,
    pos: usize,
    line: usize,
}

impl Parser<'_> {
    fn err(&self, message: impl std::fmt::Display) -> String {
        format!("строка {}: {message}", self.line)
    }

    fn eof(&self) -> bool {
        self.pos >= self.source.len()
    }

    fn rest(&self) -> &str {
        &self.source[self.pos..]
    }

    fn starts_with(&self, prefix: &str) -> bool {
        self.rest().starts_with(prefix)
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.rest().chars().next()?;
        self.pos += c.len_utf8();
        if c == '\n' {
            self.line += 1;
        }
        Some(c)
    }

    /// Продвигает позицию ровно на `n` байт (используется с байтовыми
    /// смещениями из `str::find`).
    fn bump_bytes(&mut self, n: usize) {
        let target = self.pos + n;
        while self.pos < target {
            let Some(c) = self.rest().chars().next() else {
                break;
            };
            self.pos += c.len_utf8();
            if c == '\n' {
                self.line += 1;
            }
        }
    }

    fn skip_ws(&mut self) {
        while let Some(c) = self.rest().chars().next() {
            if c.is_whitespace() {
                self.bump();
            } else {
                break;
            }
        }
    }

    /// Разбирает последовательность узлов до `</` (если `inside`) или до конца источника.
    fn parse_sequence(&mut self, inside: bool) -> Result<Vec<Node>, String> {
        let mut nodes = Vec::new();
        loop {
            if self.eof() {
                if inside {
                    return Err(self.err("не закрыт тег"));
                }
                return Ok(nodes);
            }
            if self.starts_with("</") {
                if inside {
                    return Ok(nodes);
                }
                return Err(self.err("лишний закрывающий тег"));
            }
            if self.starts_with("<!--") {
                self.skip_comment()?;
                continue;
            }
            if self.starts_with("<") {
                let element = self.parse_element()?;
                nodes.push(Node::Element(element));
                continue;
            }
            if let Some(node) = self.parse_text()? {
                nodes.push(node);
            }
        }
    }

    fn skip_comment(&mut self) -> Result<(), String> {
        for _ in 0..4 {
            self.bump();
        }
        match self.rest().find("-->") {
            None => Err(self.err("не закрыт комментарий")),
            Some(end) => {
                self.bump_bytes(end + 3);
                Ok(())
            }
        }
    }

    fn parse_element(&mut self) -> Result<Element, String> {
        self.bump(); // `<`
        let name = self.read_name("элемента")?;
        let mut attrs = Vec::new();
        loop {
            self.skip_ws();
            if self.eof() {
                return Err(self.err(format!("не закрыт тег <{name}>")));
            }
            if self.starts_with("/>") {
                self.bump();
                self.bump();
                return Ok(Element {
                    name,
                    attrs,
                    children: Vec::new(),
                });
            }
            if self.starts_with(">") {
                self.bump();
                break;
            }
            let attr = self.parse_attr(&name)?;
            attrs.push(attr);
        }

        let children = self.parse_sequence(true)?;

        if !self.starts_with("</") {
            return Err(self.err(format!("не закрыт тег <{name}>")));
        }
        self.bump();
        self.bump();
        let close = self.read_name("закрывающего тега")?;
        if close != name {
            return Err(self.err(format!("несоответствие тегов: <{name}> закрыт </{close}>")));
        }
        self.skip_ws();
        if !self.starts_with(">") {
            return Err(self.err(format!("не закрыт тег </{name}>")));
        }
        self.bump();
        Ok(Element {
            name,
            attrs,
            children,
        })
    }

    fn read_name(&mut self, what: &str) -> Result<String, String> {
        self.skip_ws();
        let start = self.pos;
        while let Some(c) = self.rest().chars().next() {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                self.bump();
            } else {
                break;
            }
        }
        let name = &self.source[start..self.pos];
        let first_is_alpha = name.chars().next().is_some_and(|c| c.is_ascii_alphabetic());
        if !first_is_alpha {
            return Err(self.err(format!("ожидалось имя {what}")));
        }
        Ok(name.to_string())
    }

    fn parse_attr(&mut self, element: &str) -> Result<Attr, String> {
        self.skip_ws();
        let start = self.pos;
        while let Some(c) = self.rest().chars().next() {
            if c.is_whitespace() || c == '=' || c == '>' || c == '/' {
                break;
            }
            self.bump();
        }
        let name = &self.source[start..self.pos];
        if name.is_empty() {
            return Err(self.err(format!("неожиданный символ в элементе <{element}>")));
        }
        if !is_valid_attr_name(name) {
            return Err(self.err(format!("недопустимое имя атрибута `{name}`")));
        }
        self.skip_ws();
        if !self.starts_with("=") {
            // Атрибут-флаг без значения (`<text textarea>`).
            return Ok(Attr {
                name: name.to_string(),
                value: String::new(),
            });
        }
        self.bump();
        self.skip_ws();
        let quote = match self.bump() {
            Some('"') => '"',
            Some('\'') => '\'',
            _ => return Err(self.err(format!("значение атрибута `{name}` должно быть в кавычках"))),
        };
        let value_start = self.pos;
        loop {
            match self.bump() {
                Some(c) if c == quote => break,
                Some(_) => {}
                None => return Err(self.err(format!("не закрыто значение атрибута `{name}`"))),
            }
        }
        let value = &self.source[value_start..self.pos - quote.len_utf8()];
        Ok(Attr {
            name: name.to_string(),
            value: value.to_string(),
        })
    }

    fn parse_text(&mut self) -> Result<Option<Node>, String> {
        let end = self.rest().find('<').unwrap_or(self.rest().len());
        let raw = &self.rest()[..end];
        let start_line = self.line;
        let segments = split_interpolation(raw, start_line)?;
        self.bump_bytes(end);
        Ok(segments)
    }
}

fn is_valid_attr_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' || c == '@' || c == ':' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '@' | ':'))
}

/// Проверяет, что путь состоит из идентификаторов, разделённых точками.
fn validate_path(path: &str) -> bool {
    !path.is_empty()
        && path.split('.').all(|segment| {
            let mut chars = segment.chars();
            match chars.next() {
                Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
                _ => return false,
            }
            chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        })
}

/// Разбирает текстовый узел на сегменты, выделяя `{{ ... }}`-интерполяцию.
///
/// Возвращает `Ok(None)`, если узел состоит только из пробелов.
fn split_interpolation(raw: &str, start_line: usize) -> Result<Option<Node>, String> {
    let mut segments: Vec<Segment> = Vec::new();
    let mut rest = raw;
    let mut line = start_line;
    loop {
        let Some(open) = rest.find("{{") else {
            segments.push(Segment::Literal(rest.to_string()));
            break;
        };
        segments.push(Segment::Literal(rest[..open].to_string()));
        let after = &rest[open + 2..];
        let line_here = line + rest[..open].matches('\n').count();
        let Some(close) = after.find("}}") else {
            return Err(format!(
                "строка {line_here}: не закрыта конструкция интерполяции «{{{{»"
            ));
        };
        let path = after[..close].trim();
        if !validate_path(path) {
            return Err(format!(
                "строка {line_here}: внутри «{{{{ }}}}» допустим только путь к свойству, \
                 например «{{{{ state.count }}}}», получено «{path}»"
            ));
        }
        segments.push(Segment::Path(path.to_string()));
        rest = &after[close + 2..];
        line = line_here + after[..=close].matches('\n').count();
    }

    normalize_segments(segments)
}

/// Нормализует сегменты: убирает краевые переводы строк, схлопывает
/// оставшиеся переводы в пробелы, обрезает края, удаляет пустые литералы.
/// Возвращает `None`, если нечего отображать.
fn normalize_segments(mut segments: Vec<Segment>) -> Result<Option<Node>, String> {
    for segment in &mut segments {
        if let Segment::Literal(text) = segment {
            *text = collapse_newlines(&trim_edge_newlines(text));
        }
    }
    if let Some(Segment::Literal(first)) = segments.first_mut() {
        *first = first.trim_start().to_string();
    }
    if let Some(Segment::Literal(last)) = segments.last_mut() {
        *last = last.trim_end().to_string();
    }
    segments.retain(|segment| !matches!(segment, Segment::Literal(text) if text.is_empty()));
    if segments.is_empty() {
        return Ok(None);
    }
    Ok(Some(Node::Text(segments)))
}

/// Удаляет пробельные строки на краях, содержащие перевод строки:
/// оформление шаблона вокруг `{{ … }}` не должно попадать в вывод.
fn trim_edge_newlines(text: &str) -> String {
    let mut start = 0;
    for (index, ch) in text.char_indices() {
        if !ch.is_whitespace() {
            break;
        }
        start = index + ch.len_utf8();
    }
    if !text[..start].contains('\n') {
        start = 0;
    }
    let mut end = text.len();
    for (index, ch) in text.char_indices().rev() {
        if index < start || !ch.is_whitespace() {
            break;
        }
        end = index;
    }
    if !text[end..].contains('\n') {
        end = text.len();
    }
    text[start..end].to_string()
}

/// Заменяет пробельные последовательности, содержащие перевод строки, одним пробелом.
fn collapse_newlines(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut run = String::new();
    for c in text.chars() {
        if c.is_whitespace() {
            run.push(c);
        } else {
            flush_run(&mut out, &run);
            run.clear();
            out.push(c);
        }
    }
    flush_run(&mut out, &run);
    out
}

fn flush_run(out: &mut String, run: &str) {
    if run.is_empty() {
        return;
    }
    if run.contains('\n') {
        out.push(' ');
    } else {
        out.push_str(run);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn trims_multibyte_text_at_line_edges() {
        let nodes = parse("<div>\n    Привет, счётчик\n</div>").expect("parse");
        let element = root_element(&nodes[0]);
        assert_eq!(text_of(&element.children[0]), "Привет, счётчик");
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
        let src = include_str!("../../light-demo/templates/main.html");
        let nodes = super::parse(src).expect("шаблон демо-приложения должен разбираться");
        assert_eq!(nodes.len(), 1, "в корне ровно один элемент");
    }

    #[test]
    fn parses_doc_template() {
        let src = include_str!("../../light/templates/doc_example.html");
        let nodes = super::parse(src).expect("шаблон из документации должен разбираться");
        assert_eq!(nodes.len(), 1, "в корне ровно один элемент");
    }
}
