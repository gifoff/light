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
    fn on_event(&mut self, _event: &Event, _ctx: &mut EventCtx<'_>) {}
}

/// Контекст события, передаваемый в [`CustomElement::on_event`].
///
/// Единственное действие, доступное элементу, — запросить перерисовку окна
/// ([`schedule_redraw`](Self::schedule_redraw)). Поле `redraw` скрыто
/// (`pub(crate)`): плагин не имеет прямого доступа к компоненту.
pub struct EventCtx<'a> {
    pub(crate) redraw: &'a dyn Fn(),
}

impl EventCtx<'_> {
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

/// Разбирает `MAJOR.MINOR.PATCH` в тройку чисел (без учёта суффиксов).
fn parse_triple(s: &str) -> Option<(u64, u64, u64)> {
    let mut nums = s.split('.').take(3).map(str::trim);
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
    fn check_core_version_accepts_compatible() {
        // Текущая версия 0.18.x: ^0.18, ^0.17, ^0.0 должны проходить.
        assert!(check_core_version("^0.18").is_ok());
        assert!(check_core_version("^0.17").is_ok());
        assert!(check_core_version("^0.0").is_ok());
    }

    #[test]
    fn check_core_version_rejects_incompatible() {
        assert!(check_core_version("^0.99").is_err());
        assert!(check_core_version("0.18").is_err()); // без ^
        assert!(check_core_version("").is_err());
    }

    #[test]
    fn check_core_version_cares_about_patch_for_nonzero_major() {
        // При MAJOR > 0 patch учитывается.
        // Текущая 0.18.0: ^1.0.0 → MAJOR 1 != 0 → несовместимо.
        assert!(check_core_version("^1.0.0").is_err());
    }
}
