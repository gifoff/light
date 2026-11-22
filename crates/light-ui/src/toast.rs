//! Виджет `Toast` (скелет, 0.21.0): временное уведомление.
//!
//! `show_toast` показывает прямоугольник с текстом в правом верхнем углу и
//! скрывает его через 3 секунды. Скрытие — фоновая std-задача (`std::thread`).
//! В полной версии задача отправит `post_update` для перерисовки UI; здесь
//! достаточно сбросить атомарный флаг видимости.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Глобальный флаг «активен ли toast» (скелет: один активный toast).
static TOAST_VISIBLE: AtomicBool = AtomicBool::new(false);

/// Включает видимость toast (UI-поток: `render` проверяет этот флаг).
pub(crate) fn set_visible(visible: bool) {
    TOAST_VISIBLE.store(visible, Ordering::Relaxed);
}

/// Возвращает текущий флаг видимости toast.
pub fn is_visible() -> bool {
    TOAST_VISIBLE.load(Ordering::Relaxed)
}

/// Показывает toast с текстом `text` на 3 секунды.
///
/// Отдельная фоновая задача засыпает 3 секунды и сбрасывает флаг
/// видимости. В полной версии задача отправит `post_update` для
/// перерисовки UI; здесь достаточно атомарного флага.
pub fn show_toast(text: &str) {
    let _ = text; // подпись toast (скелет не рендерит её)
    set_visible(true);

    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(3));
        set_visible(false);
    });
}
