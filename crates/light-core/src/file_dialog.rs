//! Нативный диалог выбора файла (Cargo-фича `file-dialog`).

use std::path::PathBuf;

/// Открывает нативный диалог выбора PNG-изображения.
///
/// Возвращает `None`, если пользователь отменил выбор. Диалог блокирует
/// вызывающий поток до закрытия — это нормально для обработчика клика.
pub fn pick_image_file() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("PNG image", &["png"])
        .pick_file()
}
