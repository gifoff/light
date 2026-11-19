#[cfg(test)]
mod tests {
    use crate::vdom::diff::{diff_kind, kind_compatible};
    use crate::vdom::events::Align;
    use crate::vdom::node::NodeKind;
    use crate::vdom::ops::Patch;
    use crate::vdom::parse::parse_align_items;

    #[test]
    fn parse_align_items_accepts_center() {
        assert_eq!(parse_align_items("center"), Align::Center);
        assert_eq!(parse_align_items("start"), Align::Start);
        assert_eq!(parse_align_items("end"), Align::End);
    }

    #[test]
    fn invalid_align_items_returns_start_not_panic() {
        let result = std::panic::catch_unwind(|| {
            // Некорректное значение не должно вызывать panic (в debug-сборке —
            // раньше это был `debug_assert!`); ожидается warning в stderr и
            // возврат значения по умолчанию.
            parse_align_items("ыефке")
        });
        assert!(
            result.is_ok(),
            "parse_align_items panicked on invalid value"
        );
        assert_eq!(result.unwrap(), Align::Start);
    }

    /// Кастомные элементы совместимы только при равном теге.
    #[test]
    fn custom_kind_compatible_by_tag() {
        assert!(kind_compatible(
            &NodeKind::Custom("datepicker"),
            &NodeKind::Custom("datepicker")
        ));
        assert!(!kind_compatible(
            &NodeKind::Custom("datepicker"),
            &NodeKind::Custom("calendar")
        ));
        // Custom несовместим со стандартным контейнером.
        assert!(!kind_compatible(
            &NodeKind::Custom("datepicker"),
            &NodeKind::Column
        ));
    }

    /// Diff Custom→Custom того же тега не генерирует ops (атомарное целое).
    #[test]
    fn diff_custom_same_tag_is_noop() {
        let mut patch = Patch::default();
        diff_kind(
            &NodeKind::Custom("datepicker"),
            &NodeKind::Custom("datepicker"),
            "k1",
            &mut patch,
        );
        assert!(patch.ops.is_empty());
    }
}
