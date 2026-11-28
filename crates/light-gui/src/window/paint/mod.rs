//! Программная отрисовка: прямоугольники, рамки и текст в пиксельный буфер
//! формата RGBA.

/// Цвет в формате `0xRRGGBBAA`.
pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> u32 {
    ((r as u32) << 24) | ((g as u32) << 16) | ((b as u32) << 8) | a as u32
}

/// Непрозрачный цвет в формате `0xRRGGBBAA`.
pub const fn rgb(r: u8, g: u8, b: u8) -> u32 {
    rgba(r, g, b, 0xFF)
}

/// Основной текст.
pub const COLOR_TEXT: u32 = rgb(0x1F, 0x24, 0x2B);
/// Приглушённый текст (placeholder).
pub const COLOR_MUTED: u32 = rgb(0x8A, 0x92, 0x9E);
/// Фон титульной полосы.
pub const TITLEBAR_BG: u32 = rgb(0xDF, 0xE4, 0xEB);
/// Нижняя граница титульной полосы.
pub const TITLEBAR_BORDER: u32 = rgb(0xC4, 0xCB, 0xD6);
/// Заливка кнопки.
pub const BUTTON_BG: u32 = rgb(0xE4, 0xE8, 0xEF);
/// Заливка кнопки под курсором.
pub const BUTTON_BG_HOVER: u32 = rgb(0xD3, 0xDA, 0xE4);
/// Заливка заблокированной кнопки.
pub const BUTTON_BG_DISABLED: u32 = rgb(0xEE, 0xF0, 0xF3);
/// Рамка кнопки.
pub const BUTTON_BORDER: u32 = rgb(0xC2, 0xC9, 0xD4);
/// Подпись заблокированной кнопки.
pub const BUTTON_LABEL_DISABLED: u32 = rgb(0xA6, 0xAD, 0xB8);
/// Заливка текстового поля.
pub const FIELD_BG: u32 = rgb(0xFF, 0xFF, 0xFF);
/// Рамка текстового поля.
pub const FIELD_BORDER: u32 = rgb(0xB8, 0xC0, 0xCC);
/// Рамка текстового поля с фокусом.
pub const FIELD_BORDER_FOCUS: u32 = rgb(0x3B, 0x82, 0xF6);
/// Каретка ввода.
pub const COLOR_CARET: u32 = rgb(0x1F, 0x24, 0x2B);
/// Подсветка выделенного текста.
pub const SELECTION_BG: u32 = rgb(0xB6, 0xD4, 0xFE);
/// Индикатор изменения размера поля.
pub const RESIZE_GRIP: u32 = rgb(0x9A, 0xA2, 0xAE);
/// Заливка флажка/переключателя.
pub const FORM_BG: u32 = rgb(0xFF, 0xFF, 0xFF);
/// Акцент флажка/переключателя/списка.
pub const FORM_ACCENT: u32 = rgb(0x3B, 0x82, 0xF6);
/// Рамка поля, не прошедшего валидацию.
pub const FIELD_INVALID: u32 = rgb(0xD9, 0x53, 0x4F);
/// Цвет индикатора обязательного поля (`*`).
pub const REQUIRED_COLOR: u32 = rgb(0xD9, 0x53, 0x4F);
/// Цвет кольца фокуса клавиатурной навигации.
pub const FOCUS_RING: u32 = rgb(0x4A, 0x90, 0xD9);
/// Дорожка полосы прокрутки.
pub const SCROLLBAR_TRACK: u32 = rgba(0x5B, 0x64, 0x72, 0x28);
/// Ползунок полосы прокрутки.
pub const SCROLLBAR_THUMB: u32 = rgba(0x5B, 0x64, 0x72, 0x90);
/// Толщина полосы прокрутки (px).
pub const SCROLLBAR_SIZE: f32 = 9.0;
/// Минимальная длина ползунка полосы прокрутки (px).
pub const SCROLLBAR_MIN_THUMB: f32 = 20.0;
/// Толщина кольца фокуса (px).
pub const FOCUS_RING_WIDTH: f32 = 2.0;
/// Отступ кольца фокуса внутрь узла (px).
pub const FOCUS_RING_OFFSET: f32 = 1.0;

/// Ширина контекстного меню.
pub(crate) const MENU_W: f32 = 160.0;
/// Высота строки контекстного меню.
pub(crate) const MENU_ROW_H: f32 = 26.0;
/// Толщина рамки контекстного меню.
pub(crate) const MENU_BORDER: f32 = 1.0;
/// Фон контекстного меню.
const MENU_BG: u32 = rgb(0xFF, 0xFF, 0xFF);
/// Рамка контекстного меню.
const MENU_BORDER_COLOR: u32 = rgb(0xC4, 0xCB, 0xD6);
/// Подсветка пункта под курсором.
const MENU_HOVER: u32 = rgb(0xE3, 0xEC, 0xFB);
/// Доступный пункт меню.
const MENU_TEXT: u32 = rgb(0x1F, 0x24, 0x2B);
/// Заблокированный пункт меню.
const MENU_TEXT_DISABLED: u32 = rgb(0xA6, 0xAD, 0xB8);

pub(crate) use crate::window::font::measure;

mod canvas;
mod forms;
mod menu;
mod render;
mod selects;

#[cfg(test)]
mod tests;

pub(crate) use canvas::Canvas;
pub use forms::Interaction;
#[cfg(feature = "elements-select")]
pub(crate) use forms::select_popup;
pub(crate) use menu::draw_context_menu;
pub use render::render;
