use super::types::{Node, Segment};

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
    let bytes = text.as_bytes();
    let mut start = 0;
    while start < bytes.len() && (bytes[start] as char).is_whitespace() {
        start += 1;
    }
    if !text[..start].contains('\n') {
        start = 0;
    }
    let mut end = bytes.len();
    while end > start && (bytes[end - 1] as char).is_whitespace() {
        end -= 1;
    }
    if !text[end..].contains('\n') {
        end = bytes.len();
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
