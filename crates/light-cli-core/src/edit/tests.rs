#[allow(clippy::module_inception)]
#[cfg(test)]
mod tests {
    use crate::edit::*;

    #[test]
    fn adds_workspace_optional_dependency() {
        let mut doc =
            parse("[package]\nname = \"p\"\n\n[dependencies]\nlight = \"0.1\"\n").unwrap();
        set_workspace_optional(&mut doc, "light-fw-element-form").unwrap();
        let out = doc.to_string();
        assert!(out.contains("light-fw-element-form = { workspace = true, optional = true }"));
        assert!(out.contains("light = \"0.1\""), "форматирование сохранено");
    }

    #[test]
    fn adds_version_optional_dependency() {
        let mut doc = parse("[package]\nname = \"p\"\n").unwrap();
        set_version_optional(&mut doc, "light-fw-element-form", "0.1.0").unwrap();
        assert!(
            doc.to_string()
                .contains("light-fw-element-form = { version = \"0.1.0\", optional = true }")
        );
    }

    #[test]
    fn adds_features_to_string_and_table_dependencies() {
        // Фикстура после переименования обёртки: метакрейт теперь `light-fw`
        // (старые проекты ещё могут использовать `light`). Тест проверяет, что
        // фичи добавляются в оба существующих вида записи — строку и таблицу.
        let mut doc = parse(
            "[dependencies]\nlight-fw = \"0.6.1\"\nlight-fw-element-form = { version = \"0.6.1\", optional = true }\n",
        )
        .unwrap();
        add_features_to_dependency(
            &mut doc,
            "light-fw",
            &["elements-form".to_string(), "validation".to_string()],
        )
        .unwrap();
        add_features_to_dependency(&mut doc, "light-fw-element-form", &["default".to_string()])
            .unwrap();
        // Повторная установка той же фичи не дублирует её.
        add_features_to_dependency(&mut doc, "light-fw", &["validation".to_string()]).unwrap();
        let out = doc.to_string();
        assert!(
            out.contains(
                "light-fw = { version = \"0.6.1\", features = [\"elements-form\", \"validation\"] }"
            ),
            "{out}"
        );
        assert!(
            out.contains("light-fw-element-form = { version = \"0.6.1\", optional = true, features = [\"default\"] }"),
            "{out}"
        );
        assert_eq!(out.matches("\"validation\"").count(), 1, "{out}");
    }

    /// Регрессия: при добавлении фичи метакрейту `light add` **никогда** не
    /// должен выключать дефолтные фичи (`default-features = false`). Иначе
    /// `<window>` и прочие дефолтные элементы перестают работать. Поведение:
    /// * строка `light-fw = "0.17"` → `{ version = "0.17", features = [...] }`
    ///   **без** `default-features`;
    /// * табличная запись, где пользователь сам прописал
    ///   `default-features = false`, — сохранена как есть (только добавляются
    ///   фичи), то есть мы не трогаем явный выбор пользователя.
    #[test]
    fn add_features_never_disables_default_features() {
        // 1) Строковая зависимость без default-features → результат её не
        //    содержит.
        let mut doc = parse("[dependencies]\nlight-fw = \"0.17\"\n").unwrap();
        add_features_to_dependency(&mut doc, "light-fw", &["elements-form".to_string()]).unwrap();
        let out = doc.to_string();
        assert!(
            !out.contains("default-features"),
            "string dep must not gain default-features: {out}"
        );
        assert!(
            out.contains("light-fw = { version = \"0.17\", features = [\"elements-form\"] }"),
            "{out}"
        );

        // 2) Табличная запись, где пользователь явно выключил дефолтные фичи —
        //    это уважается; добавляется только фича.
        let mut doc = parse(
            "[dependencies]\nlight-fw = { version = \"0.17\", default-features = false, features = [\"a11y-winit\"] }\n",
        )
        .unwrap();
        add_features_to_dependency(&mut doc, "light-fw", &["elements-form".to_string()]).unwrap();
        let out = doc.to_string();
        assert!(
            out.contains("default-features = false"),
            "user-written default-features = false must be preserved: {out}"
        );
        assert!(out.contains("\"elements-form\""), "feature appended: {out}");
        assert!(
            out.contains("\"a11y-winit\""),
            "existing feature kept: {out}"
        );
    }

    /// ТЗ 0.17.2, задача 2: полный цикл мутаций автоподгрузки (`light build`/
    /// `light dev` при `<checkbox>`) должен дать ровно
    /// `light-fw = { version = "0.17", features = ["elements-form"] }` **без**
    /// `default-features = false`, а также опциональную зависимость и фичу
    /// `element-form`. Результат остаётся валидным (дефолтные элементы `<window>`
    /// не сломаны). Проверяются те же функции, что вызывает `do_add`.
    #[test]
    fn auto_install_full_cycle_preserves_default_features() {
        // Стартовое состояние: проект из `light init test-auto` — строковая
        // зависимость `light-fw = "0.17"` без default-features.
        let mut doc = parse(
            "[dependencies]\nlight-fw = \"0.17\"\n\n[features]\ndev-mode = [\"light-fw/dev-mode\"]\n",
        )
        .unwrap();

        // То, что делает `do_add`: ставит опциональную зависимость компонента,
        // добавляет фичи в метакреjt `light-fw` и создаёт фичу `element-form`.
        set_version_optional(&mut doc, "light-fw-element-form", "0.17").unwrap();
        add_features_to_dependency(&mut doc, "light-fw", &["elements-form".to_string()]).unwrap();
        add_dep_to_feature(&mut doc, "element-form", "light-fw-element-form").unwrap();

        let out = doc.to_string();

        // Критерий приёмки 2.4: ровно такая строка `light-fw` и ни одного
        // `default-features = false`.
        assert!(
            out.contains("light-fw = { version = \"0.17\", features = [\"elements-form\"] }"),
            "expected clean light-fw dep line: {out}"
        );
        assert!(
            !out.contains("default-features = false"),
            "auto-install must not disable default-features: {out}"
        );
        // Зависимость и фича на месте.
        assert!(
            out.contains("light-fw-element-form = { version = \"0.17\", optional = true }"),
            "{out}"
        );
        assert!(
            out.contains("element-form = [\"dep:light-fw-element-form\"]"),
            "{out}"
        );
        // Итог по-прежнему парсится (т.е. Cargo.toml валиден).
        assert!(
            parse(&out).is_ok(),
            "resulting Cargo.toml must stay valid: {out}"
        );
    }

    #[test]
    fn adds_dep_to_existing_and_new_feature() {
        let mut doc = parse("[features]\ndefault = [\"full\"]\nfull = [\"light/x\"]\n").unwrap();
        assert!(!add_dep_to_feature(&mut doc, "full", "light-fw-element-form").unwrap());
        assert!(add_dep_to_feature(&mut doc, "element-form", "light-fw-element-form").unwrap());
        assert!(!add_dep_to_feature(&mut doc, "full", "light-fw-element-form").unwrap());
        let out = doc.to_string();
        assert!(out.contains("\"dep:light-fw-element-form\""));
        assert!(out.contains("element-form = [\"dep:light-fw-element-form\"]"));
    }

    #[test]
    fn adds_dep_to_multiline_feature_keeps_lines_and_comments() {
        let text = "[features]\nfull = [\n    # формы\n    \"dep:light-fw-element-form\",\n]\n";
        let mut doc = parse(text).unwrap();
        add_dep_to_feature(&mut doc, "full", "light-widget-datepicker").unwrap();
        assert_eq!(
            doc.to_string(),
            "[features]\nfull = [\n    # формы\n    \"dep:light-fw-element-form\",\n    \"dep:light-widget-datepicker\",\n]\n"
        );
    }

    #[test]
    fn add_then_remove_is_byte_perfect_in_multiline_feature() {
        let text = "[features]\ndefault = [\"full\"]\nfull = [\n    \"dep:light-fw-element-form\",\n    \"dep:light-fw-behavior-validation\",\n]\n\
                    minimal = [\n    \"std\",\n]\n";
        let mut doc = parse(text).unwrap();
        add_dep_to_feature(&mut doc, "full", "light-widget-datepicker").unwrap();
        let added = doc.to_string();
        assert!(
            added.contains(
                "    \"dep:light-fw-element-form\",\n    \"dep:light-fw-behavior-validation\",\n    \"dep:light-widget-datepicker\",\n]"
            ),
            "соседние dep: не склеиваются: {added}"
        );
        let mut doc = parse(&added).unwrap();
        remove_dependency(&mut doc, "light-widget-datepicker").unwrap();
        assert_eq!(doc.to_string(), text, "add+remove восстанавливает файл");
    }

    #[test]
    fn removes_dependency_and_feature_tokens() {
        let mut doc = parse(
            "[dependencies]\nlight = \"0.1\"\nlight-fw-element-form = { workspace = true, optional = true }\n\n\
             [features]\ndefault = [\"full\"]\nfull = [\"dep:light-fw-element-form\", \"light/x\"]\n\
             element-form = [\"dep:light-fw-element-form\"]\n",
        )
        .unwrap();
        let touched = remove_dependency(&mut doc, "light-fw-element-form").unwrap();
        let mut sorted = touched.clone();
        sorted.sort();
        assert_eq!(sorted, vec!["element-form", "full"]);
        let out = doc.to_string();
        assert!(!out.contains("light-fw-element-form"));
        assert!(!out.contains("dep:"));
        assert!(out.contains("light/x"));
        assert!(out.contains("default"));
        assert!(out.contains("full"));
    }

    #[test]
    fn sets_dependency_version_string_and_table() {
        let mut doc = parse(
            "[dependencies]\nlight-fw-element-form = \"0.0.1\"\nlight-fw = { version = \"0.1\", features = [\"x\"] }\n",
        )
        .unwrap();
        set_dependency_version(&mut doc, "light-fw-element-form", "0.1.0").unwrap();
        set_dependency_version(&mut doc, "light-fw", "0.2.0").unwrap();
        let out = doc.to_string();
        assert!(out.contains("light-fw-element-form = \"0.1.0\""));
        assert!(out.contains("version = \"0.2.0\""));
        assert!(
            out.contains("features = [\"x\"]"),
            "features сохранены: {out}"
        );
    }

    #[test]
    fn remove_keeps_preexisting_empty_features() {
        let mut doc = parse(
            "[dependencies]\nlight-fw-element-form = { workspace = true, optional = true }\n\n\
             [features]\nminimal = []\nelement-form = [\"dep:light-fw-element-form\"]\n",
        )
        .unwrap();
        let touched = remove_dependency(&mut doc, "light-fw-element-form").unwrap();
        assert_eq!(touched, vec!["element-form"]);
        let out = doc.to_string();
        assert!(out.contains("minimal = []"), "пустая фича сохранена: {out}");
        assert!(
            !out.contains("element-form"),
            "опустевшая из-за удаления фича удалена: {out}"
        );
    }

    #[test]
    fn features_containing_reports_matches() {
        let text = "[features]\nfull = [\"dep:light-fw-element-form\"]\nminimal = []\n";
        assert_eq!(
            features_containing(text, "light-fw-element-form").unwrap(),
            vec!["full"]
        );
        assert!(has_feature(text, "full").unwrap());
        assert!(!has_feature(text, "nope").unwrap());
    }
}
