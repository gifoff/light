mod custom_smoke_tests {
    use crate::component::Component;
    use crate::runtime::custom::{drain_custom_instances, store_custom_instance};
    use crate::runtime::parse::build_custom_element;
    use crate::vdom::{NodeKind, VElement, intern_key};

    /// Тестовый компонент без полей и реактивности.
    struct NullComponent;
    impl Component for NullComponent {
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
            self
        }

        fn render(&self) -> VElement {
            VElement::container(intern_key(""), NodeKind::Column, Vec::new())
        }
    }

    /// Несуществующий тег → build_custom_element возвращает None (и запись
    /// в лог через parse_fail). Публичный путь — через parse_template, но
    /// здесь проверяется именно внутренний helper.
    #[test]
    fn build_custom_unregistered_returns_none() {
        // <totally-unknown-tag> с одним текстовым дочерним узлом.
        let el = light_fw_html::Element {
            name: "totally-unknown-tag".to_string(),
            attrs: Vec::new(),
            children: vec![light_fw_html::Node::Text(vec![
                light_fw_html::Segment::Literal("hi".to_string()),
            ])],
        };
        assert!(
            build_custom_element::<NullComponent>(&el, "0", &NullComponent).is_none(),
            "незарегистрированный тег должен давать None"
        );
    }

    // -----------------------------------------------------------------------
    // Этап 5 — реестр custom instances: store → drain roundtrip
    // -----------------------------------------------------------------------

    /// Фиктивный элемент, который не делает ничего в on_event.
    struct NoopCustom;
    impl crate::plugins::CustomElement for NoopCustom {
        fn tag(&self) -> &'static str
        where
            Self: Sized,
        {
            "noop-custom"
        }
        fn render(&self, _attrs: &[(&'static str, String)], _children: &[VElement]) -> VElement {
            VElement::leaf(intern_key(""), NodeKind::Custom("noop-custom"))
        }
    }

    /// `store_custom_instance` кладёт экземпляр под ключом, а
    /// `drain_custom_instances` выносит его обратно (очищая карту).
    ///
    /// Это unit-тест на сам реестр (без оконного бэкенда): проверка того,
    /// что `build_custom_element` действительно сохраняет инстанс, а
    /// `WindowState::redraw` сможет его забрать через drain.
    #[test]
    fn stage5_store_then_drain_roundtrip() {
        // Очистим карту от записей других тестов (тесты крутятся в одном
        // процессе, thread-local общий).
        let _ = drain_custom_instances();

        store_custom_instance("0.1", Box::new(NoopCustom));
        store_custom_instance("0.2", Box::new(NoopCustom));

        let drained = drain_custom_instances();
        assert_eq!(drained.len(), 2, "две записи должны быть в карте");
        assert!(drained.contains_key("0.1"), "ключ 0.1 обязан быть");
        assert!(drained.contains_key("0.2"), "ключ 0.2 обязан быть");
        // Тег можно проверить через downcast.
        let inst = &drained["0.1"];
        assert!(
            crate::plugins::downcast_custom::<NoopCustom>(&**inst).is_some(),
            "должен быть NoopCustom"
        );

        // После drain карта пуста.
        let again = drain_custom_instances();
        assert!(again.is_empty(), "после drain карта должна быть пуста");
    }
}
