//! Трейт «реактивного класса» и его состояние.

use super::observer::PropertyId;

/// Реактивный класс (аналог `reactive`-объекта в Vue).
///
/// Макрос `#[proxy]` реализует этот трейт автоматически. Каждое свойство
/// структуры-потомка регистрируется у обозревателя, поэтому чтения и записи
/// участвуют в отслеживании зависимостей.
pub trait Proxy: 'static {
    /// Состояние реактивности этого объекта.
    fn proxy_state(&self) -> &ProxyState;

    /// Имя типа для диагностики.
    fn proxy_type_name(&self) -> &'static str {
        self.proxy_state().type_name
    }

    /// Уведомляет обозревателя об изменении всех свойств объекта
    /// (принудительная перерисовка всех зависимых элементов).
    fn invalidate(&self) {
        let properties: Vec<PropertyId> = self.proxy_state().properties.to_vec();
        for property in properties {
            super::observer::trigger(property);
        }
    }
}

/// Состояние реактивного класса: имя типа и список свойств.
#[derive(Clone, Debug)]
pub struct ProxyState {
    type_name: &'static str,
    properties: Box<[PropertyId]>,
}

impl PartialEq for ProxyState {
    /// Равенство — по имени типа: идентификаторы свойств принадлежат
    /// конкретному объекту и не должны мешать сравнению значений.
    fn eq(&self, other: &Self) -> bool {
        self.type_name == other.type_name
    }
}

impl ProxyState {
    /// Создаёт состояние для типа `T` с перечнем свойств.
    pub fn new<T: 'static>(properties: &[PropertyId]) -> Self {
        Self {
            type_name: std::any::type_name::<T>(),
            properties: properties.to_vec().into_boxed_slice(),
        }
    }

    /// Имя типа реактивного класса.
    pub fn type_name(&self) -> &'static str {
        self.type_name
    }

    /// Идентификаторы всех свойств объекта.
    pub fn properties(&self) -> &[PropertyId] {
        &self.properties
    }
}
