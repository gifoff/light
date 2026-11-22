//! # Light UI (0.21.0)
//!
//! Готовые виджеты поверх `light-fw-core` + `light-gui`: Tooltip, Toast,
//! Modal, Progress. В этой версии — скелет: `Tooltip` (кастомный элемент
//! `<tooltip>` через `linkme`) и `Toast` (`show_toast` с автоскрытием через
//! 3 секунды).

pub mod toast;
pub mod tooltip;

pub fn add(left: u64, right: u64) -> u64 {
    left + right
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
