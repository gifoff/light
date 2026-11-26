//! Plugin API: регистрация кастомных элементов (Этап 3).
//!
//! Плагин объявляет фабрику [`CustomElementFactory`] и кладёт её в
//! distributed-slice [`REGISTERED_COMPONENTS`] через атрибут
//! `#[linkme::distributed_slice]`. Сборка бинарника склеивает слайсы всех
//! крейтов, поэтому каждый подключённый плагин доступен ядру без явной
//! регистрации: [`lookup`] находит фабрику по тегу, а
//! [`crate::runtime::build_element`] вызывает его [`CustomElement::render`],
//! чтобы подставить результат как узел дерева вида
//! [`crate::vdom::NodeKind::Custom`].
//!
//! Примечание о `unsafe_code`: макрос `linkme::distributed_slice`
//! генерирует `#[link_section]`, что формально требует `unsafe`. В `lib.rs`
//! crate-level lint `unsafe_code` намеренно НЕ установлен (см. комментарий
//! в `lib.rs`) — единственный работающий вариант, не требующий выносить
//! реестр в отдельный sub-crate. Модуль сам по себе не содержит
//! `unsafe`-операторов.
//!
//! Примечание о `Send + Sync`: [`CustomElement`] реализует только `Any`.
//! Экземпляры живут в потоке отрисовки (ядро создаёт и использует их там же,
//! см. [`crate::runtime::build_custom_element`]), а реактивные `Property`
//! используют `Rc<RefCell<_>>` — межпоточные маркеры не нужны и ломали бы
//! типичный плагин с `Property`.
//!
//! ```ignore
//! struct Datepicker { selected: Option<String> }
//! impl CustomElement for Datepicker {
//!     fn tag(&self) -> &'static str where Self: Sized { "datepicker" }
//!     fn render(&self, attrs: &[(&'static str, String)], children: &[VElement]) -> VElement {
//!         // собрать из children/attrs произвольное поддерево
//!         unimplemented!()
//!     }
//! }
//! #[linkme::distributed_slice(REGISTERED_COMPONENTS)]
//! static DATEPICKER: CustomElementFactory = CustomElementFactory {
//!     tag: "datepicker",
//!     core_version_req: "^0.18",
//!     crate_name: "my-datepicker-plugin",
//!     provides: &["datepicker"],
//!     factory: || Box::new(Datepicker { selected: None }),
//! };
//! ```

use std::any::Any;

use crate::vdom::{Event, VElement};

/// Кастомный элемент, регистрируемый плагином.
///
/// Объектно-безопасный (object-safe): экземпляр хранится в
/// `Box<dyn CustomElement>` и создаётся фабрикой
/// [`CustomElementFactory::factory`]. Тег фиксируется статически
/// ([`tag`](Self::tag)), рендер — [`render`](Self::render), реакция на
/// события — необязательный [`on_event`](Self::on_event).
///
/// Границы: **только [`Any`]** — без `Send + Sync`. Реактивные свойства
/// (`Property`) используют `Rc<RefCell<_>>` и живут исключительно в потоке
/// отрисовки; ядро создаёт/вызывает экземпляры в том же потоке и не держит их
/// между кадрами, поэтому межпоточные маркеры не требуются. Плагин с
/// собственным состоянием должен хранить его в `RwLock`/`Atomic*`, только
/// если он реально передаёт данные в другие потоки.
pub trait CustomElement: Any {
    /// Имя тега элемента (например `"datepicker"`). Статически, чтобы
    /// сравнивать при поиске фабрики по тегу.
    fn tag(&self) -> &'static str
    where
        Self: Sized;

    /// Строит виртуальное поддерево для этого элемента.
    ///
    /// `attrs` — атрибуты тега (имя + значение); `children` — уже разобранные
    /// дочерние узлы. Возвращаемое дерево должно иметь
    /// [`crate::vdom::NodeKind::Custom`] (обёртку накладывает runtime).
    fn render(&self, attrs: &[(&'static str, String)], children: &[VElement]) -> VElement;

    /// Реагирует на событие, доставленное этому элементу.
    ///
    /// По умолчанию ничего не делает; переопределяется элементами, которым
    /// нужна реактивность внутри. Перерисовка запрашивается через
    /// [`EventCtx::schedule_redraw`].
    ///
    /// **Собственное состояние** (Этап 5): ядро сохраняет экземпляр по ключу
    /// родительского узла и передаёт его в `on_event` как `&mut self`. Плагин
    /// может мутировать собственное состояние — например, `downcast_mut::<T>()`
    /// из [`Any`], чтобы привести `self` к конкретному типу и изменить поле, а
    /// затем вызвать `ctx.schedule_redraw()`. Следующий рендер увидит обновлённое
    /// состояние (макрос `#[component]` создаёт новый экземпляр при каждом
    /// `render`, поэтому мутации должны быть видны на следующем кадре).
    fn on_event(&mut self, _event: &Event, _ctx: &mut EventCtx<'_>) {}
}

/// Даункаст `dyn CustomElement` к конкретному типу.
///
/// Псевдоним для читаемости тестов и отладочного кода. В production-коде
/// не нужен — плагин уже знает свой тип внутри `on_event` (через `&mut self`).
///
/// ```rust
/// use light_vdom::plugins::{CustomElement, downcast_custom};
/// use std::cell::Cell;
///
/// struct Counter { count: Cell<i32> }
/// impl CustomElement for Counter {
///     fn tag(&self) -> &'static str { "counter" }
///     fn render(&self, _: &[(&str, String)], _: &[light_vdom::vdom::VElement]) -> light_vdom::vdom::VElement { unimplemented!() }
///     fn on_event(&mut self, e: &light_vdom::vdom::Event, ctx: &mut light_vdom::plugins::EventCtx) {
///         if matches!(e, light_vdom::vdom::Event::Click) {
///             self.count.set(self.count.get() + 1);
///             ctx.schedule_redraw();
///         }
///     }
/// }
/// let mut box_elem: Box<dyn CustomElement> = Box::new(Counter { count: Cell::new(0) });
/// let counter = downcast_custom::<Counter>(&*box_elem).expect("должен быть Counter");
/// assert_eq!(counter.count.get(), 0);
/// ```
pub fn downcast_custom<T: 'static>(elem: &dyn CustomElement) -> Option<&T> {
    // `CustomElement: Any`, therefore `&dyn CustomElement` coerces to `&dyn Any`
    // (unsize_coercion, stable since Rust 1.86).
    let any: &dyn Any = elem;
    any.downcast_ref::<T>()
}

/// Контекст события, передаваемый в [`CustomElement::on_event`].
///
/// Единственное действие, доступное элементу, — запросить перерисовку окна
/// ([`schedule_redraw`](Self::schedule_redraw)). Поле `redraw` скрыто
/// (`pub(crate)`): плагин не имеет прямого доступа к компоненту.
pub struct EventCtx<'a> {
    /// Замыкание, вызываемое при перерисовке (внутреннее).
    pub redraw: &'a dyn Fn(),
}

impl<'a> EventCtx<'a> {
    /// Создаёт новый контекст событий.
    ///
    /// `redraw` — замыкание, которое вызывается при
    /// [`schedule_redraw`](Self::schedule_redraw). В production-коде ядро
    /// создаёт этот экземпляр самостоятельно; публичный конструктор нужен
    /// для тестов и интеграционных примеров.
    pub fn new(redraw: &'a dyn Fn()) -> Self {
        Self { redraw }
    }

    /// Запрашивает перерисовку окна (вызывается оконным бэкендом после
    /// обработки всех событий текущего кадра).
    pub fn schedule_redraw(&self) {
        (self.redraw)();
    }
}

/// Фабрика кастомного элемента: метаданные + функция создания экземпляра.
///
/// Хранится в distributed-slice [`REGISTERED_COMPONENTS`]. Все поля
/// `&'static`, чтобы фабрика могла жить в статическом слайсе.
#[derive(Debug)]
pub struct CustomElementFactory {
    /// Тег, под которым элемент ищется (`lookup`).
    pub tag: &'static str,
    /// Требуемая версия ядра в semver-формате (например `^0.18`). Проверяется
    /// [`check_core_version`] перед созданием экземпляра.
    pub core_version_req: &'static str,
    /// Название крейта-плагина (для диагностики).
    pub crate_name: &'static str,
    /// Теги/возможности, предоставляемые этим крейтом.
    pub provides: &'static [&'static str],
    /// Функция создания экземпляра элемента.
    pub factory: fn() -> Box<dyn CustomElement>,
}

/// Distributed-slice всех зарегистрированных плагинов.
///
/// Пустой, если ни один плагин не подключён. Плагин добавляет фабрику:
/// `#[linkme::distributed_slice(REGISTERED_COMPONENTS)]`.
#[linkme::distributed_slice]
pub static REGISTERED_COMPONENTS: [CustomElementFactory];

/// Находит зарегистрированную фабрику по тегу.
///
/// Линейный поиск: количество плагинов невелико (единицы-десятки), так что
/// хэш-таблица не нужна.
pub fn lookup(tag: &str) -> Option<&'static CustomElementFactory> {
    REGISTERED_COMPONENTS
        .iter()
        .find(|factory| factory.tag == tag)
}

/// Проверяет, удовлетворяет ли текущая версия ядра требование `req`.
///
/// Поддерживается префикс `^MAJOR.MINOR[.PATCH]` (semver caret). Считает
/// версию `[MAJOR, MINOR, PATCH]` из [`env!("CARGO_PKG_VERSION")`] и
/// сравнивает:
/// - `MAJOR == 0`: совместимо, если текущий `MINOR >= req_minor` (patch
///   игнорируется, как принято для 0.x версий);
/// - `MAJOR > 0`: `MAJOR` равны, `MINOR >= req_minor`, при равенстве `MINOR`
///   — `PATCH >= req_patch` (если patch задан в требовании).
///
/// Валидным считается только формат `^N.N[N]` — всё остальное (включая
/// пустую строку и опечатки) ошибается.
///
/// Возвращает `Ok(())`, если требование выполнено, иначе `Err` с причиной.
pub fn check_core_version(req: &str) -> Result<(), String> {
    let Some(caret) = req.strip_prefix('^') else {
        return Err(format!(
            "неподдерживаемый формат версии: {req:?} (ожидалось ^MAJOR.MINOR)"
        ));
    };
    let parts: Vec<&str> = caret.split('.').collect();
    if parts.len() < 2 || parts.len() > 3 {
        return Err(format!(
            "неподдерживаемый формат версии: {req:?} (ожидалось ^MAJOR.MINOR)"
        ));
    }
    let req_major: u64 = parts[0]
        .parse()
        .map_err(|_| format!("неверная major-версия: {req:?}"))?;
    let req_minor: u64 = parts[1]
        .parse()
        .map_err(|_| format!("неверная minor-версия: {req:?}"))?;
    // Patch: опционален; по умолчанию 0.
    let req_patch: u64 = parts
        .get(2)
        .and_then(|p| p.trim().parse::<u64>().ok())
        .unwrap_or(0);

    let cur = env!("CARGO_PKG_VERSION");
    let (cur_major, cur_minor, cur_patch): (u64, u64, u64) =
        parse_triple(cur).ok_or_else(|| format!("не удалось разобрать версию ядра {cur:?}"))?;

    let ok = if req_major == 0 {
        // ^0.N: совместимо при MINOR >= N (patch игнорируется).
        cur_major == 0 && cur_minor >= req_minor
    } else {
        // ^N.N(.P) при N > 0: MAJOR равны, MINOR >= N, при равенстве MINOR —
        // PATCH >= P.
        cur_major == req_major
            && (cur_minor > req_minor || (cur_minor == req_minor && cur_patch >= req_patch))
    };
    if ok {
        Ok(())
    } else {
        Err(format!(
            "требуемая версия ядра {req:?} несовместима с текущей {cur:?}"
        ))
    }
}

/// Разбирает `MAJOR.MINOR.PATCH` в тройку чисел.
///
/// Pre-release (`-…`) и build-метаданные (`+…`) игнорируются для тройки
/// совместимости: alpha-ядро `0.21.0-alpha.1` считается своим release-триплетом
/// `(0, 21, 0)`. Это осознанное упрощение кастомного матчера (настоящая
/// semver-семантика — отдельный ticket); формат требования остаётся
/// `^MAJOR.MINOR[.PATCH]`.
fn parse_triple(s: &str) -> Option<(u64, u64, u64)> {
    let base = s.split(['-', '+']).next().unwrap_or(s);
    let mut nums = base.split('.').take(3).map(str::trim);
    let major = nums.next()?.parse::<u64>().ok()?;
    let minor = nums.next()?.parse::<u64>().ok()?;
    let patch = nums.next()?.parse::<u64>().ok()?;
    Some((major, minor, patch))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vdom::NodeKind;

    /// Фиктивный плагин «datepicker», зарегистрированный в slice.
    ///
    /// Не содержит состояния: достаточно для проверки, что runtime
    /// действительно вызывает `render` и оборачивает результат в
    /// `NodeKind::Custom`. Реальные плагины хранят состояние в
    /// реактивных `Property` (один поток отрисовки) или в `RwLock`/
    /// `AtomicUsize`, если данные уходят в другие потоки.
    struct FakeDatepicker;

    impl CustomElement for FakeDatepicker {
        fn tag(&self) -> &'static str
        where
            Self: Sized,
        {
            "datepicker"
        }

        fn render(&self, attrs: &[(&'static str, String)], children: &[VElement]) -> VElement {
            let _ = (attrs, children);
            // Строим простое поддерево: Column с одним Text.
            VElement::container(
                "",
                NodeKind::Column,
                vec![VElement::leaf(
                    "",
                    NodeKind::Text {
                        content: "DatePicker".to_string(),
                    },
                )],
            )
        }
    }

    /// Фабрика фиктивного datepicker'а.
    ///
    /// Возвращает новый `Box` для каждого вызова (как в реальном плагине).
    const FAKE_DATEPICKER: CustomElementFactory = CustomElementFactory {
        tag: "datepicker",
        core_version_req: "^0.18",
        crate_name: "fake-datepicker",
        provides: &["datepicker"],
        factory: || Box::new(FakeDatepicker),
    };

    #[linkme::distributed_slice(REGISTERED_COMPONENTS)]
    static FAKE_DATEPICKER_ENTRY: CustomElementFactory = FAKE_DATEPICKER;

    #[test]
    fn lookup_finds_registered_datepicker() {
        let factory = lookup("datepicker");
        assert!(factory.is_some(), "datepicker должен быть зарегистрирован");
        let f = factory.unwrap();
        assert_eq!(f.tag, "datepicker");
        assert_eq!(f.crate_name, "fake-datepicker");
        assert_eq!(f.provides, &["datepicker"]);
    }

    #[test]
    fn lookup_unknown_returns_none() {
        assert!(lookup("unknown-tag").is_none());
    }

    #[test]
    fn parse_triple_strips_prerelease_and_build() {
        // Pre-release и build-метаданные игнорируются для тройки совместимости.
        assert_eq!(parse_triple("0.21.0-alpha.1"), Some((0, 21, 0)));
        assert_eq!(parse_triple("1.2.3+build.5"), Some((1, 2, 3)));
        assert_eq!(parse_triple("1.2.3"), Some((1, 2, 3)));
        // Мусор (не число на первой позиции) → None.
        assert_eq!(parse_triple("abc"), None);
        // Пустая строка: нет ни major, ни minor → None.
        assert_eq!(parse_triple(""), None);
    }

    /// Текущая версия ядра как тройка (без pre-release/build — см. doc `parse_triple`).
    fn cur_version_triple() -> (u64, u64, u64) {
        let v = env!("CARGO_PKG_VERSION");
        parse_triple(v).unwrap_or_else(|| panic!("не удалось разобрать CARGO_PKG_VERSION {v:?}"))
    }

    #[test]
    fn check_core_version_accepts_compatible() {
        // Ожидания выводятся из текущей версии ядра (env! CARGO_PKG_VERSION),
        // а не хардкодятся: после bump до 0.21.x старый хардкод ^0.18 падал.
        let (cur_major, cur_minor, _) = cur_version_triple();
        // ^MAJOR.MINOR текущей тройки — всегда совместимо (minor-floor совпадает).
        let req_current = format!("^{cur_major}.{cur_minor}");
        assert!(
            check_core_version(&req_current).is_ok(),
            "^{cur_major}.{cur_minor} должно пройти"
        );
        // ^0.0 проходит при любой 0.x версии (minor >= 0).
        assert!(check_core_version("^0.0").is_ok());
        // Для 0.x с cur_minor > 0: предшествующий минор тоже совместим
        // (minor-floor: cur_minor >= cur_minor - 1).
        if cur_major == 0 && cur_minor > 0 {
            let prev = format!("^0.{}", cur_minor - 1);
            assert!(
                check_core_version(&prev).is_ok(),
                "{prev} должно проходить (0.x minor-floor)"
            );
        }
    }

    #[test]
    fn check_core_version_rejects_incompatible() {
        let (cur_major, cur_minor, _) = cur_version_triple();
        // Минор вперёд на один — несовместимо (требование выше текущей версии).
        let ahead = format!("^{}.{cur_minor}", cur_major + 1);
        assert!(check_core_version(&ahead).is_err());
        // Для 0.x: требование следующего минора тоже несовместимо.
        if cur_major == 0 {
            let next = format!("^0.{}", cur_minor + 1);
            assert!(
                check_core_version(&next).is_err(),
                "{next} должно ошибаться (0.x minor-floor)"
            );
        }
        // ^1.0.0 при cur_major == 0: MAJOR не совпадает → несовместимо.
        if cur_major == 0 {
            assert!(check_core_version("^1.0.0").is_err());
        }
        // Без карета и пустая строка — неподдерживаемый формат.
        assert!(check_core_version("0.18").is_err());
        assert!(check_core_version("").is_err());
    }

    #[test]
    fn check_core_version_cares_about_patch_for_nonzero_major() {
        // При MAJOR > 0 patch учитывается: для 0.x ядра любое ^N.M.P (N >= 1)
        // несовместимо из-за разницы MAJOR (patch-ветка недостижима при 0.x).
        let (cur_major, _, _) = cur_version_triple();
        if cur_major == 0 {
            assert!(check_core_version("^1.0.0").is_err());
            assert!(check_core_version("^2.3.4").is_err());
        } else {
            // Для N > 0: тот же major, меньший minor — несовместимо.
            if cur_major >= 2 {
                assert!(check_core_version(&format!("^{cur_major}.0")).is_err());
            }
        }
    }

    #[test]
    fn check_core_version_tolerates_prerelease_kernel() {
        // Regression-тест к BUG 0c: на pre-release ядре (например
        // 0.21.0-alpha.1) parse_triple отдаёт release-триплет (0, 21, 0),
        // поэтому "^0.21" должен проходить, хотя в строке есть "-alpha.1".
        // Triple игнорирует pre-release — это осознанное упрощение матчера.
        let (cur_major, cur_minor, _) = cur_version_triple();
        let req = format!("^{cur_major}.{cur_minor}");
        assert!(
            check_core_version(&req).is_ok(),
            "^{cur_major}.{cur_minor} должно проходить на ядре {cur:?} (pre-release игнорируется)",
            cur = env!("CARGO_PKG_VERSION")
        );
    }
}
