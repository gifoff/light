use std::collections::BTreeSet;

use super::style::SelectOption;

#[derive(Debug, Clone, PartialEq)]
/// Точечное изменение одного узла между отрисовками.
pub enum Op {
    /// Изменилось содержимое текстового узла.
    SetContent {
        /// Ключ узла.
        key: &'static str,
        /// Новый текст.
        content: String,
    },
    /// Изменилась подпись кнопки.
    SetLabel {
        /// Ключ узла.
        key: &'static str,
        /// Новая подпись.
        label: String,
    },
    /// Изменился флаг disabled (кнопки или поля).
    SetDisabled {
        /// Ключ узла.
        key: &'static str,
        /// Новое значение.
        disabled: bool,
    },
    /// Изменилось значение текстового поля.
    SetValue {
        /// Ключ узла.
        key: &'static str,
        /// Новое значение.
        value: String,
    },
    /// Изменился placeholder текстового поля.
    SetPlaceholder {
        /// Ключ узла.
        key: &'static str,
        /// Новый placeholder.
        placeholder: String,
    },
    /// Изменился режим скрытия текста (`<password>`).
    SetMasked {
        /// Ключ узла.
        key: &'static str,
        /// Скрывать ли текст.
        masked: bool,
    },
    /// Изменилось состояние флажка.
    SetChecked {
        /// Ключ узла.
        key: &'static str,
        /// Новое состояние.
        checked: bool,
    },
    /// Изменилось состояние переключателя.
    SetSelected {
        /// Ключ узла.
        key: &'static str,
        /// Новое состояние.
        selected: bool,
    },
    /// Изменился набор пунктов выпадающего списка.
    SetOptions {
        /// Ключ узла.
        key: &'static str,
        /// Новые пункты.
        options: Vec<SelectOption>,
    },
    /// Узел добавлен вместе с поддеревом.
    Insert {
        /// Ключ узла.
        key: &'static str,
    },
    /// Узел удалён вместе с поддеревом.
    Remove {
        /// Ключ узла.
        key: &'static str,
    },
    /// Тип узла несовместим с прежним — узел заменён целиком.
    Replace {
        /// Ключ узла.
        key: &'static str,
    },
}

impl Op {
    /// Ключ затронутого узла.
    pub fn key(&self) -> &'static str {
        match self {
            Self::SetContent { key, .. }
            | Self::SetLabel { key, .. }
            | Self::SetDisabled { key, .. }
            | Self::SetValue { key, .. }
            | Self::SetPlaceholder { key, .. }
            | Self::SetMasked { key, .. }
            | Self::SetChecked { key, .. }
            | Self::SetSelected { key, .. }
            | Self::SetOptions { key, .. }
            | Self::Insert { key }
            | Self::Remove { key }
            | Self::Replace { key } => key,
        }
    }
}

/// Результат сравнения двух деревьев.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Patch {
    /// Список точечных изменений в порядке обхода.
    pub ops: Vec<Op>,
    /// Ключи узлов, содержимое которых изменилось (для перерисовки их областей).
    pub changed: BTreeSet<&'static str>,
}

impl Patch {
    /// Истина, если изменений нет.
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }
}
