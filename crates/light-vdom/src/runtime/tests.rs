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

// ---------------------------------------------------------------------------
// Runtime-поддержка `v-if="поле"` / `v-if="!поле"` (0.21.0, FIX 2)
// ---------------------------------------------------------------------------
mod v_if_tests {
    use crate::component::Component;
    use crate::runtime::parse::{build_element, parse_template};
    use crate::vdom::{NodeKind, VElement, intern_key};

    /// Тестовый компонент с одним булевым полем `show`:
    /// `eval_field("show")` возвращает `"true"`/`"false"` по флагу.
    struct BoolComp {
        show: bool,
    }

    impl Component for BoolComp {
        fn render(&self) -> VElement {
            VElement::container(intern_key(""), NodeKind::Column, Vec::new())
        }

        fn eval_field(&self, name: &str) -> Option<String> {
            (name == "show").then(|| self.show.to_string())
        }

        fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
            self
        }
    }

    /// Компонент без полей: `eval_field` всегда `None` → `eval_simple`
    /// даёт пустую строку → v-if трактуется как «показывай».
    struct EmptyComp;
    impl Component for EmptyComp {
        fn render(&self) -> VElement {
            VElement::container(intern_key(""), NodeKind::Column, Vec::new())
        }
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
            self
        }
    }

    fn div_with_v_if(cond: &str) -> light_fw_html::Element {
        light_fw_html::Element {
            name: "div".to_string(),
            attrs: vec![light_fw_html::Attr {
                name: "v-if".to_string(),
                value: cond.to_string(),
            }],
            children: Vec::new(),
        }
    }

    /// `v-if="show"` при `show=true` → узел строится (`Some`).
    #[test]
    fn v_if_true_builds_node() {
        let comp = BoolComp { show: true };
        let el = div_with_v_if("show");
        assert!(
            build_element::<BoolComp>(&el, "0.0", None, &comp).is_some(),
            "вложенный элемент <div v-if=\"show\"> должен существовать при show=true"
        );
    }

    /// `v-if="show"` при `show=false` → узел не строится (`None`).
    #[test]
    fn v_if_false_hides_node() {
        let comp = BoolComp { show: false };
        let el = div_with_v_if("show");
        assert!(
            build_element::<BoolComp>(&el, "0.0", None, &comp).is_none(),
            "узел с ложным v-if обязан исчезнуть (не роняя весь парсинг)"
        );
    }

    /// `v-if="!show"` при `show=true` → скрыт; при `show=false` → видим.
    #[test]
    fn v_if_negation_toggles() {
        let hidden = BoolComp { show: true };
        let el = div_with_v_if("!show");
        assert!(
            build_element::<BoolComp>(&el, "0.0", None, &hidden).is_none(),
            "!show при show=true должно скрывать"
        );
        let shown = BoolComp { show: false };
        assert!(
            build_element::<BoolComp>(&el, "0.0", None, &shown).is_some(),
            "!show при show=false должно показывать"
        );
    }

    /// Нераспознанное поле (метод/пустое значение) → «показывай», узел на месте.
    #[test]
    fn v_if_unknown_field_defaults_visible() {
        let comp = EmptyComp;
        let el = div_with_v_if("some_method()");
        assert!(
            build_element::<EmptyComp>(&el, "0.0", None, &comp).is_some(),
            "непонятное условие в runtime не должно прятать узел (compile-time: показать)"
        );
    }

    /// Интеграция через публичный путь `parse_template`.
    ///
    /// Семантика ложного v-if в runtime **на уровне всего дерева**: как и до
    /// 0.21.0 для «неподдерживаемых» конструкций, сбой любого дочернего узла
    /// (`build_children` использует `?`) переводит runtime-рендер на статическое
    /// дерево из бинарника ([`render_dynamic`](crate::runtime::parse::render_dynamic)
    /// вернёт его через fallback). Поэтому:
    ///   * условие **истинно** → runtime-рендер работает, узел остаётся;
    ///   * условие **ложно**   → runtime отдаёт `None`, и статическое дерево
    ///     (собранное compile-time с тем же значением поля) берёт управление.
    ///
    /// Это корректно: статика всегда согласована с текущим state.
    #[test]
    fn parse_template_true_keeps_node_false_falls_back_to_static() {
        // Истина → runtime-дерево построено, живой div на месте.
        let on = BoolComp { show: true };
        let tree_on = parse_template(
            r#"<div direction="horizontal"><div v-if="show"></div></div>"#,
            &on,
        )
        .expect("истинный v-if не должен выключать runtime-рендер");
        assert_eq!(
            tree_on.children.len(),
            1,
            "при show=true внутренний div обязан остаться в runtime-дереве"
        );

        // Ложь → runtime-рендер возвращает None (статика возьмёт управление).
        let off = BoolComp { show: false };
        assert!(
            parse_template(
                r#"<div direction="horizontal"><div v-if="show"></div></div>"#,
                &off
            )
            .is_none(),
            "ложный v-if обязан откатить runtime на статическое дерево (None)"
        );
    }
}
