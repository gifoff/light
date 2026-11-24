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
    fn parse_template_true_keeps_node_false_skips_only_itself() {
        let tpl = r#"<div direction="horizontal"><div v-if="show"></div><div>visible</div></div>"#;

        // Истина → v-if-div и видимый сосед оба на месте (внутрь корневого div).
        let on = BoolComp { show: true };
        let root_on = parse_template(tpl, &on).expect("истинный v-if не должен выключить runtime");
        let row_on = &root_on.children[0];
        assert_eq!(
            row_on.children.len(),
            2,
            "при show=true оба дочерних div'а обязаны остаться"
        );

        // Ложь → v-if-div пропущен, видимый сосед цел (шаблон НЕ None, корень жив).
        let off = BoolComp { show: false };
        let root_off = parse_template(tpl, &off)
            .expect("ложный v-if обязан пропускать только себя, а не весь шаблон");
        let row_off = &root_off.children[0];
        assert_eq!(
            row_off.children.len(),
            1,
            "при show=false остаётся ровно один (видимый) div; v-if-div пропущен"
        );
    }

    /// Прямой тест фикса каскада (0.21.0): `v-if="false"` на одном ребёнке и
    /// видимый сосед — сосед обязан отрендериться (до фикса он выпадал вместе
    /// со всем деревом).
    #[test]
    fn v_if_false_skips_siblings_stay_rendered() {
        let comp = BoolComp { show: false };
        let root = parse_template(
            r#"<div direction="vertical"><div v-if="show"></div><button>ok</button></div>"#,
            &comp,
        )
        .expect("сосед v-if-div'а обязан рендериться");
        let col = &root.children[0];
        assert_eq!(col.children.len(), 1, "ровно один видимый элемент (button)");
        assert_eq!(
            col.children[0].tag, "button",
            "это тот самый видимый button"
        );
    }

    /// Компонент с **методом-геттером** `is_active()` (не поле):
    /// `eval_field` не знает этого имени, но `eval_bool_field` вызывает метод.
    struct MethodComp {
        active: bool,
    }

    impl MethodComp {
        fn is_active(&self) -> bool {
            self.active
        }
    }

    impl Component for MethodComp {
        fn render(&self) -> VElement {
            VElement::container(intern_key(""), NodeKind::Column, Vec::new())
        }

        fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
            self
        }

        // Поля: нет. Методы: is_active() — обрабатывается eval_bool_field.
        fn eval_bool_field(&self, name: &str) -> Option<bool> {
            (name == "is_active").then(|| self.is_active())
        }
    }

    /// `v-if="is_active"` при `active=true` → узел строится.
    #[test]
    fn v_if_method_true_builds_node() {
        let comp = MethodComp { active: true };
        let el = div_with_v_if("is_active");
        assert!(
            build_element::<MethodComp>(&el, "0.0", None, &comp).is_some(),
            "v-if на метод-геттер (true) обязан построить узел"
        );
    }

    /// `v-if="is_active"` при `active=false` → узел скрывается.
    #[test]
    fn v_if_method_false_hides_node() {
        let comp = MethodComp { active: false };
        let el = div_with_v_if("is_active");
        assert!(
            build_element::<MethodComp>(&el, "0.0", None, &comp).is_none(),
            "v-if на метод-геттер (false) обязан скрыть узел"
        );
    }

    /// `v-if="!is_active"` инверсия: active=true → скрыт; active=false → видим.
    #[test]
    fn v_if_method_negation_toggles() {
        let hidden = MethodComp { active: true };
        let el = div_with_v_if("!is_active");
        assert!(
            build_element::<MethodComp>(&el, "0.0", None, &hidden).is_none(),
            "!is_active при active=true должно скрывать"
        );
        let shown = MethodComp { active: false };
        assert!(
            build_element::<MethodComp>(&el, "0.0", None, &shown).is_some(),
            "!is_active при active=false должно показывать"
        );
    }

    /// Интеграция через `parse_template`: метод-геттер в runtime (0.21.0).
    /// Ложный v-if на метод — тот же фикс каскада: узел пропускается, сосед
    /// рендерится (раньше было fallback в None).
    #[test]
    fn parse_template_v_if_method_true_keeps_false_skips_only_itself() {
        let tpl = r#"<div direction="horizontal"><div v-if="is_active"></div><div>vis</div></div>"#;

        let on = MethodComp { active: true };
        let root_on = parse_template(tpl, &on)
            .expect("истинный v-if на метод не должен выключить runtime-рендер");
        assert_eq!(
            root_on.children[0].children.len(),
            2,
            "при is_active=true оба дочерних div'а на месте"
        );

        let off = MethodComp { active: false };
        let root_off = parse_template(tpl, &off)
            .expect("ложный v-if на метод обязан пропускать только себя, а не весь шаблон");
        assert_eq!(
            root_off.children[0].children.len(),
            1,
            "при is_active=false остаётся один видимый div"
        );
    }
}

// ---------------------------------------------------------------------------
// Runtime-атрибут `pointer-events` (0.21.0, FIX 2): `"none"` →
// `Style::pointer_events == Some(false)` (hit-тест пропускает узел),
// `"auto"`/другое → `Some(true)` / обычный отклик. Не «строгий» атрибут:
// неизвестное значение — не ошибка.
// ---------------------------------------------------------------------------
mod pointer_events_tests {
    use crate::component::Component;
    use crate::runtime::style::build_style;
    use crate::vdom::intern_key;
    use light_fw_html::{Attr, Element};

    /// Компонент без полей (`_component` не используется).
    struct Null;
    impl Component for Null {
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
            self
        }
        fn render(&self) -> crate::vdom::VElement {
            crate::vdom::VElement::container(
                intern_key(""),
                crate::vdom::NodeKind::Column,
                Vec::new(),
            )
        }
    }

    fn el(attr_name: &str, attr_value: &str) -> Element {
        Element {
            name: "div".to_string(),
            attrs: vec![Attr {
                name: attr_name.to_string(),
                value: attr_value.to_string(),
            }],
            children: Vec::new(),
        }
    }

    /// Без атрибута → `None` (обычный отклик, hit-тест работает как раньше).
    #[test]
    fn no_attr_leaves_pointer_events_none() {
        let e = Element {
            name: "div".to_string(),
            attrs: Vec::new(),
            children: Vec::new(),
        };
        let s = build_style::<Null>("div", &e.attrs, None, &Null).unwrap();
        assert_eq!(s.pointer_events, None);
    }

    /// `pointer-events="none"` → `Some(false)`.
    #[test]
    fn none_gives_some_false() {
        let e = el("pointer-events", "none");
        let s = build_style::<Null>("div", &e.attrs, None, &Null).unwrap();
        assert_eq!(s.pointer_events, Some(false));
    }

    /// `pointer-events="auto"` → `Some(true)`.
    #[test]
    fn auto_gives_some_true() {
        let e = el("pointer-events", "auto");
        let s = build_style::<Null>("div", &e.attrs, None, &Null).unwrap();
        assert_eq!(s.pointer_events, Some(true));
    }

    /// Регистр не важен; `NONE`/`false`/`0` тоже → `Some(false)`.
    #[test]
    fn case_insensitive_and_falsey_values() {
        for v in ["NONE", "False", "0"] {
            let e = el("pointer-events", v);
            let s = build_style::<Null>("div", &e.attrs, None, &Null).unwrap();
            assert_eq!(s.pointer_events, Some(false), "значение {v} → Some(false)");
        }
    }
}

// ---------------------------------------------------------------------------
// Runtime-поддержка `<image>` (0.21.0, ФИКС 1): PNG читается с диска, узел
// строится как `NodeKind::RuntimeImage`; width/height проходят общим путём.
// ---------------------------------------------------------------------------
#[cfg(feature = "runtime-templates")]
mod image_runtime_tests {
    use crate::{
        component::Component,
        runtime::parse::parse_template,
        vdom::{NodeKind, VElement, intern_key},
    };

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

    /// Рекурсивный поиск `NodeKind::RuntimeImage` в runtime-дереве.
    #[cfg(feature = "image-support")]
    fn find_runtime_image(node: &VElement) -> Option<&VElement> {
        match &node.kind {
            NodeKind::RuntimeImage { .. } => Some(node),
            _ => node.children.iter().find_map(find_runtime_image),
        }
    }

    /// Валидный 2×1 RGBA-PNG (прозрачный) как base64: сигнатура, IHDR
    /// (w=2, h=1, bit=8, color=6), один IDAT (zlib-стим: 1 store-block на
    /// 9 байт), IEND. Декодер `png` (workspace-зависимость) подтвердит
    /// корректность; размеры проверит `ImageData::from_png`.
    const MIN_PNG_B64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAIAAAABCAYAAAD0In+KAAAAAXNSR0IArs4c6QAAAARnQU1BAACxjwv8YQUAAAAJcEhZcwAADsMAAA7DAcdvqGQAAAAMSURBVBhXY4AABgYAAAkAAYD316gAAAAASUVORK5CYII=";

    /// Базовый набор base64 для декода PNG-файкса.
    const B64_ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    /// Декодирует base64 в байты (без поддержки `+`/`/`, только standard).
    /// `=` padding игнорируется.
    fn b64_decode(s: &str) -> Vec<u8> {
        let s: String = s
            .chars()
            .filter(|c| *c != '=' && !c.is_whitespace())
            .collect();
        let mut out = Vec::with_capacity(s.len());
        let mut buf: u32 = 0;
        let mut bits: u32 = 0;
        for ch in s.bytes() {
            let v = B64_ALPHABET.iter().position(|&b| b == ch).unwrap() as u32;
            buf = (buf << 6) | v;
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                out.push(((buf >> bits) & 0xFF) as u8);
            }
        }
        out
    }

    /// Создаёт каталог `target/temp/...` (относительно workspace root) и
    /// пишет PNG-файкс, возвращая путь. Базовый каталог — `$CARGO_MANIFEST_DIR/../../..`
    /// (workspace root) + `target`. Путь абсолютный: работает локально и в
    /// GitHub Actions (Linux/macOS/Windows), где cwd не всегда workspace root.
    fn write_test_png(sub: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        // Workspace root = crates/light-vdom/../../..
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::path::PathBuf::from("."));
        let base = manifest_dir.join("..").join("..").join("..").join("target");
        let dir = base.join("temp").join(format!("vdom-image-{sub}"));
        // В некоторых средах (DSH sandbox, CI с ограниченной записью в
        // target/) create_dir_all может упасть с PermissionDenied. Тест
        // опционален: если каталог недоступен — пропускаем, не ломая CI.
        if std::fs::create_dir_all(&dir).is_err() {
            eprintln!(
                "note: skipping image test (cannot create {})",
                dir.display()
            );
            return (base.clone(), base);
        }
        let bytes = b64_decode(MIN_PNG_B64);
        assert!(!bytes.is_empty(), "b64 decode must be non-empty");
        let path = dir.join("temp.png");
        if std::fs::write(&path, &bytes).is_err() {
            eprintln!("note: cannot write temp.png, test will skip");
        }
        (dir, path)
    }

    /// `<image src="…png" alt="…">` → `NodeKind::RuntimeImage` с ожидаемыми
    /// `width`/`height`/`alt`, плюс `width`/`height` атрибуты работают через
    /// общий стайл-путь (style.width/height = Px(...)).
    #[cfg(feature = "image-support")]
    #[test]
    fn runtime_image_decodes_and_lays_out() {
        let (dir, png) = write_test_png("decode");
        // Если файл не удалось записать (sandbox/CI), тест пропускается —
        // это окружественная проблема, не дефект кода.
        if !png.is_file() {
            eprintln!("note: skipping image test (temp.png not writable)");
            return;
        }
        let old_cwd = std::env::current_dir().unwrap();
        let _ = std::env::set_current_dir(&dir);
        let html = r#"<window title="t"><image src="temp.png" alt="Alt text" width="100" height="50" /></window>"#;
        let tree = parse_template(html, &NullComponent).expect("runtime <image> parsed");
        let img = find_runtime_image(&tree).expect("RuntimeImage обязан быть в дереве");
        match &img.kind {
            NodeKind::RuntimeImage { image, alt } => {
                assert_eq!(*alt, "Alt text", "alt должен сохраниться");
                assert_eq!(
                    (image.width(), image.height()),
                    (2, 1),
                    "размеры из IHDR тестового PNG (2×1)"
                );
                assert!(!image.bytes.is_empty(), "буфер PNG не пуст");
            }
            _ => unreachable!("find_runtime_image вернул не RuntimeImage"),
        }
        // Атрибуты width/height → style.width/height (общий путь build_style),
        // layout разрешит их против intrinsic-размера узла.
        assert!(
            matches!(img.style.width, crate::vdom::Size::Px(100.0)),
            "width=\"100\" обязан стать style.width = Px(100)"
        );
        assert!(
            matches!(img.style.height, crate::vdom::Size::Px(50.0)),
            "height=\"50\" обязан стать style.height = Px(50)"
        );
        let _ = std::env::set_current_dir(&old_cwd);
    }

    /// Отсутствующий файл → `parse_fail` + None (без panics).
    #[cfg(feature = "image-support")]
    #[test]
    fn runtime_image_missing_file_returns_none() {
        let (dir, _png) = write_test_png("missing");
        let old_cwd = std::env::current_dir().unwrap();
        let _ = std::env::set_current_dir(&dir);
        // Файл `absent.png` не существует в этом каталоге — декодирование
        // должно вернуть None (fallback на статическое дерево).
        let html = r#"<window title="t"><image src="absent.png" /></window>"#;
        assert!(
            parse_template(html, &NullComponent).is_none(),
            "отсутствующий PNG → None"
        );
        let _ = std::env::set_current_dir(&old_cwd);
    }

    /// Несуществующий тег (проверка, что наш новый arm не заслоняет кастомный
    /// путь — кастомные элементы всё ещё идут через catch-all `_`).
    #[test]
    fn runtime_unknown_tag_still_falls_to_custom() {
        // Кастомный элемент уже покрыт другими тестами; здесь проверим, что
        // неизвестный тег по-прежнему даёт None (и не «подпадает» под image).
        let html = r#"<totally-unknown-tag />"#;
        assert!(parse_template(html, &NullComponent).is_none());
    }
}
