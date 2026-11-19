use super::helpers::split_interpolation;
use super::types::{Attr, Element, Node};

/// Разбирает источник HTML-шаблона в список корневых узлов.
///
/// Ошибки возвращаются в виде сообщения с номером строки.
pub fn parse(source: &str) -> Result<Vec<Node>, String> {
    // Файлы, сохранённые с BOM (UTF-8 BOM — байты EF BB BF), при чтении
    // дают символ `\u{FEFF}` в начале строки. Без очистки он становится
    // корневым текстовым узлом и ломает детекцию единственного корневого
    // `<window>` (см. `separate_window` в `light-fw-macros`).
    let source = source.trim_start_matches('\u{FEFF}');
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
pub(crate) fn validate_path(path: &str) -> bool {
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
