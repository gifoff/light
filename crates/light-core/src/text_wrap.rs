//! Разбиение текста на строки по словам.

/// Переносит `text` по словам так, чтобы ширина строки не превышала
/// `max_width`. Длинные слова без пробелов делятся по символам.
///
/// Возвращает строки вместе с их шириной.
pub(crate) fn wrap_words_with_widths<F>(
    text: &str,
    max_width: f32,
    mut measure: F,
) -> Vec<(String, f32)>
where
    F: FnMut(&str) -> f32,
{
    let limit = max_width.max(1.0);
    let space_width = measure(" ");
    let mut lines = Vec::new();

    for paragraph in text.split('\n') {
        let mut line = String::new();
        let mut line_width = 0.0;
        let mut has_word = false;

        for word in paragraph.split_whitespace() {
            let word_width = measure(word);

            if has_word {
                if line_width + space_width + word_width <= limit {
                    line.push(' ');
                    line.push_str(word);
                    line_width += space_width + word_width;
                    continue;
                }
                lines.push((std::mem::take(&mut line), line_width));
                line_width = 0.0;
                has_word = false;
            }

            if word_width <= limit {
                line.push_str(word);
                line_width = word_width;
                has_word = true;
                continue;
            }

            let mut chunk = String::new();
            let mut chunk_width = 0.0;
            for ch in word.chars() {
                let mut buffer = [0_u8; 4];
                let encoded = ch.encode_utf8(&mut buffer);
                let char_width = measure(encoded);
                if !chunk.is_empty() && chunk_width + char_width > limit {
                    lines.push((std::mem::take(&mut chunk), chunk_width));
                    chunk_width = 0.0;
                }
                chunk.push(ch);
                chunk_width += char_width;
            }
            if !chunk.is_empty() {
                line = chunk;
                line_width = chunk_width;
                has_word = true;
            }
        }

        lines.push((line, line_width));
    }

    if lines.is_empty() {
        lines.push((String::new(), 0.0));
    }
    lines
}

#[cfg(feature = "window")]
/// Переносит `text` по словам и возвращает только строки.
pub(crate) fn wrap_words<F>(text: &str, max_width: f32, measure: F) -> Vec<String>
where
    F: FnMut(&str) -> f32,
{
    wrap_words_with_widths(text, max_width, measure)
        .into_iter()
        .map(|(line, _)| line)
        .collect()
}

#[cfg(all(test, feature = "window"))]
mod tests {
    use super::wrap_words;

    fn fixed(text: &str) -> f32 {
        text.chars().count() as f32 * 10.0
    }

    #[test]
    fn wraps_at_word_boundaries() {
        let lines = wrap_words("one two three", 85.0, fixed);
        assert_eq!(lines, ["one two", "three"]);
    }

    #[test]
    fn splits_long_words() {
        let lines = wrap_words("abcdefgh", 35.0, fixed);
        assert_eq!(lines, ["abc", "def", "gh"]);
    }

    #[test]
    fn keeps_empty_paragraphs() {
        let lines = wrap_words("one\n\ntwo", 200.0, fixed);
        assert_eq!(lines, ["one", "", "two"]);
    }
}
